//! Foreground host authorization and original-owner playback pause leases.
use crate::{api::MicrophoneProgress, runtime};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(1);
static ACTIVE: Mutex<Option<Arc<Control>>> = Mutex::new(None);

pub(crate) struct Control {
    pub(crate) token: u64,
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) progress: Arc<mio_core::recognition::Progress>,
    capturing: AtomicBool,
    pub(crate) resume_gate: Mutex<bool>,
    restore_failed: AtomicBool,
}
impl Control {
    pub(crate) fn new(cancelled: Arc<AtomicBool>) -> Result<Arc<Self>, String> {
        let token = SEQUENCE
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                (value < i64::MAX as u64).then_some(value + 1)
            })
            .map_err(|_| "capacity_exceeded: microphone sessions")?;
        Ok(Arc::new(Self {
            token,
            cancelled,
            progress: Arc::default(),
            capturing: AtomicBool::new(false),
            resume_gate: Mutex::new(true),
            restore_failed: AtomicBool::new(false),
        }))
    }
    pub(crate) fn view(&self) -> MicrophoneProgress {
        MicrophoneProgress {
            seconds: self.progress.seconds.load(Ordering::Acquire).min(15),
            level: self.progress.level.load(Ordering::Acquire).min(100),
            capturing: self.capturing.load(Ordering::Acquire),
            restore_failed: self.restore_failed.load(Ordering::Acquire),
        }
    }
}
/// Called only by the native foreground/lifecycle gate for its exact issued token.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn cancel(token: u64) {
    let active = ACTIVE.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(control) = active.as_ref().filter(|control| control.token == token) {
        *control
            .resume_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = false;
        control.cancelled.store(true, Ordering::Release);
    }
}
pub(crate) struct Binding(u64);
impl Binding {
    pub(crate) fn new(control: Arc<Control>) -> Result<Self, String> {
        let mut active = ACTIVE.lock().map_err(|_| "microphone_unavailable")?;
        if active.is_some() {
            return Err("busy: microphone".into());
        }
        let token = control.token;
        *active = Some(control);
        Ok(Self(token))
    }
}
impl Drop for Binding {
    fn drop(&mut self) {
        let mut active = ACTIVE.lock().unwrap_or_else(|error| error.into_inner());
        if active
            .as_ref()
            .is_some_and(|control| control.token == self.0)
        {
            *active = None;
        }
    }
}
#[derive(Clone)]
pub(crate) struct Request {
    pub(crate) control: Arc<Control>,
    pub(crate) lease: runtime::RuntimeLease,
    pub(crate) revision: u64,
}
impl Request {
    pub(crate) fn check(&self) -> Result<(), String> {
        self.lease.ensure_open()?;
        if self.control.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if self.lease.source_revision() != self.revision {
            return Err("source_changed".into());
        }
        Ok(())
    }
}
pub(crate) struct CaptureLease(Request);
impl CaptureLease {
    pub(crate) fn acquire(request: Request) -> Result<Self, String> {
        request.check()?;
        native(request.control.token, true)?;
        // Native release must happen even if owner admission or pause fails.
        let lease = Self(request);
        lease
            .0
            .lease
            .execute(runtime::Action::BeginCapture(lease.0.clone()))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            lease.0.check()?;
            let paused = lease.0.lease.inspect(|core| {
                !matches!(
                    core.player.status,
                    mio_core::playback::Status::Playing | mio_core::playback::Status::Loading
                )
            })?;
            if paused {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err("microphone_pause_timeout".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        #[cfg(target_os = "ios")]
        host_operation(0, lease.0.control.token, 2)?;
        lease.0.check()?;
        lease.0.control.capturing.store(true, Ordering::Release);
        Ok(lease)
    }
}
impl Drop for CaptureLease {
    fn drop(&mut self) {
        self.0.control.capturing.store(false, Ordering::Release);
        self.0.control.progress.level.store(0, Ordering::Release);
        let restored = native(self.0.control.token, false).is_ok();
        let finished = self
            .0
            .lease
            .execute(runtime::Action::EndCapture(self.0.control.token, restored));
        if !restored || (finished.is_err() && self.0.lease.ensure_open().is_ok()) {
            self.0.control.restore_failed.store(true, Ordering::Release);
        }
    }
}
fn native(token: u64, begin: bool) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return crate::android::recording(token, begin);
    }
    #[cfg(target_os = "ios")]
    {
        return host_operation(0, token, if begin { 1 } else { 3 });
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = (token, begin);
        Err("microphone_unavailable".into())
    }
}

#[cfg(target_os = "ios")]
type HostOperation = extern "C" fn(u64, u64, u32) -> i32;
#[cfg(target_os = "ios")]
static HOST: Mutex<Option<HostOperation>> = Mutex::new(None);
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn mio_media_register_microphone(callback: Option<HostOperation>) -> i32 {
    let Ok(mut slot) = HOST.lock() else {
        return 0;
    };
    *slot = callback;
    1
}
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn mio_media_cancel_microphone(token: u64) {
    cancel(token);
}
#[cfg(target_os = "ios")]
pub(crate) fn host_operation(runtime_id: u64, token: u64, operation: u32) -> Result<(), String> {
    let callback = *HOST.lock().map_err(|_| "microphone_unavailable")?;
    if callback.is_some_and(|f| f(runtime_id, token, operation) != 0) {
        Ok(())
    } else {
        Err("microphone_host_failed".into())
    }
}
