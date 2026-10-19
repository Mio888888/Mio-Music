//! Bounded current/next PCM lanes share one device; the audio callback never waits on I/O.
mod online_stream;
pub mod seamless;
mod stream;
use crate::song::{Song, SongQuality};
use rodio::{OutputStreamBuilder, Sink};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use stream::{Decode, PcmSource, PcmStats, Track, TransferGuard};
use tokio::sync::watch;

#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlayMode {
    #[default]
    List,
    Sequence,
    Random,
    Single,
}
impl PlayMode {
    pub fn next(self) -> Self {
        match self {
            Self::List => Self::Sequence,
            Self::Sequence => Self::Random,
            Self::Random => Self::Single,
            Self::Single => Self::List,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::List => "play.modeList",
            Self::Sequence => "play.modeSequence",
            Self::Random => "play.modeRandom",
            Self::Single => "play.modeSingle",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::Random => "Shuffle",
            Self::Sequence | Self::Single => "RepeatSong",
            _ => "Repeat",
        }
    }
    // Match original: LIST stops at the end; previous wraps in both list modes.
    pub fn index(self, current: usize, len: usize, forward: bool) -> Option<usize> {
        if len == 0 {
            return None;
        }
        if self == Self::Single && forward {
            return (current < len).then_some(current);
        }
        if self == Self::Random {
            return Some((uuid::Uuid::new_v4().as_u128() % len as u128) as usize);
        }
        if forward {
            if self == Self::List && current >= len - 1 {
                None
            } else {
                Some((current % len + 1) % len)
            }
        } else {
            Some((current % len + len - 1) % len)
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
    Ended,
    Failed,
}
pub struct Player {
    pub engine: Option<Engine>,
    pub queue: Vec<Song>,
    pub queue_qualities: std::collections::HashMap<String, SongQuality>,
    pub queue_revision: u64,
    pub current: Option<usize>,
    // Audio metadata survives removal from the editable queue. `current` is
    // the original player's numeric navigation cursor, not a selected row.
    pub active_song: Option<Song>,
    pub generation: u64,
    pub status: Status,
    pub position: f64,
    pub duration: f64,
    pub error: Option<String>,
    pub volume: f32,
    pub mode: PlayMode,
    pub buffering: bool,
    pub seamless: seamless::Config,
    planned_next: Option<(u64, PlayMode, Option<usize>)>,
    pending_seek: Option<f64>,
}
impl Player {
    pub fn new(volume: f32, mode: PlayMode) -> Self {
        Self {
            engine: None,
            queue: vec![],
            queue_qualities: std::collections::HashMap::new(),
            queue_revision: 0,
            current: None,
            active_song: None,
            generation: 0,
            status: Status::Idle,
            position: 0.,
            duration: 0.,
            error: None,
            volume: if volume.is_finite() {
                volume.clamp(0., 1.)
            } else {
                0.5
            },
            mode,
            buffering: false,
            seamless: Default::default(),
            planned_next: None,
            pending_seek: None,
        }
    }
    pub fn song(&self) -> Option<&Song> {
        self.active_song
            .as_ref()
            .or_else(|| self.current.and_then(|i| self.queue.get(i)))
    }
    pub fn queue_position(&self) -> Option<usize> {
        let song = self.song()?;
        self.queue.iter().position(|item| item.id == song.id)
    }
    pub fn remove_from_queue(&mut self, id: &str) {
        let Some(index) = self.queue.iter().position(|song| song.id == id) else {
            return;
        };
        self.active_song = self.song().cloned();
        self.queue.remove(index);
        self.queue_qualities.remove(id);
        self.queue_changed();
    }
    pub fn clear_queue(&mut self) {
        self.active_song = self.song().cloned();
        self.queue.clear();
        self.queue_qualities.clear();
        self.queue_changed();
    }
    pub fn move_in_queue(&mut self, from: usize, boundary: usize) -> Option<usize> {
        if from >= self.queue.len() || boundary > self.queue.len() {
            return None;
        }
        let to = boundary.saturating_sub(usize::from(boundary > from));
        if from != to {
            self.active_song = self.song().cloned();
            let song = self.queue.remove(from);
            self.queue.insert(to, song);
            self.queue_changed();
        }
        Some(to)
    }
    /// Match a manual song selection: reuse its queue position, or insert at the front.
    pub fn enqueue_first(&mut self, song: Song) -> usize {
        if let Some(index) = self.queue.iter().position(|item| item.id == song.id) {
            return index;
        }
        self.active_song = self.song().cloned();
        self.queue.insert(0, song);
        self.queue_changed();
        0
    }
    /// Adds songs behind the queue (a merged import).
    pub fn append_to_queue(&mut self, songs: Vec<Song>) {
        if songs.is_empty() {
            return;
        }
        self.active_song = self.song().cloned();
        self.queue.extend(songs);
        self.queue_changed();
    }
    pub fn replace_queue(&mut self, queue: Vec<Song>) {
        self.active_song = self.song().cloned();
        self.queue = queue;
        self.queue_qualities.clear();
        self.queue_revision = self.queue_revision.wrapping_add(1);
        self.planned_next = None;
    }
    fn queue_changed(&mut self) {
        self.queue_revision = self.queue_revision.wrapping_add(1);
        self.planned_next = None;
        self.refresh_prefetch();
    }
    fn advance_generation(&mut self) {
        self.generation = self.engine.as_ref().map_or(self.generation, |engine| {
            self.generation
                .max(engine.generation.load(Ordering::SeqCst))
        }) + 1;
    }
    pub fn stop(&mut self) {
        self.advance_generation();
        if let Some(engine) = &self.engine {
            engine.stop(self.generation);
        }
        self.planned_next = None;
        self.pending_seek = None;
        self.buffering = false;
        self.queue.clear();
        self.queue_qualities.clear();
        self.queue_revision = self.queue_revision.wrapping_add(1);
        self.current = None;
        self.active_song = None;
        self.status = Status::Idle;
        self.position = 0.;
        self.duration = 0.;
        self.error = None;
    }
    pub fn play_online(&mut self, source: Arc<crate::online::Online>, index: usize) {
        let Some(song) = self.queue.get(index).cloned() else {
            return;
        };
        self.current = Some(index);
        self.load_online_song(source, song);
    }
    pub fn resume_online(&mut self, source: Arc<crate::online::Online>, seconds: f64) {
        let Some(song) = self.song().cloned() else {
            return;
        };
        self.load_online_song(source, song);
        if seconds > 0. && seconds.is_finite() {
            self.seek(seconds);
        }
    }
    pub fn play_local(&mut self, index: usize, path: std::path::PathBuf) {
        let Some(song) = self.queue.get(index).cloned() else {
            return;
        };
        self.current = Some(index);
        self.load_local_song(song, path, 0., false);
    }
    pub fn resume_local(&mut self, path: std::path::PathBuf, seconds: f64) {
        let Some(song) = self.song().cloned() else {
            return;
        };
        self.load_local_song(song, path, seconds, false);
    }
    pub fn reload_local(&mut self, path: std::path::PathBuf) {
        let paused = self.status == Status::Paused;
        let Some(song) = self.song().cloned() else {
            return;
        };
        self.load_local_song(song, path, self.position, paused);
    }
    fn load_local_song(
        &mut self,
        song: Song,
        path: std::path::PathBuf,
        seconds: f64,
        paused: bool,
    ) {
        self.active_song = Some(song.clone());
        self.advance_generation();
        self.planned_next = None;
        self.pending_seek = None;
        self.position = 0.;
        self.duration = song.duration as f64;
        if seconds.is_finite() && seconds > 0. {
            self.position = if self.duration > 0. {
                seconds.min(self.duration)
            } else {
                seconds
            };
            self.pending_seek = Some(self.position);
        }
        self.status = Status::Loading;
        self.error = None;
        if self.engine.is_none() {
            match Engine::new(self.volume) {
                Ok(engine) => self.engine = Some(engine),
                Err(error) => {
                    self.status = Status::Failed;
                    self.error = Some(error);
                    return;
                }
            }
        }
        self.engine
            .as_ref()
            .unwrap()
            .load_local(self.generation, path, self.position, paused);
        if paused {
            self.status = Status::Paused;
        }
    }
    fn load_online_song(&mut self, source: Arc<crate::online::Online>, song: Song) {
        self.advance_generation();
        self.planned_next = None;
        self.pending_seek = None;
        self.active_song = Some(song.clone());
        self.position = 0.;
        self.duration = song.duration as f64;
        self.status = Status::Loading;
        self.error = None;
        if self.engine.is_none() {
            match Engine::new(self.volume) {
                Ok(engine) => self.engine = Some(engine),
                Err(error) => {
                    self.status = Status::Failed;
                    self.error = Some(error);
                    return;
                }
            }
        }
        self.engine
            .as_ref()
            .unwrap()
            .load_online(self.generation, source, song);
    }
    pub fn seek(&mut self, seconds: f64) {
        if !seconds.is_finite() || seconds < 0. {
            return;
        }
        self.position = if self.duration > 0. {
            seconds.min(self.duration)
        } else {
            seconds
        };
        if let Some(engine) = &self.engine {
            self.pending_seek = Some(self.position);
            engine.seek(self.generation, self.position);
        }
    }
    pub fn refresh_prefetch(&mut self) {
        let Some(current) = self.current else {
            return;
        };
        if self
            .planned_next
            .as_ref()
            .is_some_and(|(generation, mode, _)| {
                *generation == self.generation && *mode == self.mode
            })
        {
            return;
        }
        let next = self.mode.index(current, self.queue.len(), true);
        self.planned_next = Some((self.generation, self.mode, next));
    }
    pub fn next_index(&mut self, forward: bool) -> Option<usize> {
        self.refresh_prefetch();
        if forward {
            let next = self.planned_next.and_then(|(_, _, next)| next);
            // Original playNext falls through to sequence navigation when a
            // removed SINGLE-mode cursor no longer addresses a queue item.
            next.or_else(|| {
                if self.mode == PlayMode::Single && !self.queue.is_empty() {
                    self.current
                        .map(|current| (current % self.queue.len() + 1) % self.queue.len())
                } else {
                    None
                }
            })
        } else {
            self.current
                .and_then(|current| self.mode.index(current, self.queue.len(), false))
        }
    }
    pub fn seeking(&self) -> bool {
        self.pending_seek.is_some()
    }
    /// Whether this position report will be applied; a rejected seek acknowledgement must
    /// never reset the interpolated lyric clock merely because its numeric position matches.
    pub fn accepts_clock(&self, event: &Event) -> bool {
        event.generation == self.generation
            && match &event.kind {
                EventKind::Position(_, _) => self.pending_seek.is_none(),
                EventKind::Seeked(pos, _) => self
                    .pending_seek
                    .is_some_and(|target| (target - pos).abs() < 0.001),
                EventKind::Swapped(..) => true,
                _ => false,
            }
    }
    pub fn apply(&mut self, event: Event) -> bool {
        if event.generation != self.generation {
            return false;
        }
        self.refresh_prefetch();
        match event.kind {
            EventKind::Swapped(index, song, duration, position) => {
                self.generation += 1;
                self.current = self
                    .queue
                    .iter()
                    .position(|item| item.id == song.id)
                    .or(Some(index));
                self.active_song = Some(song);
                self.planned_next = None;
                self.position = self.pending_seek.unwrap_or(position);
                self.duration = duration.unwrap_or(self.song().map_or(0., |s| s.duration as f64));
                self.buffering = false;
                self.status = Status::Playing;
            }
            EventKind::Buffering(value) => self.buffering = value,
            EventKind::Ready(duration) => {
                if self.pending_seek.is_some() {
                    return false;
                }
                self.buffering = false;
                self.duration = duration.unwrap_or(self.duration);
                self.status = Status::Playing;
            }
            EventKind::Position(pos, paused) => {
                if self.pending_seek.is_some() {
                    return false;
                }
                self.position = pos;
                self.status = if paused {
                    Status::Paused
                } else {
                    Status::Playing
                };
            }
            EventKind::Ended => {
                if self.pending_seek.is_some() {
                    return false;
                }
                self.status = Status::Ended;
                self.position = self.duration;
                return true;
            }
            EventKind::Failed(message) => {
                self.pending_seek = None;
                self.buffering = false;
                self.status = Status::Failed;
                self.error = Some(message);
                self.advance_generation();
                if let Some(engine) = &self.engine {
                    engine.stop(self.generation);
                }
            }
            EventKind::Seeked(pos, paused) => {
                if self
                    .pending_seek
                    .is_some_and(|target| (target - pos).abs() < 0.001)
                {
                    self.pending_seek = None;
                    self.position = pos;
                    self.status = if paused {
                        Status::Paused
                    } else {
                        Status::Playing
                    };
                }
            }
        }
        false
    }
}
pub struct Event {
    pub generation: u64,
    pub kind: EventKind,
}
pub enum EventKind {
    Swapped(usize, Song, Option<f64>, f64),
    Ready(Option<f64>),
    Buffering(bool),
    Position(f64, bool),
    Seeked(f64, bool),
    Ended,
    Failed(String),
}
/// An online cover (at most 5 MiB, within 10 s) through the shared client; blocks the caller.
pub fn online_cover(raw: String) -> Option<Vec<u8>> {
    use std::io::Read;
    if let Some(bytes) = crate::image_cache::read(&raw) {
        return Some(bytes);
    }
    let url = mio_plugin::runtime_url(&raw).ok()?;
    let trace = std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some();
    let host = url.host_str().unwrap_or("").to_owned();
    let response = match crate::http::blocking()?
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
    {
        Ok(response) => response,
        Err(_) => {
            if trace {
                eprintln!("[cover] host={host} transfer=failed");
            }
            return None;
        }
    };
    if trace {
        eprintln!("[cover] host={host} status={}", response.status().as_u16());
    }
    if !response.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .take(5 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 5 * 1024 * 1024 {
        return None;
    }
    crate::image_cache::write(&raw, &bytes);
    Some(bytes)
}

enum Command {
    NextPrepared(seamless::Request, Arc<Track>),
    Transitioned(u64, u64),
    CancelNext,
    Reset(u64),
    Local(u64, std::path::PathBuf),
    Prepared(u64, Result<Arc<Track>, String>),
    Decoded(u64, u64, Result<PcmSource, String>),
    TransferFailed(u64, String),
    Pause(u64, bool),
    Seek(u64, f64),
    Volume(f32),
    OutputFailed(u64),
    Shutdown,
}
pub struct Engine {
    active_file: Arc<std::sync::Mutex<Option<(u64, String, Option<Arc<Track>>)>>>,
    commands: mpsc::Sender<Command>,
    online_loads: watch::Sender<Option<online_stream::OnlineLoad>>,
    generation: Arc<AtomicU64>,
    events: mpsc::Receiver<Event>,
    event_times: mpsc::Receiver<Instant>,
    workers: Vec<std::thread::JoinHandle<()>>,
    next_loads: watch::Sender<Option<seamless::Request>>,
    next_token: Arc<AtomicU64>,
    transition_mode: Arc<AtomicU64>,
    transition_from: Arc<AtomicU64>,
    staged: Option<(seamless::Request, u64)>,
}
impl Engine {
    pub fn new(volume: f32) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "无法启动音频下载线程".to_owned())?;
        let (commands, receiver) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let (time_tx, event_times) = mpsc::channel();
        let (online_loads, online_rx) = watch::channel(None);
        let (next_loads, next_rx) = watch::channel(None);
        let active_file = Arc::new(std::sync::Mutex::new(None));
        let worker_active_file = active_file.clone();
        let next_token = Arc::new(AtomicU64::new(0));
        let transition_mode = Arc::new(AtomicU64::new(0));
        let worker_token = next_token.clone();
        let worker_mode = transition_mode.clone();
        let transition_from = Arc::new(AtomicU64::new(u64::MAX));
        let worker_transition = transition_from.clone();
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = generation.clone();
        let output_errors = commands.clone();
        let device_worker = std::thread::spawn(move || {
            audio_worker(
                receiver,
                event_tx,
                time_tx,
                worker_generation,
                volume,
                output_errors,
                worker_token,
                worker_mode,
                worker_transition,
                worker_active_file,
            )
        });
        let ready_tx = commands.clone();
        let download_worker = std::thread::spawn(move || {
            runtime.block_on(async move {
                tokio::join!(
                    online_stream::loader(online_rx, ready_tx.clone()),
                    seamless::loader(next_rx, ready_tx)
                );
            })
        });
        Ok(Self {
            commands,
            online_loads,
            generation,
            events,
            event_times,
            workers: vec![device_worker, download_worker],
            next_loads,
            next_token,
            transition_mode,
            transition_from,
            staged: None,
            active_file,
        })
    }
    pub fn configure(&self, config: seamless::Config) {
        let mode = config.samples_mode();
        if self.transition_mode.swap(mode, Ordering::Relaxed) != mode {
            self.cancel_next();
        }
    }
    fn cancel_next(&self) {
        self.next_token.fetch_add(1, Ordering::SeqCst);
        let _ = self.next_loads.send(None);
        let _ = self.commands.send(Command::CancelNext);
    }
    pub fn prepare_next(
        &mut self,
        generation: u64,
        revision: u64,
        next: Option<(usize, Song, seamless::Input)>,
    ) {
        let Some((index, song, input)) = next else {
            if self.staged.take().is_some() {
                self.cancel_next();
            }
            return;
        };
        if self.staged.as_ref().is_some_and(|(old, rev)| {
            old.generation == generation
                && *rev == revision
                && old.index == index
                && old.song.id == song.id
                && old.token == self.next_token.load(Ordering::SeqCst)
                && match (&old.input, &input) {
                    (seamless::Input::Local(a), seamless::Input::Local(b)) => a == b,
                    (seamless::Input::Online(a), seamless::Input::Online(b)) => Arc::ptr_eq(a, b),
                    _ => false,
                }
        }) {
            return;
        }
        let reuse = if matches!(input, seamless::Input::Online(_)) {
            let active = self.active_file.lock().unwrap();
            if let Some((_, active_id, file)) =
                active.as_ref().filter(|(id, _, _)| *id == generation)
            {
                if active_id == &song.id {
                    let Some(file) = file else {
                        if self.staged.take().is_some() {
                            self.cancel_next();
                        }
                        return;
                    };
                    Some(file.clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        self.cancel_next();
        let request = seamless::Request {
            generation,
            token: self.next_token.load(Ordering::SeqCst),
            index,
            song,
            input,
            reuse,
        };
        self.staged = Some((request.clone(), revision));
        let _ = self.next_loads.send(Some(request));
    }
    pub fn load_online(&self, generation: u64, source: Arc<crate::online::Online>, song: Song) {
        *self.active_file.lock().unwrap() = Some((generation, song.id.clone(), None));
        self.cancel_next();
        self.generation.store(generation, Ordering::SeqCst);
        let _ = self.commands.send(Command::Reset(generation));
        let _ = self.online_loads.send(Some(online_stream::OnlineLoad {
            generation,
            source,
            id: song.id,
            duration: song.duration,
        }));
    }
    pub fn stop(&self, generation: u64) {
        *self.active_file.lock().unwrap() = None;
        self.cancel_next();
        let _ = self.online_loads.send(None);
        self.generation.store(generation, Ordering::SeqCst);
        let _ = self.commands.send(Command::Reset(generation));
    }
    pub fn load_local(
        &self,
        generation: u64,
        path: std::path::PathBuf,
        seconds: f64,
        paused: bool,
    ) {
        *self.active_file.lock().unwrap() = Some((generation, String::new(), None));
        self.cancel_next();
        let _ = self.online_loads.send(None);
        self.generation.store(generation, Ordering::SeqCst);
        let _ = self.commands.send(Command::Reset(generation));
        // Set playback intent before Local can start a decoder on another sender.
        if paused {
            let _ = self.commands.send(Command::Pause(generation, true));
        }
        if seconds > 0. {
            let _ = self
                .commands
                .send(Command::Seek(self.control_generation(generation), seconds));
        }
        let _ = self.commands.send(Command::Local(generation, path));
    }
    fn control_generation(&self, generation: u64) -> u64 {
        let active = self.generation.load(Ordering::SeqCst);
        if active == generation + 1 && self.transition_from.load(Ordering::SeqCst) == generation {
            active
        } else {
            generation
        }
    }
    pub fn pause(&self, generation: u64, paused: bool) {
        let _ = self
            .commands
            .send(Command::Pause(self.control_generation(generation), paused));
    }
    pub fn seek(&self, generation: u64, seconds: f64) {
        if seconds.is_finite() && seconds >= 0. {
            self.cancel_next();
            let _ = self.commands.send(Command::Seek(generation, seconds));
        }
    }
    pub fn volume(&self, volume: f32) {
        let _ = self.commands.send(Command::Volume(volume.clamp(0., 1.)));
    }
    #[allow(dead_code)]
    pub fn drain(&self) -> Vec<Event> {
        self.drain_timed()
            .into_iter()
            .map(|(event, _)| event)
            .collect()
    }
    /// The engine sampling instant travels with the event, avoiding UI polling latency.
    pub fn drain_timed(&self) -> Vec<(Event, Instant)> {
        self.events
            .try_iter()
            .map(|event| {
                // One producer publishes the timestamp before its event, in FIFO order.
                let sampled_at = self
                    .event_times
                    .try_recv()
                    .unwrap_or_else(|_| Instant::now());
                (event, sampled_at)
            })
            .collect()
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let _ = self.commands.send(Command::Shutdown);
        // Close both watch channels before joining, so no temporary file survives exit.
        let (placeholder, _) = watch::channel(None);
        drop(std::mem::replace(&mut self.online_loads, placeholder));
        let (placeholder, _) = watch::channel(None);
        drop(std::mem::replace(&mut self.next_loads, placeholder));
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn audio_worker(
    rx: mpsc::Receiver<Command>,
    tx: mpsc::Sender<Event>,
    time_tx: mpsc::Sender<Instant>,
    current: Arc<AtomicU64>,
    mut volume: f32,
    commands: mpsc::Sender<Command>,
    next_token: Arc<AtomicU64>,
    transition_mode: Arc<AtomicU64>,
    transition_from: Arc<AtomicU64>,
    active_file: Arc<std::sync::Mutex<Option<(u64, String, Option<Arc<Track>>)>>>,
) {
    // Opt-in event diagnostics on the worker, never the audio callback; no URLs/credentials.
    let trace = std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some();
    let mut output = None;
    let mut sink: Option<Sink> = None;
    let mut track_file: Option<Arc<Track>> = None;
    let mut generation = 0;
    let mut last_update = Instant::now();
    let mut output_epoch = 0;
    let mut epoch = Arc::new(AtomicU64::new(0));
    let (mut decode_tx, decode_rx) = mpsc::channel();
    let decode_epoch = epoch.clone();
    let decode_commands = commands.clone();
    let decoder = std::thread::spawn(move || {
        stream::decoder_worker(decode_rx, decode_epoch, decode_commands)
    });
    let mut next_epoch = Arc::new(AtomicU64::new(0));
    let (mut next_decode_tx, next_decode_rx) = mpsc::channel();
    let next_worker_epoch = next_epoch.clone();
    let next_commands = commands.clone();
    let next_decoder = std::thread::spawn(move || {
        stream::decoder_worker(next_decode_rx, next_worker_epoch, next_commands)
    });
    let mut decode_version = 0_u64;
    let mut next_track: Option<(seamless::Request, Arc<Track>)> = None;
    let mut next_stats = None;
    let mut pcm_slot = Arc::new(std::sync::Mutex::new(None));
    let mut handoff = Arc::new(AtomicU64::new(0));
    let mut stats: Option<(Arc<PcmStats>, u16, u32, f64)> = None;
    let mut target = 0.;
    let mut paused = false;
    let mut seeking = false;
    let mut ended = false;
    let mut deferred_pause = None;
    let mut deferred_seek = None;
    loop {
        if generation != current.load(Ordering::SeqCst) {
            if let Some(old) = sink.take() {
                old.stop();
            }
            epoch.store(0, Ordering::SeqCst);
            next_epoch.store(0, Ordering::SeqCst);
            next_track = None;
            next_stats = None;
            pcm_slot = Arc::new(std::sync::Mutex::new(None));
            handoff = Arc::new(AtomicU64::new(0));
            track_file = None;
            stats = None;
            ended = false;
            deferred_pause = None;
            deferred_seek = None;
        }
        let deferred = if handoff.load(Ordering::SeqCst) == 0 {
            deferred_pause
                .take()
                .map(|(id, value)| Command::Pause(id, value))
                .or_else(|| {
                    deferred_seek
                        .take()
                        .map(|(id, position)| Command::Seek(id, position))
                })
        } else {
            None
        };
        let command = if deferred.is_some() {
            deferred
        } else {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(_) => break,
            }
        };
        // Serialize callback promotion and controls with a nonblocking atomic claim.
        // Only the most recent pause/seek intent is retained while promotion commits.
        let mut control_claimed = false;
        let command = match command {
            Some(Command::Pause(id, value)) => {
                if handoff
                    .compare_exchange(0, u64::MAX, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    control_claimed = true;
                    Some(Command::Pause(id, value))
                } else {
                    deferred_pause = Some((id, value));
                    None
                }
            }
            Some(Command::Seek(id, position)) => {
                if handoff
                    .compare_exchange(0, u64::MAX, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    control_claimed = true;
                    Some(Command::Seek(id, position))
                } else {
                    deferred_seek = Some((id, position));
                    None
                }
            }
            other => other,
        };
        // Open local files on this worker, never on the UI / audio callback.
        let command = match command {
            Some(Command::Local(id, path)) if id == current.load(Ordering::SeqCst) => {
                Some(Command::Prepared(
                    id,
                    Track::local(path).map_err(|_| "本地音乐文件不存在或无法读取".into()),
                ))
            }
            Some(Command::Local(_, _)) => None,
            other => other,
        };
        let event_generation = generation;
        let emit = |kind| {
            let _ = time_tx.send(Instant::now());
            let _ = tx.send(Event {
                generation: event_generation,
                kind,
            });
        };
        match command {
            Some(Command::Shutdown) => break,
            Some(Command::CancelNext) if handoff.load(Ordering::SeqCst) == 0 => {
                next_epoch.store(0, Ordering::SeqCst);
                next_track = None;
                next_stats = None;
            }
            Some(Command::NextPrepared(request, file))
                if request.generation == generation
                    && generation == current.load(Ordering::SeqCst)
                    && request.token == next_token.load(Ordering::SeqCst)
                    && transition_mode.load(Ordering::Relaxed) != 0
                    && handoff.load(Ordering::SeqCst) == 0 =>
            {
                decode_version += 1;
                next_epoch.store(decode_version, Ordering::SeqCst);
                let _ = next_decode_tx.send(Decode {
                    generation,
                    version: decode_version,
                    track: file.clone(),
                    seconds: 0.,
                });
                next_track = Some((request, file));
            }
            Some(Command::Decoded(id, version, source))
                if id == generation
                    && id == current.load(Ordering::SeqCst)
                    && version == next_epoch.load(Ordering::SeqCst)
                    && next_track.as_ref().is_some_and(|(request, _)| {
                        request.token == next_token.load(Ordering::SeqCst)
                    }) =>
            {
                if let (Ok(source), Some((request, _))) = (source, &next_track) {
                    let (channels, rate) = source.format();
                    if trace {
                        eprintln!(
                            "[playback] next_pcm_ready generation={generation} token={} channels={channels} rate={rate} duration={:?}",
                            request.token, source.duration
                        );
                    }
                    next_stats = Some((
                        source.stats.clone(),
                        channels,
                        rate,
                        source.offset,
                        source.duration,
                    ));
                    *pcm_slot.lock().unwrap() = Some(seamless::NextPcm {
                        token: request.token,
                        source,
                    });
                }
            }
            Some(Command::Transitioned(id, token))
                if id == generation
                    && next_track
                        .as_ref()
                        .is_some_and(|(request, _)| request.token == token) =>
            {
                if let (Some((request, file)), Some((state, channels, rate, offset, duration))) =
                    (next_track.take(), next_stats.take())
                {
                    if current
                        .compare_exchange(
                            generation,
                            generation + 1,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        )
                        .is_ok()
                    {
                        let position = offset
                            + state.samples.load(Ordering::Relaxed) as f64
                                / f64::from(channels)
                                / f64::from(rate);
                        if trace {
                            eprintln!(
                                "[playback] seamless_swap generation={generation} next_index={} mode={} incoming_seconds={position:.6} paused={paused}",
                                request.index,
                                transition_mode.load(Ordering::Relaxed)
                            );
                        }
                        transition_from.store(generation, Ordering::SeqCst);
                        {
                            let mut active = active_file.lock().unwrap();
                            if active.as_ref().is_some_and(|(id, _, _)| *id == generation) {
                                *active = Some((
                                    generation + 1,
                                    request.song.id.clone(),
                                    Some(file.clone()),
                                ));
                            }
                        }
                        emit(EventKind::Swapped(
                            request.index,
                            request.song,
                            duration,
                            position,
                        ));
                        generation += 1;
                        track_file = Some(file);
                        ended = false;
                        stats = Some((state, channels, rate, offset));
                        std::mem::swap(&mut epoch, &mut next_epoch);
                        std::mem::swap(&mut decode_tx, &mut next_decode_tx);
                        next_epoch.store(0, Ordering::SeqCst);
                        target = 0.;
                        handoff.store(0, Ordering::SeqCst);
                        continue;
                    }
                }
            }
            Some(Command::Reset(id)) if id == current.load(Ordering::SeqCst) => {
                if let Some(old) = sink.take() {
                    old.stop();
                }
                epoch.store(0, Ordering::SeqCst);
                next_epoch.store(0, Ordering::SeqCst);
                next_track = None;
                next_stats = None;
                pcm_slot = Arc::new(std::sync::Mutex::new(None));
                handoff = Arc::new(AtomicU64::new(0));
                track_file = None;
                stats = None;
                generation = id;
                transition_from.store(u64::MAX, Ordering::SeqCst);
                target = 0.;
                paused = false;
                seeking = false;
                ended = false;
            }
            Some(Command::Prepared(id, file))
                if id == generation && id == current.load(Ordering::SeqCst) =>
            {
                match file {
                    Ok(file) => {
                        if let Some((_, _, slot)) = active_file
                            .lock()
                            .unwrap()
                            .as_mut()
                            .filter(|(id, _, _)| *id == generation)
                        {
                            *slot = Some(file.clone());
                        }
                        decode_version += 1;
                        let version = decode_version;
                        epoch.store(version, Ordering::SeqCst);
                        track_file = Some(file.clone());
                        let _ = decode_tx.send(Decode {
                            generation: id,
                            version,
                            track: file,
                            seconds: target,
                        });
                    }
                    Err(message) => emit(EventKind::Failed(message)),
                }
            }
            Some(Command::Decoded(id, version, source))
                if id == generation
                    && id == current.load(Ordering::SeqCst)
                    && version == epoch.load(Ordering::SeqCst) =>
            {
                let prepared = (|| {
                    let source = source?;
                    if trace {
                        eprintln!(
                            "[playback] active_pcm_ready generation={generation} duration={:?} offset={:.6}",
                            source.duration, source.offset
                        );
                    }
                    if output.is_none() {
                        output_epoch += 1;
                        let active_epoch = output_epoch;
                        let errors = commands.clone();
                        let mut stream = OutputStreamBuilder::from_default_device()
                            .map_err(|_| "无法打开音频输出设备".to_owned())?
                            .with_error_callback(move |_| {
                                let _ = errors.send(Command::OutputFailed(active_epoch));
                            })
                            .open_stream_or_fallback()
                            .map_err(|_| "无法打开音频输出设备".to_owned())?;
                        stream.log_on_drop(false);
                        output = Some(stream);
                    }
                    if id != current.load(Ordering::SeqCst) {
                        return Ok(None);
                    }
                    let new_sink = Sink::connect_new(output.as_ref().unwrap().mixer());
                    new_sink.set_volume(volume);
                    if paused {
                        new_sink.pause();
                    }
                    let (channels, rate) = source.format();
                    let duration = source.duration;
                    let offset = source.offset;
                    let new_stats = (source.stats.clone(), channels, rate, offset);
                    // The original's chain: decoder → EQ → balance → spectrum.
                    let spectrum_generation = Arc::new(AtomicU64::new(generation));
                    let source = seamless::Continuous::new(
                        source,
                        pcm_slot.clone(),
                        handoff.clone(),
                        next_token.clone(),
                        transition_mode.clone(),
                        commands.clone(),
                        generation,
                        spectrum_generation.clone(),
                        (
                            output.as_ref().unwrap().config().channel_count(),
                            output.as_ref().unwrap().config().sample_rate(),
                        ),
                    );
                    let effects = crate::effects::shared();
                    let source = crate::effects::EqSource::new(source, effects.clone());
                    let source = crate::effects::BalanceSource::new(source, effects.clone());
                    let source =
                        crate::effects::SpectrumSource::new(source, effects, spectrum_generation);
                    new_sink.append(source);
                    Ok(Some((new_sink, new_stats, duration, offset)))
                })();
                match prepared {
                    Ok(Some((new_sink, new_stats, duration, offset))) => {
                        sink = Some(new_sink);
                        stats = Some(new_stats);
                        seeking = false;
                        emit(EventKind::Ready(duration));
                        emit(EventKind::Position(offset, paused));
                        emit(EventKind::Buffering(false));
                    }
                    Ok(None) => {}
                    Err(message) => {
                        epoch.fetch_add(1, Ordering::SeqCst);
                        track_file = None;
                        emit(EventKind::Failed(message));
                    }
                }
            }
            Some(Command::Pause(id, value))
                if !ended
                    && generation == current.load(Ordering::SeqCst)
                    && (id == generation
                        || (id + 1 == generation
                            && transition_from.load(Ordering::SeqCst) == id)) =>
            {
                paused = value;
                if let Some(sink) = &sink {
                    if paused {
                        sink.pause();
                    } else {
                        sink.play();
                    }
                }
                let position = stats
                    .as_ref()
                    .map(|(s, channels, rate, offset)| {
                        *offset
                            + s.samples.load(Ordering::Relaxed) as f64
                                / f64::from(*channels)
                                / f64::from(*rate)
                    })
                    .unwrap_or(target);
                emit(EventKind::Position(position, paused));
            }
            Some(Command::Seek(id, pos))
                if generation == current.load(Ordering::SeqCst)
                    && (id == generation
                        || (id + 1 == generation
                            && transition_from.load(Ordering::SeqCst) == id)) =>
            {
                target = pos;
                ended = false;
                emit(EventKind::Seeked(target, paused));
                if let Some(file) = &track_file {
                    if let Some(old) = sink.take() {
                        old.stop();
                    }
                    stats = None;
                    seeking = true;
                    decode_version += 1;
                    let version = decode_version;
                    epoch.store(version, Ordering::SeqCst);
                    let _ = decode_tx.send(Decode {
                        generation,
                        version,
                        track: file.clone(),
                        seconds: target,
                    });
                    emit(EventKind::Position(target, paused));
                    emit(EventKind::Buffering(true));
                }
            }
            Some(Command::TransferFailed(id, message)) if id == generation => {
                if let Some(old) = sink.take() {
                    old.stop();
                }
                epoch.fetch_add(1, Ordering::SeqCst);
                track_file = None;
                stats = None;
                emit(EventKind::Failed(message));
            }
            Some(Command::OutputFailed(active_epoch))
                if active_epoch == output_epoch && output.is_some() =>
            {
                if let Some(old) = sink.take() {
                    old.stop();
                }
                epoch.fetch_add(1, Ordering::SeqCst);
                track_file = None;
                stats = None;
                output = None;
                emit(EventKind::Failed("音频输出设备已断开，请重新播放".into()));
            }
            Some(Command::Volume(value)) => {
                volume = value;
                if let Some(sink) = &sink {
                    sink.set_volume(volume);
                }
            }
            _ => {}
        }
        if control_claimed {
            handoff.store(0, Ordering::SeqCst);
        }
        if handoff.load(Ordering::SeqCst) != 0 {
            continue;
        }
        if let (Some(active), Some((state, channels, rate, offset))) = (&sink, &stats) {
            if active.empty() && !seeking {
                if state.failed.load(Ordering::Relaxed) {
                    track_file = None;
                    emit(EventKind::Failed("音频下载或解码中断，请重新播放".into()));
                } else {
                    // Keep the completed source available for a later seek.
                    // Its sink has ended, so locating another position must
                    // prepare paused audio until the user resumes playback.
                    paused = true;
                    // Pause commands after EOF must not turn Ended into Paused:
                    // resume uses Ended to reload the exhausted source.
                    ended = true;
                    emit(EventKind::Ended);
                }
                sink = None;
                stats = None;
            } else if !paused && last_update.elapsed() >= Duration::from_millis(200) {
                // A paused clock does not move; the pause and seek handlers report it at once.
                emit(EventKind::Position(
                    *offset
                        + state.samples.load(Ordering::Relaxed) as f64
                            / f64::from(*channels)
                            / f64::from(*rate),
                    paused,
                ));
                emit(EventKind::Buffering(
                    state.buffering.load(Ordering::Relaxed),
                ));
                last_update = Instant::now();
            }
        }
    }
    epoch.store(0, Ordering::SeqCst);
    next_epoch.store(0, Ordering::SeqCst);
    drop(next_track);
    drop(next_decode_tx);
    drop(pcm_slot);
    drop(sink);
    drop(track_file);
    drop(decode_tx);
    let _ = decoder.join();
    let _ = next_decoder.join();
    drop(output);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_queue_boundaries_and_modes() {
        assert_eq!(PlayMode::List.index(2, 3, true), None);
        assert_eq!(PlayMode::Sequence.index(2, 3, true), Some(0));
        assert_eq!(PlayMode::List.index(0, 3, false), Some(2));
        assert_eq!(PlayMode::Single.index(1, 3, true), Some(1));
        assert_eq!(PlayMode::List.index(0, 0, true), None);
        assert!(PlayMode::Random.index(0, 3, true).unwrap() < 3);
        assert!(PlayMode::List.next().next().next().next() == PlayMode::List);
    }
    #[test]
    fn repeated_seek_rejects_old_positions_until_latest_ack_and_random_plan_is_stable() {
        let mut player = Player::new(0.5, PlayMode::Random);
        player.generation = 4;
        player.duration = 100.;
        player.pending_seek = Some(30.);
        player.position = 30.;
        player.status = Status::Paused;
        for kind in [
            EventKind::Position(5., false),
            EventKind::Ready(Some(100.)),
            EventKind::Seeked(20., false),
            EventKind::Ended,
        ] {
            player.apply(Event {
                generation: 4,
                kind,
            });
            assert_eq!(player.position, 30.);
            assert!(player.status == Status::Paused);
        }
        player.apply(Event {
            generation: 4,
            kind: EventKind::Seeked(30., true),
        });
        assert!(player.pending_seek.is_none());
        player.apply(Event {
            generation: 4,
            kind: EventKind::Position(30.1, false),
        });
        assert_eq!(player.position, 30.1);
        player.queue = (0..3)
            .map(|i| Song {
                id: i.to_string(),
                title: "Song".into(),
                artist: "".into(),
                album: "".into(),
                duration: 100,
                cover_art: None,
            })
            .collect();
        player.current = Some(0);
        let next = player.next_index(true);
        for _ in 0..100 {
            assert_eq!(player.next_index(true), next);
        }
        player.mode = PlayMode::Sequence;
        assert_eq!(player.next_index(true), Some(1));
    }
    #[test]
    fn stale_ready_position_and_end_cannot_change_new_track_or_source() {
        let mut player = Player::new(0.5, PlayMode::List);
        player.generation = 4;
        player.status = Status::Loading;
        for kind in [
            EventKind::Ready(Some(20.)),
            EventKind::Position(18., false),
            EventKind::Ended,
            EventKind::Failed("old".into()),
        ] {
            assert!(!player.apply(Event {
                generation: 3,
                kind
            }));
            assert!(player.status == Status::Loading);
            assert_eq!(player.position, 0.);
        }
        player.stop();
        assert!(!player.apply(Event {
            generation: 4,
            kind: EventKind::Ended
        }));
        assert!(player.status == Status::Idle);
    }
}
