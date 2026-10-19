//! Desktop-compatible local tags and embedded artwork, bound to an original indexed file and owner.
use crate::{api::*, catalog::SnapshotPermit, runtime};
use mio_core::local_music;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
static WORKING: AtomicBool = AtomicBool::new(false);
struct Worker;
impl Worker {
    fn acquire() -> Result<Self, String> {
        WORKING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "busy: local tag worker")?;
        Ok(Self)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        WORKING.store(false, Ordering::Release);
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Form {
    title: String,
    artist: String,
    album: String,
    year: u32,
    cover: local_music::TagCoverEdit,
}
#[derive(Default)]
struct State {
    cover: Option<local_music::TagCover>,
    outcome: Option<(Form, Result<LocalTagSaveResult, String>)>,
}
pub(crate) struct Editor {
    expected: local_music::Track,
    revision: u64,
    lease: runtime::RuntimeLease,
    closed: Arc<AtomicBool>,
    state: Mutex<State>,
    _permit: SnapshotPermit,
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin(
    expected: local_music::Track,
    revision: u64,
    lease: runtime::RuntimeLease,
) -> Result<LocalTagEditor, String> {
    let permit = SnapshotPermit::acquire()?;
    let _worker = Worker::acquire()?;
    lease.ensure_open()?;
    let metadata = std::fs::symlink_metadata(&expected.path).map_err(|_| "file_unavailable")?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
        return Err("capacity_exceeded: audio edit supports regular files <=256 MiB".into());
    }
    let refreshed = local_music::refresh_track(&expected.path)
        .map_err(|_| "tags_unavailable: cannot read audio tags")?;
    if refreshed.song.id != expected.song.id
        || refreshed.size > 256 * 1024 * 1024
        || [
            &refreshed.song.title,
            &refreshed.song.artist,
            &refreshed.song.album,
        ]
        .iter()
        .any(|s| s.len() > 4096 || s.contains('\0'))
        || refreshed.path.to_str().is_none_or(|s| s.len() > 4096)
    {
        return Err("capacity_exceeded: local tag form".into());
    }
    let editor = Editor {
        expected: refreshed,
        revision,
        lease,
        closed: Arc::new(AtomicBool::new(false)),
        state: Mutex::new(State::default()),
        _permit: permit,
    };
    editor.check()?;
    Ok(LocalTagEditor { editor })
}
impl Editor {
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
    fn check(&self) -> Result<(), String> {
        let closed = self.closed.clone();
        let revision = self.revision;
        self.lease.inspect(move |core| {
            if closed.load(Ordering::Acquire) {
                return Err("cancelled".to_owned());
            }
            if core.local_revision() != revision {
                return Err("catalog_changed: reopen local tags".into());
            }
            Ok(())
        })?
    }
    pub(crate) fn draft(&self) -> Result<LocalTagDraft, String> {
        self.check()?;
        Ok(LocalTagDraft {
            title: self.expected.song.title.clone(),
            artist: self.expected.song.artist.clone(),
            album: self.expected.song.album.clone(),
            year: self.expected.year,
            path: self.expected.path.to_string_lossy().into_owned(),
        })
    }
    pub(crate) fn prepare_cover(
        &self,
        bytes: Vec<u8>,
        extension: String,
    ) -> Result<Vec<u8>, String> {
        if bytes.len() > 5 * 1024 * 1024 || extension.len() > 5 {
            return Err("invalid_image: cover exceeds limits".into());
        }
        let _worker = Worker::acquire()?;
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "busy: local tag editor")?;
        self.check()?;
        if state.outcome.is_some() {
            return Err("session_used: reopen local tags".into());
        }
        let cover = local_music::TagCover::prepare(bytes, &extension.to_ascii_lowercase())?;
        self.check()?;
        let preview = cover.preview();
        state.cover = Some(cover);
        Ok(preview)
    }
    pub(crate) fn save(
        &self,
        title: String,
        artist: String,
        album: String,
        year: u32,
        cover_mode: LocalTagCoverMode,
    ) -> Result<LocalTagSaveResult, String> {
        if year > 2099
            || [&title, &artist, &album]
                .iter()
                .any(|s| s.len() > 4096 || s.contains('\0'))
        {
            return Err("invalid_input: tags <=4096 bytes each, year 0..2099".into());
        }
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| "busy: local tag editor")?;
        let cover = match cover_mode {
            LocalTagCoverMode::Keep => local_music::TagCoverEdit::Keep,
            LocalTagCoverMode::Remove => local_music::TagCoverEdit::Remove,
            LocalTagCoverMode::Replace => local_music::TagCoverEdit::Replace(
                state
                    .cover
                    .clone()
                    .ok_or("invalid_input: choose a cover first")?,
            ),
        };
        let form = Form {
            title,
            artist,
            album,
            year,
            cover,
        };
        if let Some((previous, result)) = &state.outcome {
            return if previous == &form {
                result.clone()
            } else {
                Err("session_used: reopen local tags".into())
            };
        }
        let _worker = Worker::acquire()?;
        let result = (|| {
            self.check()?;
            let prepared = local_music::prepare_audio_tags_with_cover_when(
                &self.expected,
                &local_music::TagValues {
                    title: form.title.clone(),
                    artist: form.artist.clone(),
                    album: form.album.clone(),
                    year: form.year,
                },
                &form.cover,
                || self.closed.load(Ordering::Acquire),
            )
            .map_err(|_| "tag_save_failed: audio was not replaced; reopen editor")?;
            self.check()?;
            let output = Arc::new(Mutex::new(None));
            self.lease.execute(runtime::Action::CommitLocalTags(
                prepared,
                self.revision,
                self.closed.clone(),
                output.clone(),
            ))?;
            let saved = output
                .lock()
                .map_err(|_| "tag_result_unavailable")?
                .clone()
                .ok_or("tag_result_unavailable")?;
            Ok(saved)
        })();
        state.outcome = Some((form, result.clone()));
        result
    }
}
