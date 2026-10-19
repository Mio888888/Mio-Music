//! File and captured-queue playlist imports; no host path crosses the bridge.
use crate::{
    api::*,
    catalog::{SnapshotPermit, playlist_info},
    discovery::Worker,
    runtime,
};
use mio_core::library;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_SONGS: usize = 20_000;
#[derive(Default)]
struct State {
    items: Option<Vec<serde_json::Value>>,
    cover: String,
    outcome: Option<(String, String, Result<PlaylistInfo, String>)>,
}
pub(crate) struct Session {
    lease: runtime::RuntimeLease,
    path: PathBuf,
    closed: Arc<AtomicBool>,
    queue: Option<crate::queue::Guard>,
    network: Option<(crate::discovery::Guard, mio_core::network_import::Request)>,
    state: Mutex<State>,
    _permit: SnapshotPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin() -> Result<PlaylistFileImport, String> {
    let permit = SnapshotPermit::acquire()?;
    let (path, lease) = runtime::inspect_with_lease(|core| core.store.library_path())?;
    Ok(PlaylistFileImport {
        session: Session {
            lease,
            path,
            closed: Arc::new(AtomicBool::new(false)),
            queue: None,
            network: None,
            state: Mutex::new(State::default()),
            _permit: permit,
        },
    })
}
pub(crate) fn begin_queue(revision: u64, source: u64) -> Result<PlaylistFileImport, String> {
    let permit = SnapshotPermit::acquire()?;
    let _worker = Worker::acquire()?;
    let (captured, lease) = runtime::inspect_with_lease(move |core| {
        let guard = crate::queue::Guard::capture(core, revision)?;
        if core.player.queue.is_empty() {
            return Err("queue_empty: no songs to save".to_owned());
        }
        // Match the desktop songlist's current-queue import gate. Online rows
        // remain saveable through their own source-bound loaded-song flow.
        if core.player.queue.iter().any(mio_core::online::is_online) {
            return Err("queue_online_unsupported: use the source playlist save action".into());
        }
        let items = core.playlist_queue_snapshot()?;
        Ok::<_, String>((core.store.library_path(), items, guard))
    })?;
    if lease.source_revision() != source {
        return Err("queue_changed: reopen import".into());
    }
    let (path, items, guard) = captured?;
    Ok(PlaylistFileImport {
        session: Session {
            lease,
            path,
            closed: Arc::new(AtomicBool::new(false)),
            queue: Some(guard),
            network: None,
            state: Mutex::new(State {
                items: Some(items),
                outcome: None,
                cover: String::new(),
            }),
            _permit: permit,
        },
    })
}

pub(crate) fn network_sources() -> Result<Vec<DiscoverySource>, String> {
    let (online, lease) = runtime::inspect_with_lease(|core| core.online.clone())?;
    let sources = online.map(|o| o.sources()).unwrap_or_default();
    let mut result = Vec::new();
    for id in ["wy", "tx", "kw", "bd", "kg", "mg"] {
        if let Some(source) = sources.iter().find(|s| s.id == id) {
            if mio_music_sdk::provider_with(id, |c| c.playlist_detail).is_some() {
                if source.name.len() > 1024 {
                    return Err("capacity_exceeded: source name".into());
                }
                result.push(DiscoverySource {
                    id: id.into(),
                    name: source.name.clone(),
                });
            }
        }
    }
    lease.ensure_open()?;
    Ok(result)
}
pub(crate) fn begin_network(source: String, input: String) -> Result<PlaylistFileImport, String> {
    let permit = SnapshotPermit::acquire()?;
    let ((online, path), lease) =
        runtime::inspect_with_lease(|core| (core.online.clone(), core.store.library_path()))?;
    let online = online.ok_or("source_unavailable")?;
    if !online.sources().iter().any(|s| s.id == source) {
        return Err("source_unavailable".into());
    }
    let request =
        mio_core::network_import::Request::prepare(&source, &input).map_err(network_error)?;
    let guard = crate::discovery::Guard::new(online, request.provider(), source);
    guard.check(&lease)?;
    Ok(PlaylistFileImport {
        session: Session {
            lease,
            path,
            closed: Arc::new(AtomicBool::new(false)),
            queue: None,
            network: Some((guard, request)),
            state: Mutex::new(State::default()),
            _permit: permit,
        },
    })
}
fn network_error(error: mio_core::network_import::Error) -> String {
    use mio_core::network_import::Error;
    match error {
        Error::Input => "invalid_input: playlist link or ID",
        Error::Unavailable => "source_unavailable",
        Error::Incomplete => "incomplete_playlist: no songs saved",
        Error::Cancelled => "cancelled",
    }
    .into()
}

impl Session {
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Some((guard, _)) = &self.network {
            guard.cancelled.store(true, Ordering::Release);
        }
        if let Some(guard) = &self.queue {
            guard.close();
        }
    }
    fn check(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()?;
        if let Some((guard, _)) = &self.network {
            guard.check(&self.lease)?;
        }
        if let Some(guard) = &self.queue {
            let guard = guard.clone();
            self.lease.inspect(move |core| guard.validate(core))??;
        }
        Ok(())
    }
    pub(crate) fn prepare_network(&self) -> Result<u32, String> {
        let _worker = Worker::acquire()?;
        self.check()?;
        let (guard, request) = self
            .network
            .as_ref()
            .ok_or("invalid_input: not network import")?;
        let mut state = self.state.try_lock().map_err(|_| "busy: playlist import")?;
        if state.items.is_some() || state.outcome.is_some() {
            return Err("session_used: reopen import".into());
        }
        let (items, cover) = request.read(&guard.cancelled).map_err(network_error)?;
        self.check()?;
        if items.is_empty() {
            return Err("empty_playlist: no songs to save".into());
        }
        let count = items.len() as u32;
        state.items = Some(items);
        state.cover = cover;
        Ok(count)
    }
    pub(crate) fn prepared_count(&self) -> Result<u32, String> {
        self.check()?;
        self.state
            .try_lock()
            .map_err(|_| "busy: playlist import")?
            .items
            .as_ref()
            .map(|items| items.len() as u32)
            .ok_or_else(|| "invalid_input: no prepared playlist".into())
    }
    pub(crate) fn prepare(&self, bytes: Vec<u8>) -> Result<u32, String> {
        if self.network.is_some() || self.queue.is_some() {
            return Err("invalid_input: not file import".into());
        }
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return Err("capacity_exceeded: playlist file must be <=32 MiB".into());
        }
        let _worker = Worker::acquire()?;
        self.check()?;
        let mut state = self.state.try_lock().map_err(|_| "busy: playlist import")?;
        if state.items.is_some() || state.outcome.is_some() {
            return Err("session_used: reopen file import".into());
        }
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|_| "invalid_data: expected a UTF-8 JSON song array")?;
        let serde_json::Value::Array(items) = value else {
            return Err("invalid_data: expected a JSON song array".into());
        };
        if items.len() > MAX_SONGS {
            return Err("capacity_exceeded: playlist supports up to 20000 songs".into());
        }
        for item in &items {
            let obj = item.as_object().ok_or("invalid_data: song object")?;
            let id = match obj.get("songmid") {
                Some(serde_json::Value::String(id)) => id.clone(),
                Some(serde_json::Value::Number(id)) => id.to_string(),
                _ => return Err("invalid_data: song ID".into()),
            };
            if id.is_empty() || id.len() > 4096 || id.chars().any(char::is_control) {
                return Err("invalid_data: song ID".into());
            }
            for key in ["name", "singer", "albumName", "img"] {
                match obj.get(key) {
                    None | Some(serde_json::Value::Null) => {}
                    Some(serde_json::Value::String(text))
                        if text.len() <= 65536 && !text.contains('\0') => {}
                    _ => return Err("invalid_data: song field".into()),
                }
            }
        }
        self.check()?;
        let count = items.len() as u32;
        state.items = Some(items);
        Ok(count)
    }
    pub(crate) fn save(&self, name: String, description: String) -> Result<PlaylistInfo, String> {
        if name.len() > 4096 || description.len() > 16384 {
            return Err("invalid_input: playlist form".into());
        }
        let name = name.trim().to_owned();
        let description = description.trim().to_owned();
        if name.is_empty()
            || name.encode_utf16().count() > 50
            || description.encode_utf16().count() > 200
            || name.contains('\0')
            || description.contains('\0')
        {
            return Err("invalid_input: playlist form".into());
        }
        let mut state = self.state.try_lock().map_err(|_| "busy: playlist import")?;
        if let Some((old_name, old_description, result)) = &state.outcome {
            return if old_name == &name && old_description == &description {
                result.clone()
            } else {
                Err("session_used: reopen file import".into())
            };
        }
        let items = state
            .items
            .as_ref()
            .ok_or("invalid_input: select a file first")?;
        let _worker = Worker::acquire()?;
        self.check()?;
        let result = if let Some((guard, _)) = &self.network {
            library::save_network_playlist_when(
                &self.path,
                &name,
                &description,
                &guard.source,
                Some(&state.cover),
                items,
                || self.check().is_err(),
            )
        } else {
            library::save_file_playlist_when(&self.path, &name, &description, items, || {
                self.check().is_err()
            })
        }
        .map(|(row, _)| playlist_info(row))
        .map_err(|_| "playlist_save_failed: close and reopen before retry".to_owned());
        state.outcome = Some((name, description, result.clone()));
        state.items = None;
        result
    }
}
