//! Original-runtime adapter; wire reception never chooses a replacement owner.
use crate::{
    api::{LanTransferEvent, LanTransferView},
    runtime,
};
use mio_core::{
    lan_transfer::{Control, Server, Upload, View},
    local_music::PreparedImport,
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
static HANDLES: AtomicUsize = AtomicUsize::new(0);
pub(crate) struct Session {
    lease: runtime::RuntimeLease,
    data: PathBuf,
    cache: PathBuf,
    control: Arc<Control>,
    server: Mutex<Option<Server>>,
    started: AtomicBool,
}
pub(crate) fn begin() -> Result<Session, String> {
    if !cfg!(target_os = "android") {
        return Err("unsupported: LAN receiver".into());
    }
    HANDLES
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
            (value < 2).then_some(value + 1)
        })
        .map_err(|_| "busy: LAN sessions")?;
    match runtime::inspect_with_lease(|core| (core.paths.data.clone(), core.paths.cache.clone())) {
        Ok(((data, cache), lease)) => Ok(Session {
            lease,
            data,
            cache,
            control: Arc::new(Control::default()),
            server: Mutex::new(None),
            started: AtomicBool::new(false),
        }),
        Err(error) => {
            HANDLES.fetch_sub(1, Ordering::AcqRel);
            Err(error)
        }
    }
}
impl Session {
    pub(crate) fn close(&self) {
        self.control.close();
    }
    pub(crate) fn finish_close(&self) {
        self.close();
        if let Some(server) = self
            .server
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            server.finish_close();
        }
    }
    pub(crate) fn start(&self, address: String) -> Result<LanTransferView, String> {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err("invalid_input: reopen LAN page".into());
        }
        self.lease.ensure_open()?;
        let lease = self.lease.clone();
        let data = self.data.clone();
        let handler = Arc::new(move |upload, closed: Arc<AtomicBool>| {
            let check = || -> Result<(), String> {
                lease.ensure_open()?;
                if closed.load(Ordering::Acquire) {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            };
            check()?;
            match upload {
                Upload::Audio { path, name } => {
                    let _permit = crate::imports::Worker::acquire()?;
                    let prepared =
                        PreparedImport::prepare_named(&[(path, name)], &data, || check().is_err())
                            .map_err(|_| {
                                if closed.load(Ordering::Acquire) {
                                    "cancelled"
                                } else {
                                    "audio_import_failed"
                                }
                            })?;
                    check()?;
                    lease
                        .execute(runtime::Action::ImportLocal(prepared, closed))
                        .map_err(|error| {
                            if error == "cancelled" {
                                error
                            } else {
                                "audio_import_failed".into()
                            }
                        })?;
                    Ok("audio_imported".into())
                }
                Upload::PluginCode(code) => install(&lease, code, false, closed),
                Upload::PluginUrl(url) => install(&lease, url, true, closed),
            }
        });
        let mut server = self.server.lock().map_err(|_| "lan_unavailable")?;
        let receiver = Server::start(
            self.control.clone(),
            &address,
            &self.cache,
            &self.data,
            handler,
        )?;
        let view = project(receiver.view());
        *server = Some(receiver);
        Ok(view)
    }
    pub(crate) fn state(&self) -> Result<LanTransferView, String> {
        if self.lease.ensure_open().is_err() {
            self.close();
        }
        self.server
            .lock()
            .map_err(|_| "lan_unavailable")?
            .as_ref()
            .map(|server| project(server.view()))
            .ok_or_else(|| "lan_not_started".into())
    }
}
fn install(
    lease: &runtime::RuntimeLease,
    value: String,
    url: bool,
    closed: Arc<AtomicBool>,
) -> Result<String, String> {
    crate::plugins::install_received(lease.clone(), value, url, closed).map_err(|error| {
        if error == "cancelled" {
            error
        } else if error.starts_with("source_changed") {
            "source_changed".into()
        } else {
            "plugin_install_failed".into()
        }
    })?;
    Ok("plugin_installed".into())
}
impl Drop for Session {
    fn drop(&mut self) {
        self.finish_close();
        HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
}
fn project(value: View) -> LanTransferView {
    LanTransferView {
        address: value.address,
        url: value.url,
        code: value.code,
        paired: value.paired,
        closed: value.closed,
        phase: value.phase,
        received_bytes: value.received_bytes,
        total_reserved: value.total_reserved,
        error: value.error,
        events: value
            .events
            .into_iter()
            .map(|event| LanTransferEvent {
                id: event.id,
                name: event.name,
                kind: event.kind,
                success: event.success,
                message: event.message,
            })
            .collect(),
    }
}
