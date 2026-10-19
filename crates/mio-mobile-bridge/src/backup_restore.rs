//! Original-owner restore preparation and explicit, outcome-cached commits.
use crate::{api::*, catalog::SnapshotPermit, discovery::Worker, runtime};
use mio_core::{
    backup,
    backup_restore::PreparedRestore,
    library::LibraryCursor,
    online::{BackupPluginSnapshot, Online, PreparedBackupPlugins},
};
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
static CLOUD_OPEN: AtomicBool = AtomicBool::new(false);
pub(crate) struct CloudPermit;
impl CloudPermit {
    pub(crate) fn acquire() -> Result<Self, String> {
        CLOUD_OPEN
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "busy: another cloud backup session is open".into())
    }
}
impl Drop for CloudPermit {
    fn drop(&mut self) {
        CLOUD_OPEN.store(false, Ordering::Release);
    }
}
struct State {
    config: Option<backup::S3Config>,
    result: Option<Result<PreparedRestore, String>>,
    plugins: Option<BackupPluginSnapshot>,
    outcome: Option<(bool, Result<S3RestoreOutcome, String>)>,
}
pub(crate) struct Session {
    lease: runtime::RuntimeLease,
    source_revision: u64,
    cursor: Arc<Mutex<LibraryCursor>>,
    expected_settings: Value,
    expected_online: Option<Arc<Online>>,
    state: Mutex<State>,
    closed: Arc<AtomicBool>,
    _permit: SnapshotPermit,
    _restore: CloudPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin(config: S3RestoreConfig) -> Result<S3RestorePreparation, String> {
    for value in [
        &config.endpoint,
        &config.region,
        &config.access_key_id,
        &config.secret_access_key,
        &config.bucket,
    ] {
        if value.len() > 4096 || value.chars().any(char::is_control) {
            return Err("invalid_input: S3 configuration".into());
        }
    }
    let restore = CloudPermit::acquire()?;
    let permit = SnapshotPermit::acquire()?;
    let ((store, expected_settings, expected_online), lease) =
        runtime::inspect_with_lease(|core| {
            (
                core.store.clone(),
                core.backup_settings_snapshot(),
                core.online.clone(),
            )
        })?;
    let path = store.library_path();
    let source_revision = lease.source_revision();
    let _worker = Worker::acquire()?;
    let cursor = LibraryCursor::open_mobile(&path).map_err(crate::catalog::browse_error)?;
    let plugins = BackupPluginSnapshot::capture(&store)?;
    lease.ensure_open()?;
    Ok(S3RestorePreparation {
        session: Session {
            lease,
            source_revision,
            cursor: Arc::new(Mutex::new(cursor)),
            expected_settings,
            expected_online,
            closed: Arc::new(AtomicBool::new(false)),
            state: Mutex::new(State {
                config: Some(backup::S3Config {
                    endpoint: config.endpoint,
                    region: config.region,
                    access_key_id: config.access_key_id,
                    secret_access_key: config.secret_access_key,
                    bucket: config.bucket,
                    path_style: config.path_style,
                }),
                result: None,
                plugins: Some(plugins),
                outcome: None,
            }),
            _permit: permit,
            _restore: restore,
        },
    })
}
pub(crate) fn preview(prepared: &PreparedRestore) -> S3RestorePreview {
    let summary = prepared.summary();
    S3RestorePreview {
        timestamp: prepared.timestamp().to_owned(),
        playlists: summary.playlists,
        songs: summary.songs,
        local_references: summary.local_references,
        plugins: prepared.plugins().len() as u32,
    }
}
impl Session {
    pub(crate) fn commit(&self, overwrite: bool) -> Result<S3RestoreOutcome, String> {
        let mut state = self.state.try_lock().map_err(|_| "busy: backup restore")?;
        // Once attempted, return the actual cached result even after cancellation
        // or our own successful writes invalidated the original cursor/source.
        if let Some((mode, outcome)) = &state.outcome {
            return if *mode == overwrite {
                outcome.clone()
            } else {
                Err("completed: restore mode cannot change".into())
            };
        }
        let _worker = Worker::acquire()?;
        self.check()?;
        self.cursor
            .lock()
            .map_err(|_| "unavailable: backup cursor")?
            .check()
            .map_err(crate::catalog::browse_error)?;
        if state.result.is_none() {
            return Err("not_ready: load backup preview first".into());
        }
        let result = (|| {
            let prepared = state.result.take().ok_or("not_ready: backup preview")??;
            let plugins = state
                .plugins
                .take()
                .ok_or("completed: plugin snapshot consumed")?
                .prepare(prepared.plugins().iter().map(|p| p.code()), &|| {
                    self.check()
                })?;
            self.check()?;
            let actual = Arc::new(Mutex::new(None));
            let sent = self
                .lease
                .execute(runtime::Action::RestoreBackup(PreparedCommit {
                    prepared,
                    plugins,
                    overwrite,
                    cursor: self.cursor.clone(),
                    expected_settings: self.expected_settings.clone(),
                    expected_online: self.expected_online.clone(),
                    closed: self.closed.clone(),
                    actual: actual.clone(),
                }));
            if let Some(outcome) = actual
                .lock()
                .map_err(|_| "unavailable: restore result")?
                .take()
            {
                return Ok(outcome);
            }
            sent?;
            Err("unavailable: restore result missing".into())
        })();
        state.outcome = Some((overwrite, result.clone()));
        result
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut state) = self.state.try_lock() {
            state.config = None;
            state.plugins = None;
            state.result = None;
        }
    }
    fn check(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()?;
        if self.source_revision != self.lease.source_revision() {
            return Err("source_changed: reopen backup".into());
        }
        Ok(())
    }
    pub(crate) fn load(&self, password: String) -> Result<S3RestorePreview, String> {
        if password.is_empty() || password.len() > 4096 {
            return Err("invalid_input: backup password".into());
        }
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "busy: backup preparation")?;
        if state.outcome.is_some() {
            return Err("completed: backup restore already attempted".into());
        }
        if let Some(result) = &state.result {
            // Read-only previews are not returned to a closed or superseded owner.
            self.check()?;
            self.cursor
                .try_lock()
                .map_err(|_| "busy: backup cursor")?
                .check()
                .map_err(crate::catalog::browse_error)?;
            return result.as_ref().map(preview).map_err(Clone::clone);
        }
        let _worker = Worker::acquire()?;
        let result = (|| {
            self.check()?;
            self.cursor
                .try_lock()
                .map_err(|_| "busy: backup cursor")?
                .check()
                .map_err(crate::catalog::browse_error)?;
            let config = state
                .config
                .take()
                .ok_or("cancelled: backup credentials closed")?;
            let prepared = backup::fetch_latest_prepared(config, &password, &|| self.check())?;
            self.check()?;
            self.cursor
                .try_lock()
                .map_err(|_| "busy: backup cursor")?
                .check()
                .map_err(crate::catalog::browse_error)?;
            Ok(prepared)
        })();
        state.result = Some(result);
        state
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .map(preview)
            .map_err(Clone::clone)
    }
}

pub(crate) struct PreparedCommit {
    prepared: PreparedRestore,
    plugins: PreparedBackupPlugins,
    overwrite: bool,
    cursor: Arc<Mutex<LibraryCursor>>,
    expected_settings: Value,
    expected_online: Option<Arc<Online>>,
    closed: Arc<AtomicBool>,
    actual: Arc<Mutex<Option<S3RestoreOutcome>>>,
}
impl PreparedCommit {
    pub(crate) fn apply(self, core: &mut mio_core::app_core::AppCore) -> Result<(), String> {
        let same_source = match (&core.online, &self.expected_online) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same_source {
            return Err("source_changed: reopen backup".into());
        }
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if core.backup_settings_snapshot() != self.expected_settings {
            return Err("settings_changed: reopen backup".into());
        }
        self.plugins.check(&core.store)?;
        // Reserve the result slot before any persistence so recording the actual
        // outcome cannot fail after an admitted commit.
        let mut actual = self
            .actual
            .lock()
            .map_err(|_| "unavailable: restore result")?;
        let library = self
            .cursor
            .lock()
            .map_err(|_| "unavailable: backup cursor")?
            .restore_prepared(self.prepared.library(), self.overwrite, || {
                !self.closed.load(Ordering::Acquire)
            })
            .map_err(crate::catalog::browse_error)?;
        let mut outcome = S3RestoreOutcome {
            playlists_added: library.playlists_added,
            songs_added: library.songs_added,
            songs_skipped: library.songs_skipped,
            plugins_applied: false,
            plugins_added: 0,
            plugins_skipped: 0,
            settings_applied: false,
            warnings: Vec::new(),
        };
        if self.closed.load(Ordering::Acquire) {
            outcome.warnings.push("cancelled_after_library".into());
        } else {
            match self.plugins.commit(&core.store) {
                Ok((added, skipped)) => {
                    outcome.plugins_applied = true;
                    outcome.plugins_added = added;
                    outcome.plugins_skipped = skipped;
                    if skipped > 0 {
                        outcome.warnings.push("plugins_skipped".into());
                    }
                }
                Err(_) => outcome.warnings.push("plugins_not_saved".into()),
            }
            if self.closed.load(Ordering::Acquire) {
                outcome.warnings.push("cancelled_before_settings".into());
            } else {
                let mode = if self.overwrite {
                    backup::Mode::Overwrite
                } else {
                    backup::Mode::Merge
                };
                match core.restore_backup_settings(
                    &self.expected_settings,
                    self.prepared.settings(),
                    mode,
                ) {
                    Ok(()) => outcome.settings_applied = true,
                    Err(_) => outcome.warnings.push("settings_not_saved".into()),
                }
            }
        }
        *actual = Some(outcome);
        Ok(())
    }
}
