//! Original cursor-bound cover editing. No selected host path crosses FRB.
use crate::{
    api::*,
    catalog::{SnapshotPermit, playlist_info},
    discovery::Worker,
    runtime,
};
use mio_core::{cover, library};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[derive(Default)]
struct State {
    data: Option<String>,
    outcome: Option<Result<PlaylistInfo, String>>,
}
pub(crate) struct Editor {
    cursor: Arc<Mutex<library::LibraryCursor>>,
    lease: runtime::RuntimeLease,
    id: String,
    parent: Arc<AtomicBool>,
    closed: AtomicBool,
    state: Mutex<State>,
    _permit: SnapshotPermit,
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin(
    cursor: Arc<Mutex<library::LibraryCursor>>,
    lease: runtime::RuntimeLease,
    id: String,
    parent: Arc<AtomicBool>,
) -> Result<PlaylistCoverEditor, String> {
    let permit = SnapshotPermit::acquire()?;
    let editor = Editor {
        cursor,
        lease,
        id,
        parent,
        closed: AtomicBool::new(false),
        state: Mutex::new(State::default()),
        _permit: permit,
    };
    editor.check()?;
    Ok(PlaylistCoverEditor { editor })
}
impl Editor {
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
    fn check(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) || self.parent.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()
    }
    pub(crate) fn prepare(&self, bytes: Vec<u8>, extension: String) -> Result<Vec<u8>, String> {
        if bytes.len() > 5 * 1024 * 1024 || extension.len() > 5 {
            return Err("invalid_input: cover".into());
        }
        let _worker = Worker::acquire()?;
        self.check()?;
        let mut state = self.state.try_lock().map_err(|_| "busy: cover editor")?;
        if state.outcome.is_some() {
            return Err("session_used: reopen cover editor".into());
        }
        let prepared = cover::prepare_playlist_cover(bytes, &extension.to_ascii_lowercase())
            .map_err(|_| "invalid_image: PNG/JPG/GIF/WebP <=5 MiB within image limits")?;
        self.cursor
            .try_lock()
            .map_err(|_| "busy: cover editor")?
            .check()
            .map_err(|_| "catalog_changed: reopen cover editor")?;
        self.check()?;
        state.data = Some(prepared.data_url);
        Ok(prepared.preview)
    }
    pub(crate) fn save(&self) -> Result<PlaylistInfo, String> {
        let mut state = self.state.try_lock().map_err(|_| "busy: cover editor")?;
        if let Some(result) = &state.outcome {
            return result.clone();
        }
        let data = state
            .data
            .as_ref()
            .ok_or("invalid_input: choose a cover first")?;
        let _worker = Worker::acquire()?;
        self.check()?;
        let result = self
            .cursor
            .try_lock()
            .map_err(|_| "busy: cover editor")?
            .save_cover(&self.id, data, || self.check().is_ok())
            .map(playlist_info)
            .map_err(|_| "cover_save_failed: close and reopen before retry".to_owned());
        state.outcome = Some(result.clone());
        state.data = None;
        result
    }
}
