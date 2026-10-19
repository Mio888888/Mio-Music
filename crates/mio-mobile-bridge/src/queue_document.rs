//! Desktop-compatible encrypted queue transfer, fixed to its original owner.
use crate::{api::*, catalog::SnapshotPermit, discovery::Worker, queue::Guard, runtime};
use mio_core::{library, playlist_file};
use std::sync::{Arc, Mutex};
const MAX_BYTES: usize = 64 * 1024 * 1024;
#[derive(Default)]
struct State {
    rows: Option<Vec<library::PlaylistSong>>,
    total: u32,
    outcome: Option<Result<QueueImportResult, String>>,
}
pub(crate) struct Session {
    lease: runtime::RuntimeLease,
    guard: Guard,
    state: Mutex<State>,
    _permit: SnapshotPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin(revision: u64, source: u64) -> Result<QueueDocumentSession, String> {
    let permit = SnapshotPermit::acquire()?;
    let (guard, lease) = runtime::inspect_with_lease(move |core| Guard::capture(core, revision))?;
    if lease.source_revision() != source {
        return Err("queue_changed: reopen transfer".into());
    }
    Ok(QueueDocumentSession {
        session: Session {
            lease,
            guard: guard?,
            state: Mutex::new(State::default()),
            _permit: permit,
        },
    })
}
impl Session {
    pub(crate) fn close(&self) {
        self.guard.close();
    }
    fn check(&self) -> Result<(), String> {
        let guard = self.guard.clone();
        self.lease.inspect(move |core| guard.validate(core))?
    }
    fn encrypted(&self) -> Result<String, String> {
        let guard = self.guard.clone();
        let items = self.lease.inspect(move |core| {
            guard.validate(core)?;
            if core.player.queue.is_empty() {
                return Err("queue_empty".to_owned());
            }
            core.playlist_queue_snapshot()
        })??;
        let text = playlist_file::encrypt(&items).map_err(|_| "export_failed: queue encoding")?;
        if text.len() > MAX_BYTES {
            return Err("capacity_exceeded: queue text".into());
        }
        self.check()?;
        Ok(text)
    }
    pub(crate) fn export_text(&self) -> Result<String, String> {
        let _worker = Worker::acquire()?;
        let _state = self.state.try_lock().map_err(|_| "busy: queue transfer")?;
        self.encrypted()
    }
    pub(crate) fn export_file(&self) -> Result<QueueExportFile, String> {
        let _worker = Worker::acquire()?;
        let _state = self.state.try_lock().map_err(|_| "busy: queue transfer")?;
        let text = self.encrypted()?;
        let bytes = playlist_file::gzip(&text).map_err(|_| "export_failed: queue compression")?;
        if bytes.len() > MAX_BYTES {
            return Err("capacity_exceeded: queue file".into());
        }
        self.check()?;
        Ok(QueueExportFile {
            name: playlist_file::default_file_name(),
            bytes,
        })
    }
    pub(crate) fn prepare_file(&self, bytes: Vec<u8>, extension: String) -> Result<u32, String> {
        if bytes.is_empty()
            || bytes.len() > MAX_BYTES
            || !matches!(extension.as_str(), "cpl" | "cmpl")
        {
            return Err("invalid_input: queue file".into());
        }
        self.prepare(|| playlist_file::read_file(&bytes, &extension))
    }
    pub(crate) fn prepare_text(&self, text: String) -> Result<u32, String> {
        if text.is_empty() || text.len() > MAX_BYTES {
            return Err("invalid_input: queue clipboard".into());
        }
        self.prepare(|| playlist_file::decrypt(&text))
    }
    fn prepare(
        &self,
        decode: impl FnOnce() -> Result<Vec<serde_json::Value>, String>,
    ) -> Result<u32, String> {
        let _worker = Worker::acquire()?;
        let mut state = self.state.try_lock().map_err(|_| "busy: queue transfer")?;
        if state.rows.is_some() || state.outcome.is_some() {
            return Err("session_used: reopen transfer".into());
        }
        self.check()?;
        let items = decode().map_err(|_| "invalid_data: encrypted queue")?;
        if items.is_empty() || items.len() > 20_000 {
            return Err("capacity_exceeded: queue row count".into());
        }
        let mut rows = Vec::with_capacity(items.len());
        for item in items {
            let object = item.as_object().ok_or("invalid_data: queue song")?;
            let id = match object.get("songmid") {
                Some(serde_json::Value::String(id)) => id.clone(),
                Some(serde_json::Value::Number(id)) => id.to_string(),
                _ => return Err("invalid_data: queue song ID".into()),
            };
            if id.is_empty() || id.len() > 4096 || id.chars().any(char::is_control) {
                return Err("invalid_data: queue song ID".into());
            }
            let field = |key: &str| -> Result<String, String> {
                let s = object
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .ok_or("invalid_data: queue song field")?;
                if s.len() > 65536 || s.contains('\0') {
                    return Err("invalid_data: queue song field".into());
                }
                Ok(s.to_owned())
            };
            let title = field("name")?;
            if title.is_empty() {
                return Err("invalid_data: queue song title".into());
            }
            for key in ["img", "interval", "source"] {
                field(key)?;
            }
            let artist = field("singer")?;
            let album = object
                .get("albumName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if album.len() > 65536 || album.contains('\0') {
                return Err("invalid_data: queue album".into());
            }
            let row =
                library::song_from_saved(id, title, artist, album.to_owned(), &item.to_string());
            // Validate online objects before owner admission, rather than letting a
            // malformed object silently vanish during metadata registration.
            if let Some(raw) = &row.online {
                let _: mio_music_sdk::MusicItem = serde_json::from_value(raw.clone())
                    .map_err(|_| "invalid_data: queue online metadata")?;
            }
            rows.push(row);
        }
        self.check()?;
        state.total = rows.len() as u32;
        state.rows = Some(rows);
        Ok(state.total)
    }
    pub(crate) fn merge(&self) -> Result<QueueImportResult, String> {
        let _worker = Worker::acquire()?;
        let mut state = self.state.try_lock().map_err(|_| "busy: queue transfer")?;
        if let Some(result) = &state.outcome {
            return result.clone();
        }
        self.check()?;
        let rows = state
            .rows
            .take()
            .ok_or("invalid_input: select a queue first")?;
        let output = Arc::new(Mutex::new(None));
        let result = self
            .lease
            .execute(runtime::Action::MergeQueue(
                self.guard.clone(),
                rows,
                output.clone(),
            ))
            .and_then(|()| {
                output
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .ok_or_else(|| "runtime_failed: missing merge outcome".to_owned())
            })
            .map(|added| QueueImportResult {
                total: state.total,
                added: added as u32,
                skipped: state.total - added as u32,
            });
        state.outcome = Some(result.clone());
        result
    }
}
