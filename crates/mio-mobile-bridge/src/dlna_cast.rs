//! One process-wide cast worker; all remote commands target one immutable renderer.
use crate::{api::DlnaCastState, discovery::Worker, runtime};
use mio_core::{app_core::AppCore, dlna::Renderer, online::Online, song::Song};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
static CAST: Mutex<Option<Arc<Cast>>> = Mutex::new(None);
static TOKENS: AtomicU64 = AtomicU64::new(1);
#[derive(Clone)]
pub(crate) struct Playback {
    lease: runtime::RuntimeLease,
    cancelled: Arc<AtomicBool>,
    revision: u64,
    host_revision: u64,
    source: u64,
    generation: u64,
    song: Option<Song>,
    online: Option<Arc<Online>>,
    playing: bool,
    volume: i32,
    seek: Option<(u64, u64, f64)>,
    restore_failed: Arc<AtomicBool>,
}
impl Playback {
    fn capture(cast: &Cast) -> Result<Self, String> {
        cast.check()?;
        let revision = cast.lease.playback_revision();
        let host_revision = cast.lease.host_revision();
        let (generation, song, online, volume) = cast.lease.inspect(|core| {
            (
                core.player.generation,
                core.player.song().cloned(),
                core.online.clone(),
                (core.player.volume * 100.).round() as i32,
            )
        })?;
        let snapshot = Self {
            lease: cast.lease.clone(),
            cancelled: cast.cancelled.clone(),
            revision,
            host_revision,
            source: cast.source,
            generation,
            song,
            online,
            playing: cast.lease.playback_desired() && cast.lease.playback_allowed(),
            restore_failed: cast.restore_failed.clone(),
            volume,
            seek: cast.lease.seek_intent(),
        };
        snapshot.check()?;
        Ok(snapshot)
    }
    pub(crate) fn validate(&self, core: &AppCore) -> Result<(), String> {
        self.lease.ensure_open()?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if self.lease.source_revision() != self.source {
            return Err("source_changed".into());
        }
        let same_online = match (&self.online, &core.online) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if self.lease.host_revision() != self.host_revision
            || self.lease.playback_revision() != self.revision
            || !same_online
            || self.generation != core.player.generation
            || self.song.as_ref() != core.player.song()
            || self.playing != (self.lease.playback_desired() && self.lease.playback_allowed())
        {
            return Err("dlna_playback_changed".into());
        }
        Ok(())
    }
    pub(crate) fn restore_failed(&self) {
        self.restore_failed.store(true, Ordering::Release);
    }
    pub(crate) fn lease(&self) -> &runtime::RuntimeLease {
        &self.lease
    }
    pub(crate) fn epoch(&self) -> u64 {
        self.host_revision
    }
    pub(crate) fn was_playing(&self) -> bool {
        self.playing
    }
    pub(crate) fn same_owner(&self, core: &AppCore) -> bool {
        self.lease.ensure_open().is_ok()
            && self.lease.source_revision() == self.source
            && self.lease.host_revision() == self.host_revision
            && self.generation == core.player.generation
            && self.song.as_ref() == core.player.song()
            && match (&self.online, &core.online) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    pub(crate) fn transfer_valid(&self, core: &AppCore) -> bool {
        self.same_owner(core)
            && !self.cancelled.load(Ordering::Acquire)
            && self.lease.playback_revision() == self.revision
    }
    fn check(&self) -> Result<(), String> {
        let snapshot = self.clone();
        self.lease.inspect(move |core| snapshot.validate(core))?
    }
    pub(crate) fn correct(self, core: &mut AppCore, seconds: f64) -> Result<(), String> {
        self.validate(core)?;
        if !seconds.is_finite() || seconds < 0. {
            return Err("invalid_input: DLNA position".into());
        }
        if self.playing && (core.player.position - seconds).abs() > 2. {
            core.seek(seconds)?;
        }
        Ok(())
    }
}
struct Cast {
    token: u64,
    lease: runtime::RuntimeLease,
    source: u64,
    initial_seek: u64,
    restore_failed: Arc<AtomicBool>,
    renderer: Renderer,
    cancelled: Arc<AtomicBool>,
    view: Mutex<DlnaCastState>,
    ended: Mutex<bool>,
    finished: Condvar,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Cast {
    fn check(&self) -> Result<(), String> {
        if self.restore_failed.load(Ordering::Acquire) {
            return Err("dlna_restore_failed: local playback remains paused".into());
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()?;
        if self.source != self.lease.source_revision() {
            return Err("source_changed".into());
        }
        Ok(())
    }
    fn snapshot(&self) -> Result<DlnaCastState, String> {
        Ok(self.view.lock().map_err(|_| "dlna_unavailable")?.clone())
    }
    fn publish(&self, phase: &str, song: Option<String>, error: Option<String>) {
        let mut view = self.view.lock().unwrap_or_else(|error| error.into_inner());
        view.phase = phase.into();
        view.song = song;
        view.error = error;
    }
    fn run(&self) {
        let mut touched = false;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.follow(&mut touched)))
                .unwrap_or_else(|_| Err("dlna_worker_failed".into()));
        // Cleanup always addresses the captured renderer, even after runtime/source replacement.
        let stop = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if touched {
                self.renderer.stop()
            } else {
                Ok(())
            }
        }))
        .unwrap_or_else(|_| Err("dlna_stop_unconfirmed".into()));
        let error = if self.restore_failed.load(Ordering::Acquire) {
            Some("dlna_restore_failed: local playback remains paused; use Play to retry".into())
        } else {
            result.err().filter(|error| error != "cancelled")
        };
        match stop {
            Ok(()) => self.publish("stopped", None, error),
            Err(_) => self.publish(
                "stop_unconfirmed",
                None,
                Some("dlna_stop_unconfirmed: renderer may still be playing".into()),
            ),
        }
        *self.ended.lock().unwrap_or_else(|error| error.into_inner()) = true;
        self.finished.notify_all();
    }
    fn follow(&self, touched: &mut bool) -> Result<(), String> {
        let mut handed: Option<(u64, String)> = None;
        let mut playing = false;
        let mut volume = -1;
        let mut position_at = Instant::now();
        let mut seek_revision = self.initial_seek;
        let mut pending_seek: Option<(f64, Instant)> = None;
        loop {
            let snapshot = match Playback::capture(self) {
                Err(error) if error == "dlna_playback_changed" => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                result => result?,
            };
            let Some(song) = &snapshot.song else {
                if *touched {
                    self.renderer.stop()?;
                    *touched = false;
                }
                handed = None;
                self.publish("waiting", None, None);
                std::thread::sleep(Duration::from_millis(250));
                continue;
            };
            if !mio_core::online::is_online(song) {
                return Err("dlna_online_song_required".into());
            }
            let key = (snapshot.generation, song.id.clone());
            let result = (|| {
                if handed.as_ref() != Some(&key) {
                    self.publish(
                        "connecting",
                        Some(song.title.chars().take(256).collect()),
                        None,
                    );
                    let transfer = crate::dlna_handoff::Lease::begin(snapshot.clone())?;
                    let online = snapshot.online.as_ref().ok_or("source_unavailable")?;
                    // Bound metadata before the resolver clones it.
                    online.bounded_item(&song.id)?.ok_or("source_unavailable")?;
                    transfer.check()?;
                    let url = online.resolve(&song.id, self.cancelled.clone())?;
                    transfer.check()?;
                    self.renderer.play_checked(
                        &url,
                        &song.title,
                        false,
                        &|| transfer.check(),
                        &mut || *touched = true,
                    )?;
                    handed = Some(key);
                    playing = false;
                    transfer.check()?;
                    self.renderer.set_volume(f64::from(snapshot.volume))?;
                    volume = snapshot.volume;
                    transfer.check()?;
                    if snapshot.playing {
                        self.renderer.resume()?;
                        playing = true;
                    }
                    pending_seek = None;
                    // A paused selection must not briefly start the renderer just to query it.
                    let position = if snapshot.playing {
                        transfer.wait(Duration::from_millis(1500))?;
                        let position = self.renderer.position()?;
                        transfer.check()?;
                        Some(position)
                    } else {
                        None
                    };
                    transfer.finish(position)?;
                    position_at = Instant::now();
                    // The local resume acknowledgement is asynchronous. Capture a fresh
                    // projection next iteration rather than using the pre-pause status.
                    return Ok(());
                }
                snapshot.check()?;
                if volume != snapshot.volume {
                    self.renderer.set_volume(f64::from(snapshot.volume))?;
                    volume = snapshot.volume;
                }
                snapshot.check()?;
                if playing != snapshot.playing {
                    if snapshot.playing {
                        self.renderer.resume()?;
                    } else {
                        self.renderer.pause()?;
                    }
                    playing = snapshot.playing;
                }
                if let Some((revision, generation, target)) = snapshot.seek {
                    if revision > seek_revision {
                        snapshot.check()?;
                        if generation == snapshot.generation {
                            self.renderer.seek(target)?;
                            pending_seek = Some((target, Instant::now()));
                            position_at = Instant::now();
                        }
                        seek_revision = revision;
                    }
                }
                if (snapshot.playing || pending_seek.is_some())
                    && position_at.elapsed() >= Duration::from_secs(1)
                {
                    snapshot.check()?;
                    let position = self.renderer.position()?;
                    snapshot.check()?;
                    let confirmed = if let Some((target, sent)) = pending_seek {
                        let expected = target
                            + if snapshot.playing {
                                sent.elapsed().as_secs_f64()
                            } else {
                                0.
                            };
                        if (position.position - expected).abs() <= 2. {
                            pending_seek = None;
                            true
                        } else if sent.elapsed() >= Duration::from_secs(5) {
                            return Err("dlna_seek_unconfirmed".into());
                        } else {
                            false
                        }
                    } else {
                        true
                    };
                    if confirmed {
                        self.lease.execute(runtime::Action::DlnaPosition(
                            snapshot.clone(),
                            position.position,
                        ))?;
                    }
                    position_at = Instant::now();
                }
                snapshot.check()?;
                Ok::<(), String>(())
            })();
            match result {
                Err(error) if error == "dlna_playback_changed" => {
                    // The next snapshot reconciles the remote state with the newer intent.
                    // Keep a successfully handed URI; a stale volume/status read must not restart it.
                }
                Err(error) => return Err(error),
                Ok(()) => self.publish(
                    if playing { "playing" } else { "paused" },
                    Some(song.title.chars().take(256).collect()),
                    None,
                ),
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}
pub(crate) fn start(
    lease: runtime::RuntimeLease,
    renderer: Renderer,
    closed: Arc<AtomicBool>,
) -> Result<DlnaCastState, String> {
    let mut slot = CAST.try_lock().map_err(|_| "busy: DLNA")?;
    if let Some(previous) = slot.as_ref() {
        if !*previous.ended.lock().map_err(|_| "dlna_unavailable")? {
            return Err("dlna_already_connected: stop the current renderer first".into());
        }
        if let Some(thread) = previous
            .thread
            .lock()
            .map_err(|_| "dlna_unavailable")?
            .take()
        {
            let _ = thread.join();
        }
    }
    lease.ensure_open()?;
    if closed.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    let worker = Worker::acquire()?;
    let token = TOKENS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
            (value < i64::MAX as u64).then_some(value + 1)
        })
        .map_err(|_| "capacity_exceeded: DLNA")?;
    let view = DlnaCastState {
        token,
        name: renderer.device().name.chars().take(256).collect(),
        address: renderer.device().address.clone(),
        phase: "connecting".into(),
        song: None,
        error: None,
    };
    let cast = Arc::new(Cast {
        token,
        source: lease.source_revision(),
        restore_failed: Arc::new(AtomicBool::new(false)),
        initial_seek: lease.seek_intent().map(|intent| intent.0).unwrap_or(0),
        lease,
        renderer,
        cancelled: Arc::new(AtomicBool::new(false)),
        view: Mutex::new(view.clone()),
        ended: Mutex::new(false),
        finished: Condvar::new(),
        thread: Mutex::new(None),
    });
    let output = cast.clone();
    let thread = std::thread::Builder::new()
        .name("mio-dlna-cast".into())
        .spawn(move || {
            let _worker = worker;
            output.run();
        })
        .map_err(|_| "dlna_worker_unavailable")?;
    *cast.thread.lock().map_err(|_| "dlna_unavailable")? = Some(thread);
    *slot = Some(cast);
    Ok(view)
}
pub(crate) fn state() -> Result<Option<DlnaCastState>, String> {
    let cast = CAST.lock().map_err(|_| "dlna_unavailable")?.clone();
    cast.map(|cast| cast.snapshot()).transpose()
}
pub(crate) fn stop(token: u64) -> Result<DlnaCastState, String> {
    let cast = CAST
        .lock()
        .map_err(|_| "dlna_unavailable")?
        .as_ref()
        .filter(|cast| cast.token == token)
        .cloned()
        .ok_or("dlna_session_changed")?;
    cast.cancelled.store(true, Ordering::Release);
    let ended = cast.ended.lock().map_err(|_| "dlna_unavailable")?;
    let _ended = cast
        .finished
        .wait_while(ended, |ended| !*ended)
        .map_err(|_| "dlna_unavailable")?;
    cast.snapshot()
}
