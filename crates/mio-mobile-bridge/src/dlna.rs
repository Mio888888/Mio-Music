//! Original-runtime, bounded Android renderer discovery. Native lock lifetime follows workers.
use crate::{api::*, catalog::SnapshotPermit, discovery::Worker, runtime};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
static SEARCH: Mutex<Option<Instant>> = Mutex::new(None);
static TOKENS: AtomicU64 = AtomicU64::new(1);
struct Multicast {
    token: u64,
    released: bool,
}
impl Multicast {
    fn acquire() -> Result<Self, String> {
        let token = TOKENS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                (value < i64::MAX as u64).then_some(value + 1)
            })
            .map_err(|_| "capacity_exceeded: DLNA")?;
        native(token, true)?;
        Ok(Self {
            token,
            released: false,
        })
    }
    fn release(&mut self) -> Result<(), String> {
        native(self.token, false)?;
        self.released = true;
        Ok(())
    }
}
impl Drop for Multicast {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.release();
        }
    }
}
fn native(token: u64, acquire: bool) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return crate::android::multicast(token, acquire);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (token, acquire);
        Err("dlna_unavailable".into())
    }
}
struct Devices {
    revision: u64,
    rows: Vec<mio_core::dlna::Renderer>,
}
pub(crate) struct Session {
    lease: runtime::RuntimeLease,
    closed: Arc<AtomicBool>,
    controller: mio_core::dlna::Controller,
    devices: Mutex<Devices>,
    writer: Mutex<()>,
    _permit: SnapshotPermit,
}
pub(crate) fn begin() -> Result<DlnaSession, String> {
    if !cfg!(target_os = "android") {
        return Err("dlna_unavailable".into());
    }
    let permit = SnapshotPermit::acquire()?;
    let (_, lease) = runtime::inspect_with_lease(|_| ())?;
    Ok(DlnaSession {
        session: Session {
            lease,
            closed: Arc::new(AtomicBool::new(false)),
            controller: mio_core::dlna::Controller::default(),
            devices: Mutex::new(Devices {
                revision: 0,
                rows: Vec::new(),
            }),
            writer: Mutex::new(()),
            _permit: permit,
        },
    })
}
impl Session {
    fn check(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()
    }
    pub(crate) fn search(&self) -> Result<DlnaDevices, String> {
        let _writer = self.writer.try_lock().map_err(|_| "busy")?;
        self.check()?;
        let _worker = Worker::acquire()?;
        let mut gate = SEARCH.try_lock().map_err(|_| "busy: DLNA discovery")?;
        if gate.is_some_and(|at| at.elapsed() < Duration::from_secs(3)) {
            return Err("dlna_search_cooldown".into());
        }
        let mut multicast = Multicast::acquire()?;
        self.check()?;
        let result = self.controller.search_with_cancel(self.closed.clone());
        *gate = Some(Instant::now());
        // All SSDP and description workers have joined before releasing the native lock.
        multicast.release()?;
        let rows = result?;
        self.check()?;
        if rows.len() > 65 {
            return Err("capacity_exceeded: DLNA devices".into());
        }
        let mut devices = self.devices.lock().map_err(|_| "dlna_unavailable")?;
        devices.revision = devices
            .revision
            .checked_add(1)
            .ok_or("capacity_exceeded: DLNA revision")?;
        devices.rows = rows
            .iter()
            .map(|row| {
                let renderer = self.controller.renderer(&row.location)?;
                if renderer.device() != row {
                    return Err("dlna_devices_changed".into());
                }
                Ok(renderer)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let result = DlnaDevices {
            revision: devices.revision,
            rows: devices
                .rows
                .iter()
                .enumerate()
                .map(|(index, device)| DlnaDevice {
                    index: index as u32,
                    name: device.device().name.chars().take(256).collect(),
                    address: device.device().address.clone(),
                })
                .collect(),
        };
        self.check()?;
        Ok(result)
    }
    pub(crate) fn select(&self, revision: u64, index: u32) -> Result<DlnaCastState, String> {
        let _writer = self.writer.try_lock().map_err(|_| "busy")?;
        self.check()?;
        let devices = self.devices.lock().map_err(|_| "dlna_unavailable")?;
        if devices.revision != revision {
            return Err("dlna_devices_changed".into());
        }
        let row = devices
            .rows
            .get(index as usize)
            .ok_or("invalid_input: DLNA device")?;
        let renderer = row.clone();
        drop(devices);
        self.check()?;
        crate::dlna_cast::start(self.lease.clone(), renderer, self.closed.clone())
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
