//! Native artwork and shared song theme: one worker process-wide, one pending source per runtime.
//! No decoding/network work on the application owner or native main thread.
use mio_core::{app_core::AppCore, cover::Source, song::Song};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

static WORKING: AtomicBool = AtomicBool::new(false);
static REVISION: AtomicU64 = AtomicU64::new(1);
fn next_revision() -> u64 {
    REVISION.fetch_add(1, Ordering::Relaxed)
}
struct Worker;
impl Drop for Worker {
    fn drop(&mut self) {
        WORKING.store(false, Ordering::Release);
    }
}

#[derive(PartialEq)]
struct Identity {
    song: Option<Song>,
    generation: u64,
    local: u64,
    source: u64,
}
#[derive(Default)]
struct State {
    identity: Option<Identity>,
    epoch: u64,
    revision: u64,
    source: Option<Source>,
    png: Option<String>,
    theme: Option<crate::api::CoverTheme>,
    closed: bool,
}
#[derive(Default)]
pub(crate) struct Artwork(Arc<Mutex<State>>);
impl Artwork {
    /// Called while publishing the matching text snapshot. Only clone source data.
    pub(crate) fn observe(&self, core: &AppCore, source: u64) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return;
        }
        if state.identity.as_ref().is_some_and(|identity| {
            identity.song.as_ref() == core.player.song()
                && identity.generation == core.player.generation
                && identity.local == core.local_revision()
                && identity.source == source
        }) {
            return;
        }
        let identity = Identity {
            song: core.player.song().cloned(),
            generation: core.player.generation,
            local: core.local_revision(),
            source,
        };
        state.source = identity
            .song
            .as_ref()
            .and_then(|song| core.cover_source(song));
        state.identity = Some(identity);
        state.epoch = next_revision();
        state.revision = state.epoch;
        state.png = None;
        state.theme = None;
    }
    pub(crate) fn close(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.closed = true;
        state.source = None;
        state.png = None;
        state.theme = None;
        state.identity = None;
    }
    pub(crate) fn theme(&self) -> Option<crate::api::CoverTheme> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        self.start(&mut state);
        state.theme.clone()
    }
    fn start(&self, state: &mut State) {
        if !state.closed
            && state.source.is_some()
            && WORKING
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            let worker = Worker;
            let source = state.source.take().expect("source checked");
            let epoch = state.epoch;
            let weak = Arc::downgrade(&self.0);
            // No request queue: while one old load completes, snapshots retain only
            // the newest source; the next observer poll admits that source.
            let _ = std::thread::Builder::new()
                .name("mio-media-artwork".into())
                .spawn(move || {
                    let _worker = worker;
                    let projection = source.load_media_theme().ok().flatten();
                    if let Some(state) = weak.upgrade() {
                        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                        if !state.closed && state.epoch == epoch {
                            state.theme = projection.as_ref().map(|(_, palette)| {
                                let rgb = palette.main.to_rgb();
                                crate::api::CoverTheme {
                                    red: (rgb.r * 255.).round() as u8,
                                    green: (rgb.g * 255.).round() as u8,
                                    blue: (rgb.b * 255.).round() as u8,
                                    use_black_text: palette.use_black_text,
                                }
                            });
                            state.png = projection.map(|(png, _)| png);
                            state.revision = next_revision();
                        }
                    }
                });
        }
    }
    /// Native observers acknowledge only an applied image revision. An explicit
    /// null on a new revision clears old art immediately while the new image loads.
    pub(crate) fn snapshot(&self, seen: u64) -> (u64, Option<String>) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        self.start(&mut state);
        (
            state.revision,
            if state.revision == seen {
                None
            } else {
                state.png.clone()
            },
        )
    }
}
