//! Immutable local export and outcome-cached cloud upload, tied to its owner.
use crate::{
    api::*, backup_restore::CloudPermit, catalog::SnapshotPermit, discovery::Worker, runtime,
};
use mio_core::{
    backup,
    backup_restore::PreparedRestore,
    library::LibraryCursor,
    online::{BackupPluginSnapshot, Online},
    storage::Storage,
};
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

fn dto(s: &backup::Saved) -> S3SavedConnection {
    S3SavedConnection {
        endpoint: s.endpoint.clone(),
        region: s.region.clone(),
        access_key_id: s.access_key_id.clone(),
        bucket: s.bucket.clone(),
        path_style: s.path_style,
        max_backups: s.max_backups,
        last_backup_time: s.last_backup_time.clone(),
    }
}
fn saved(s: S3SavedConnection) -> backup::Saved {
    backup::Saved {
        endpoint: s.endpoint,
        region: s.region,
        access_key_id: s.access_key_id,
        bucket: s.bucket,
        path_style: s.path_style,
        max_backups: s.max_backups,
        last_backup_time: None,
    }
}
pub(crate) struct SettingsSession {
    lease: runtime::RuntimeLease,
    saved: Arc<Mutex<backup::MobileSaved>>,
    closed: Arc<AtomicBool>,
    _permit: SnapshotPermit,
}
impl Drop for SettingsSession {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin_settings() -> Result<S3SettingsSession, String> {
    let permit = SnapshotPermit::acquire()?;
    let (store, lease) = runtime::inspect_with_lease(|core| core.store.clone())?;
    let _worker = Worker::acquire()?;
    let original = backup::MobileSaved::capture(&store)?;
    lease.ensure_open()?;
    Ok(S3SettingsSession {
        session: SettingsSession {
            lease,
            saved: Arc::new(Mutex::new(original)),
            closed: Arc::new(AtomicBool::new(false)),
            _permit: permit,
        },
    })
}
impl SettingsSession {
    pub(crate) fn state(&self) -> Result<S3SavedConnection, String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()?;
        Ok(dto(self
            .saved
            .lock()
            .map_err(|_| "unavailable: S3 settings")?
            .value()))
    }
    pub(crate) fn save(&self, next: S3SavedConnection) -> Result<S3SavedConnection, String> {
        let actual = Arc::new(Mutex::new(None));
        let sent = self.lease.execute(runtime::Action::SaveS3(SaveSettings {
            saved: self.saved.clone(),
            next: saved(next),
            uploaded: None,
            closed: Some(self.closed.clone()),
            actual: actual.clone(),
        }));
        let value = actual
            .lock()
            .map_err(|_| "unavailable: S3 settings result")?
            .take();
        if let Some(value) = value {
            return Ok(value);
        }
        sent?;
        Err("unavailable: S3 settings result missing".into())
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
}
pub(crate) struct SaveSettings {
    saved: Arc<Mutex<backup::MobileSaved>>,
    next: backup::Saved,
    uploaded: Option<String>,
    closed: Option<Arc<AtomicBool>>,
    actual: Arc<Mutex<Option<S3SavedConnection>>>,
}
impl SaveSettings {
    pub(crate) fn apply(self, core: &mut mio_core::app_core::AppCore) -> Result<(), String> {
        if self
            .closed
            .as_ref()
            .is_some_and(|v| v.load(Ordering::Acquire))
        {
            return Err("cancelled".into());
        }
        let mut actual = self
            .actual
            .lock()
            .map_err(|_| "unavailable: S3 settings result")?;
        let mut original = self.saved.lock().map_err(|_| "unavailable: S3 settings")?;
        original.save(&core.store, self.next, self.uploaded.as_deref())?;
        *actual = Some(dto(original.value()));
        Ok(())
    }
}
struct State {
    config: Option<backup::S3Config>,
    plugins: Option<BackupPluginSnapshot>,
    prepared: Option<Result<PreparedRestore, String>>,
    outcome: Option<Result<S3UploadOutcome, String>>,
}
pub(crate) struct UploadSession {
    lease: runtime::RuntimeLease,
    source_revision: u64,
    store: Storage,
    cursor: Mutex<LibraryCursor>,
    settings: Value,
    online: Option<Arc<Online>>,
    saved: Arc<Mutex<backup::MobileSaved>>,
    max_backups: u32,
    closed: AtomicBool,
    state: Mutex<State>,
    _permit: SnapshotPermit,
    _cloud: CloudPermit,
}
impl Drop for UploadSession {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin_upload(
    config: S3RestoreConfig,
    max_backups: u32,
) -> Result<S3UploadPreparation, String> {
    for v in [
        &config.endpoint,
        &config.region,
        &config.access_key_id,
        &config.secret_access_key,
        &config.bucket,
    ] {
        if v.len() > 4096 || v.chars().any(char::is_control) {
            return Err("invalid_input: S3 configuration".into());
        }
    }
    if !(1..=100).contains(&max_backups) {
        return Err("invalid_input: retention".into());
    }
    let cloud = CloudPermit::acquire()?;
    let permit = SnapshotPermit::acquire()?;
    let ((store, settings, online), lease) = runtime::inspect_with_lease(|core| {
        (
            core.store.clone(),
            core.backup_settings_snapshot(),
            core.online.clone(),
        )
    })?;
    let _worker = Worker::acquire()?;
    let cursor =
        LibraryCursor::open_mobile(&store.library_path()).map_err(crate::catalog::browse_error)?;
    let plugins = BackupPluginSnapshot::capture(&store)?;
    let original = backup::MobileSaved::capture(&store)?;
    lease.ensure_open()?;
    Ok(S3UploadPreparation {
        session: UploadSession {
            source_revision: lease.source_revision(),
            lease,
            store,
            cursor: Mutex::new(cursor),
            settings,
            online,
            saved: Arc::new(Mutex::new(original)),
            max_backups,
            closed: AtomicBool::new(false),
            state: Mutex::new(State {
                config: Some(backup::S3Config {
                    endpoint: config.endpoint,
                    region: config.region,
                    access_key_id: config.access_key_id,
                    secret_access_key: config.secret_access_key,
                    bucket: config.bucket,
                    path_style: config.path_style,
                }),
                plugins: Some(plugins),
                prepared: None,
                outcome: None,
            }),
            _permit: permit,
            _cloud: cloud,
        },
    })
}
impl UploadSession {
    fn check(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()?;
        if self.lease.source_revision() != self.source_revision {
            return Err("source_changed: reopen backup".into());
        }
        Ok(())
    }
    fn check_snapshot(&self, plugins: &BackupPluginSnapshot) -> Result<(), String> {
        self.check()?;
        self.cursor
            .lock()
            .map_err(|_| "unavailable: backup cursor")?
            .check()
            .map_err(crate::catalog::browse_error)?;
        plugins.check(&self.store)?;
        let settings = self.settings.clone();
        let online = self.online.clone();
        let same = self.lease.inspect(move |core| {
            let same = match (&core.online, &online) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            same && core.backup_settings_snapshot() == settings
        })?;
        if !same {
            return Err("source_changed: reopen backup".into());
        }
        self.check()
    }
    pub(crate) fn load(&self) -> Result<S3RestorePreview, String> {
        let mut state = self.state.try_lock().map_err(|_| "busy: backup upload")?;
        if state.outcome.is_some() {
            return Err("completed: upload already attempted".into());
        }
        let _worker = Worker::acquire()?;
        let plugins = state.plugins.as_ref().ok_or("cancelled")?;
        self.check_snapshot(plugins)?;
        if state.prepared.is_none() {
            let result = (|| {
                let playlists = self
                    .cursor
                    .lock()
                    .map_err(|_| "unavailable: backup cursor")?
                    .export_mobile_backup(&|| self.check())
                    .map_err(crate::catalog::browse_error)?;
                let prepared = PreparedRestore::from_payload(
                    backup::Payload {
                        playlists,
                        settings: self.settings.clone(),
                        plugins: plugins.backup_values(),
                    },
                    &|| self.check(),
                )?;
                // Check escaped document size before presenting the confirmation.
                drop(prepared.document_bytes(&|| self.check())?);
                self.check_snapshot(plugins)?;
                Ok(prepared)
            })();
            state.prepared = Some(result);
        }
        state
            .prepared
            .as_ref()
            .unwrap()
            .as_ref()
            .map(crate::backup_restore::preview)
            .map_err(Clone::clone)
    }
    pub(crate) fn upload(&self, password: String) -> Result<S3UploadOutcome, String> {
        let mut state = self.state.try_lock().map_err(|_| "busy: backup upload")?;
        // Actual outcome wins over close/revision changes; repeated confirmation
        // must never issue another PUT or retention DELETE.
        if let Some(outcome) = &state.outcome {
            return outcome.clone();
        }
        let _worker = Worker::acquire()?;
        self.check()?;
        if state.prepared.is_none() {
            return Err("not_ready: prepare backup first".into());
        }
        let result = (|| {
            self.check_snapshot(state.plugins.as_ref().ok_or("cancelled")?)?;
            let config = state.config.take().ok_or("cancelled")?;
            let next = backup::Saved {
                endpoint: config.endpoint.clone(),
                region: config.region.clone(),
                access_key_id: config.access_key_id.clone(),
                bucket: config.bucket.clone(),
                max_backups: self.max_backups,
                path_style: config.path_style,
                last_backup_time: None,
            };
            let prepared = state
                .prepared
                .as_ref()
                .ok_or("not_ready")?
                .as_ref()
                .map_err(Clone::clone)?;
            let mut actual =
                backup::upload_prepared(config, &password, self.max_backups, prepared, &|| {
                    self.check()
                })?;
            // Confirmed upload survives close; saving its nonsecret connection
            // metadata still targets the original owner and original file token.
            if self
                .lease
                .execute(runtime::Action::SaveS3(SaveSettings {
                    saved: self.saved.clone(),
                    next,
                    uploaded: Some(actual.timestamp.clone()),
                    closed: None,
                    actual: Arc::new(Mutex::new(None)),
                }))
                .is_err()
            {
                actual.warnings.push("connection_not_saved".into());
            }
            Ok(S3UploadOutcome {
                timestamp: actual.timestamp,
                deleted: actual.deleted,
                warnings: actual.warnings,
            })
        })();
        state.config = None;
        state.plugins = None;
        state.prepared = None;
        state.outcome = Some(result.clone());
        result
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut state) = self.state.try_lock() {
            state.config = None;
            state.plugins = None;
            state.prepared = None;
        }
    }
}
