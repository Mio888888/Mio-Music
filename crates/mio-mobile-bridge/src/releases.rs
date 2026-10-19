//! Original-runtime mobile update settings and bounded off-owner checks.
use crate::{api::*, catalog::SnapshotPermit, discovery::Worker, runtime};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

// Keep the gate until an in-flight blocking request ends, even after close.
static REQUEST: Mutex<Option<Instant>> = Mutex::new(None);
pub(crate) struct Session {
    lease: runtime::RuntimeLease,
    current_version: String,
    closed: Arc<AtomicBool>,
    writer: Mutex<()>,
    _permit: SnapshotPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin(current_version: String) -> Result<ReleaseSession, String> {
    mio_core::mobile_release::installed_version(&current_version)?;
    let permit = SnapshotPermit::acquire()?;
    let (_, lease) = runtime::inspect_with_lease(|_| ())?;
    Ok(ReleaseSession {
        session: Session {
            lease,
            current_version,
            closed: Arc::new(AtomicBool::new(false)),
            writer: Mutex::new(()),
            _permit: permit,
        },
    })
}
impl Session {
    fn ensure_open(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()
    }
    pub(crate) fn state(&self) -> Result<bool, String> {
        self.ensure_open()?;
        let enabled = self
            .lease
            .inspect(|core| core.preferences.app.auto_update)?;
        self.ensure_open()?;
        Ok(enabled)
    }
    pub(crate) fn save(&self, expected: bool, enabled: bool) -> Result<bool, String> {
        let _writer = self.writer.try_lock().map_err(|_| "busy")?;
        self.ensure_open()?;
        self.lease.execute(runtime::Action::AutoUpdate(
            expected,
            enabled,
            self.closed.clone(),
        ))?;
        // Admitted owner writes return the actual result, even after close.
        Ok(enabled)
    }
    pub(crate) fn check(&self, startup: bool) -> Result<Option<MobileRelease>, String> {
        let _writer = self.writer.try_lock().map_err(|_| "busy")?;
        self.ensure_open()?;
        if startup && !self.state()? {
            return Ok(None);
        }
        let _worker = Worker::acquire()?;
        let mut gate = REQUEST.try_lock().map_err(|_| "busy")?;
        if gate.is_some_and(|last| last.elapsed() < Duration::from_secs(60)) {
            return Err("release_cooldown".into());
        }
        self.ensure_open()?;
        *gate = Some(Instant::now());
        let version = mio_core::mobile_release::installed_version(&self.current_version)?;
        let result = mio_core::mobile_release::check(&version, &|| self.ensure_open())?;
        self.ensure_open()?;
        Ok(Some(MobileRelease {
            version: result.version,
            tag: result.tag,
            notes: result.notes,
            notes_truncated: result.notes_truncated,
            newer: result.newer,
        }))
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
}
