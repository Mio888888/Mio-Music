//! Cancellable player presentation data. Provider/disk work stays off owner;
//! seek and clock remain bound to the exact playback generation.
use crate::{api::*, runtime};
use mio_core::{app_core::AppCore, lyrics::Lyrics, online::Online, song::Song};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

static HANDLES: AtomicUsize = AtomicUsize::new(0);
static WORKERS: AtomicUsize = AtomicUsize::new(0);
struct Worker;
impl Worker {
    fn acquire() -> Result<Self, String> {
        WORKERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .map_err(|_| "busy: player details workers occupied")?;
        Ok(Self)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}
#[derive(Clone)]
pub(crate) struct Token {
    song: Song,
    generation: u64,
    queue: u64,
    local: u64,
    online: Option<Arc<Online>>,
    cancelled: Arc<AtomicBool>,
}
impl Token {
    pub(crate) fn validate(&self, core: &AppCore) -> Result<(), String> {
        let same = match (&self.online, &core.online) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if !same
            || core.player.generation != self.generation
            || core.player.queue_revision != self.queue
            || core.local_revision() != self.local
            || core.player.song() != Some(&self.song)
        {
            return Err("player_changed: reload player details".into());
        }
        Ok(())
    }
}
pub(crate) struct Session {
    token: Token,
    lease: runtime::RuntimeLease,
    lyrics: Mutex<Option<Arc<Lyrics>>>,
    favorite_operation: Mutex<()>,
    loading: AtomicBool,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.cancel();
        HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
}
fn capture(
    id: String,
    generation: u64,
    queue: u64,
    source_revision: u64,
) -> Result<(Token, runtime::RuntimeLease), String> {
    if id.is_empty() || id.len() > 4096 {
        return Err("invalid_input: song identifier".into());
    }
    let (token, lease) = runtime::inspect_with_lease(move |core| {
        let song = core
            .player
            .song()
            .filter(|song| song.id == id)
            .cloned()
            .ok_or("player_changed: song unavailable")?;
        let token = Token {
            song,
            generation,
            queue,
            local: core.local_revision(),
            online: core.online.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        token.validate(core)?;
        Ok::<_, String>(token)
    })?;
    if lease.source_revision() != source_revision {
        return Err("player_changed: source changed".into());
    }
    Ok((token?, lease))
}
pub(crate) fn begin(
    id: String,
    generation: u64,
    queue: u64,
    source_revision: u64,
) -> Result<Session, String> {
    HANDLES
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 4).then_some(n + 1)
        })
        .map_err(|_| "busy: too many player detail sessions")?;
    let result = capture(id, generation, queue, source_revision).map(|(token, lease)| Session {
        token,
        lease,
        lyrics: Mutex::new(None),
        favorite_operation: Mutex::new(()),
        loading: AtomicBool::new(false),
    });
    if result.is_err() {
        HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
    result
}
pub(crate) fn seek_current(
    id: String,
    generation: u64,
    queue: u64,
    source_revision: u64,
    seconds: f64,
) -> Result<(), String> {
    if !seconds.is_finite() || seconds < 0. {
        return Err("invalid_input: seek position".into());
    }
    let (token, lease) = capture(id, generation, queue, source_revision)?;
    lease.execute(runtime::Action::SeekDetails(token, seconds))
}

impl Session {
    fn comment_reference(
        &self,
    ) -> Result<
        Option<(
            Arc<Online>,
            mio_music_sdk::MusicItem,
            Arc<dyn mio_music_sdk::MusicListProvider>,
        )>,
        String,
    > {
        let token = self.token.clone();
        self.lease.inspect(move |core| {
            token.validate(core)?;
            let Some(online) = token.online.clone() else {
                return Ok(None);
            };
            let Some(item) = online.bounded_item(&token.song.id)? else {
                return Ok(None);
            };
            if item.source.len() > 4096 {
                return Err("capacity_exceeded: comment source".into());
            }
            Ok(mio_music_sdk::provider_with(&item.source, |c| c.comments)
                .map(|provider| (online, item, provider)))
        })?
    }
    pub(crate) fn comments_available(&self) -> Result<bool, String> {
        Ok(self.comment_reference()?.is_some())
    }
    pub(crate) fn open_comments(&self) -> Result<CommentsSession, String> {
        let (online, item, provider) = self.comment_reference()?.ok_or("comments_unavailable")?;
        crate::comments::begin(self.lease.clone(), online, item, provider)
    }

    fn singer_reference(&self) -> Result<(Arc<Online>, crate::singer::Reference, String), String> {
        let token = self.token.clone();
        self.lease.inspect(move |core| {
            token.validate(core)?;
            let online = token.online.clone().ok_or("singer_unavailable")?;
            let reference =
                crate::singer::reference(&online, &token.song)?.ok_or("singer_unavailable")?;
            Ok((online, reference, token.song.artist.clone()))
        })?
    }
    pub(crate) fn singer_available(&self) -> Result<bool, String> {
        let token = self.token.clone();
        self.lease.inspect(move |core| {
            token.validate(core)?;
            let Some(online) = &token.online else {
                return Ok(false);
            };
            Ok(crate::singer::reference(online, &token.song)?.is_some())
        })?
    }
    pub(crate) fn open_singer(&self) -> Result<SingerSession, String> {
        let (online, reference, name) = self.singer_reference()?;
        crate::singer::begin(self.lease.clone(), online, reference, name)
    }
    pub(crate) fn cancel(&self) {
        self.token.cancelled.store(true, Ordering::Release);
    }
    fn valid(&self) -> Result<(), String> {
        let token = self.token.clone();
        self.lease.inspect(move |core| token.validate(core))?
    }
    fn favorite_target(&self) -> Result<(mio_core::app_core::FavoriteTarget, bool), String> {
        let token = self.token.clone();
        self.lease.inspect(move |core| {
            token.validate(core)?;
            let target = core
                .favorite_target(&token.song)
                .ok_or("favorite_unavailable: reopen player")?;
            Ok((target, core.preferences.english()))
        })?
    }
    pub(crate) fn prepare_download(&self) -> Result<DownloadChoice, String> {
        crate::downloads::begin(self.lease.clone(), || {
            let token = self.token.clone();
            self.lease.inspect(move |core| {
                token.validate(core)?;
                let online = token
                    .online
                    .clone()
                    .ok_or("download_unavailable: online source required")?;
                let item = online
                    .item_json(&token.song.id)
                    .ok_or("download_unavailable: metadata unavailable")?;
                let request = core
                    .prepare_download(online, item)
                    .map_err(|_| "download_unavailable: source or quality unavailable")?;
                Ok((request, token.song.title))
            })?
        })
    }

    pub(crate) fn favorite(&self) -> Result<bool, String> {
        let _operation = self
            .favorite_operation
            .try_lock()
            .map_err(|_| "busy: favorite operation in progress")?;
        let _worker = Worker::acquire()?;
        let (target, _) = self.favorite_target()?;
        let liked = target
            .read_state()
            .map_err(|_| "favorite_unavailable: cannot read favorites")?;
        self.valid()?;
        Ok(liked)
    }
    pub(crate) fn set_favorite(&self, liked: bool) -> Result<bool, String> {
        let _operation = self
            .favorite_operation
            .try_lock()
            .map_err(|_| "busy: favorite operation in progress")?;
        let _worker = Worker::acquire()?;
        let (target, english) = self.favorite_target()?;
        self.valid()?;
        // Admission is now tied to the captured song/path. Once SQLite begins,
        // return the actual outcome even if the player/view subsequently changes.
        target
            .save_state(liked, english)
            .map_err(|_| "favorite_save_failed: previous favorites retained".into())
    }
    pub(crate) fn cover(&self) -> Result<Option<Vec<u8>>, String> {
        let _worker = Worker::acquire()?;
        let token = self.token.clone();
        let source = self.lease.inspect(move |core| {
            token.validate(core)?;
            Ok::<_, String>(core.cover_source(&token.song))
        })??;
        let bytes = source
            .map(|source| source.load())
            .transpose()
            .map_err(|_| "cover_unavailable: cannot load cover")?
            .flatten();
        self.valid()?;
        Ok(bytes)
    }
    pub(crate) fn lyrics(&self) -> Result<LyricDocument, String> {
        struct Loading<'a>(&'a AtomicBool);
        impl Drop for Loading<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        self.loading
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "busy: lyrics loading")?;
        let _loading = Loading(&self.loading);
        let _worker = Worker::acquire()?;
        let token = self.token.clone();
        let source = self.lease.inspect(move |core| {
            token.validate(core)?;
            core.lyrics_source(&token.song)
                .map_err(|_| "lyrics_unavailable: song details missing".to_owned())
        })??;
        let lyrics = source
            .load()
            .map_err(|_| "lyrics_unavailable: cannot load lyrics")?;
        self.valid()?;
        let Some(lyrics) = lyrics else {
            *self.lyrics.lock().map_err(|_| "runtime_failed")? = None;
            return Ok(LyricDocument {
                rows: Vec::new(),
                synced: false,
                word_timed: false,
            });
        };
        let mut bytes = 0usize;
        let mut words = 0usize;
        for line in &lyrics.lines {
            bytes = bytes
                .saturating_add(line.text.len())
                .saturating_add(line.translation.as_ref().map_or(0, String::len))
                .saturating_add(line.romanization.as_ref().map_or(0, String::len));
            words = words.saturating_add(line.words.len());
            for word in &line.words {
                bytes = bytes
                    .saturating_add(word.text.len())
                    .saturating_add(word.romanization.len());
            }
            if bytes > 8 * 1024 * 1024 || words > 32768 || lyrics.lines.len() > 4096 {
                return Err("capacity_exceeded: lyric document too large".into());
            }
        }
        let rows = lyrics
            .lines
            .iter()
            .enumerate()
            .map(|(i, row)| LyricRow {
                start_ms: row.start.map(|v| v as f64),
                end_ms: row.end.map(|v| v as f64),
                text: row.text.clone(),
                translation: row.translation.clone(),
                romanization: row.romanization.clone(),
                words: row
                    .words
                    .iter()
                    .map(|word| LyricWord {
                        start_ms: word.start as f64,
                        end_ms: word.end as f64,
                        text: word.text.clone(),
                        romanization: word.romanization.clone(),
                    })
                    .collect(),
                background: row.background,
                duet: row.duet,
                seek_seconds: lyrics.line_position(i),
            })
            .collect();
        let document = LyricDocument {
            rows,
            synced: lyrics.synced,
            word_timed: lyrics.word_timed,
        };
        self.valid()?;
        *self.lyrics.lock().map_err(|_| "runtime_failed")? = Some(Arc::new(lyrics));
        Ok(document)
    }
    pub(crate) fn clock(&self) -> Result<LyricClock, String> {
        let lyrics = self.lyrics.lock().map_err(|_| "runtime_failed")?.clone();
        let token = self.token.clone();
        self.lease.inspect(move |core| {
            token.validate(core)?;
            let Some(lyrics) = lyrics else {
                return Ok(LyricClock {
                    position_ms: core.player.position * 1000.,
                    playing: core.player.status == mio_core::playback::Status::Playing,
                    active_row: None,
                });
            };
            let active = lyrics.active_line(core.player.position);
            Ok(LyricClock {
                position_ms: core.player.position * 1000. + lyrics.offset as f64,
                playing: core.player.status == mio_core::playback::Status::Playing,
                active_row: active.map(|i| i as u32),
            })
        })?
    }
    pub(crate) fn seek(&self, position: f64) -> Result<(), String> {
        if !position.is_finite() || position < 0. {
            return Err("invalid_input: seek position".into());
        }
        self.lease
            .execute(runtime::Action::SeekDetails(self.token.clone(), position))
    }
    pub(crate) fn seek_line(&self, index: u32) -> Result<(), String> {
        let lyrics = self
            .lyrics
            .lock()
            .map_err(|_| "runtime_failed")?
            .clone()
            .ok_or("lyrics_unavailable")?;
        let position = lyrics
            .line_position(index as usize)
            .ok_or("invalid_input: untimed lyric line")?;
        self.lease
            .execute(runtime::Action::SeekDetails(self.token.clone(), position))
    }
}
