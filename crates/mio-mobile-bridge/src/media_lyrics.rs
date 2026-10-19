//! Bounded native lyric projection. The native media service remains usable when
//! Flutter is suspended; fetching never blocks the core owner or native UI.
use mio_core::{
    app_core::AppCore,
    lyrics::{Lyrics, Source},
    song::Song,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
static WORKING: AtomicBool = AtomicBool::new(false);
struct Worker;
impl Drop for Worker {
    fn drop(&mut self) {
        WORKING.store(false, Ordering::Release);
    }
}
#[derive(Default)]
struct State {
    identity: Option<(Option<Song>, u64, u64, u64)>,
    epoch: u64,
    pending: Option<Source>,
    lyrics: Option<Lyrics>,
    closed: bool,
}
#[derive(Default)]
pub(crate) struct NativeLyrics(Arc<Mutex<State>>);
impl NativeLyrics {
    pub(crate) fn observe(&self, core: &AppCore, source_revision: u64) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return;
        }
        if state
            .identity
            .as_ref()
            .is_some_and(|(song, generation, local, source)| {
                song.as_ref() == core.player.song()
                    && *generation == core.player.generation
                    && *local == core.local_revision()
                    && *source == source_revision
            })
        {
            return;
        }
        state.identity = Some((
            core.player.song().cloned(),
            core.player.generation,
            core.local_revision(),
            source_revision,
        ));
        state.epoch = state.epoch.wrapping_add(1);
        state.lyrics = None;
        state.pending = core
            .player
            .song()
            .and_then(|song| core.lyrics_source(song).ok());
    }
    pub(crate) fn close(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.closed = true;
        state.pending = None;
        state.lyrics = None;
    }
    pub(crate) fn line(&self, position: f64, enabled: bool) -> Option<String> {
        if !enabled {
            return None;
        }
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return None;
        }
        if state.pending.is_some()
            && WORKING
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            let worker = Worker;
            let source = state.pending.take().expect("pending source");
            let epoch = state.epoch;
            let weak = Arc::downgrade(&self.0);
            let _ = std::thread::Builder::new()
                .name("mio-native-lyrics".into())
                .spawn(move || {
                    let _worker = worker;
                    let lyrics = source.load().ok().flatten();
                    if let Some(shared) = weak.upgrade() {
                        let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                        if !state.closed && state.epoch == epoch {
                            state.lyrics = lyrics;
                        }
                    }
                });
        }
        let lyrics = state.lyrics.as_ref()?;
        let index = lyrics.active_line(position)?;
        Some(lyrics.lines[index].text.chars().take(256).collect())
    }
}
