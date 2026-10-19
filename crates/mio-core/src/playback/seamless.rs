//! Two bounded PCM lanes feed one sink. The callback only consumes ready PCM.
use super::*;
use rodio::{Source, source::UniformSourceIterator};
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub crossfade: bool,
    pub duration_ms: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            crossfade: false,
            duration_ms: 3000,
        }
    }
}
impl Config {
    pub fn samples_mode(self) -> u64 {
        if !self.enabled {
            0
        } else if !self.crossfade {
            1
        } else {
            self.duration_ms.clamp(500, 8000)
        }
    }
}
#[derive(Clone)]
pub enum Input {
    Local(std::path::PathBuf),
    Online(Arc<crate::online::Online>),
}
#[derive(Clone)]
pub(super) struct Request {
    pub generation: u64,
    pub token: u64,
    pub index: usize,
    pub song: Song,
    pub input: Input,
    pub reuse: Option<Arc<Track>>,
}
pub(super) async fn loader(
    mut requests: watch::Receiver<Option<Request>>,
    tx: mpsc::Sender<Command>,
) {
    let slots = Arc::new(tokio::sync::Semaphore::new(1));
    let client = online_stream::audio_client();
    let mut failures = 0_u32;
    let mut retry_started = Instant::now();
    loop {
        let request = requests.borrow_and_update().clone();
        if let Some(request) = request {
            let remaining = Duration::from_secs(300).saturating_sub(retry_started.elapsed());
            if remaining.is_zero() {
                if requests.changed().await.is_err() {
                    break;
                }
                failures = 0;
                retry_started = Instant::now();
                continue;
            }
            let prepare = async {
                if let Some(track) = &request.reuse {
                    if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
                        eprintln!(
                            "[playback] reuse_current generation={} token={}",
                            request.generation, request.token
                        );
                    }
                    // One download feeds both decoders in repeat mode; only stage complete audio.
                    while !track.complete() {
                        if track.failed() {
                            return Err("当前音频下载失败".to_owned());
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    return Ok(track.clone());
                }
                match &request.input {
                    Input::Local(path) => {
                        let path = path.clone();
                        tokio::task::spawn_blocking(move || Track::local(path))
                            .await
                            .map_err(|_| "本地预加载任务失败".to_owned())?
                            .map_err(|_| "本地音乐文件无法读取".to_owned())
                    }
                    Input::Online(source) => {
                        online_stream::transfer(
                            online_stream::OnlineLoad {
                                generation: request.generation,
                                source: source.clone(),
                                id: request.song.id.clone(),
                                duration: request.song.duration,
                            },
                            tx.clone(),
                            slots.clone(),
                            true,
                            client.as_ref(),
                        )
                        .await
                    }
                }
            };
            tokio::select! { biased;
                changed = requests.changed() => {
                    if changed.is_err() { break; }
                    failures = 0;
                    retry_started = Instant::now();
                }
                result = tokio::time::timeout(Duration::from_secs(150).min(remaining), prepare) => {
                    if let Ok(Ok(track)) = result {
                        let _ = tx.send(Command::NextPrepared(request, track));
                    } else if matches!(request.input, Input::Online(_))
                        && request.reuse.is_none()
                        && retry_started.elapsed() < Duration::from_secs(300)
                        && result.as_ref().map_or(true, |result| result.as_ref()
                            .err().is_some_and(|error| online_stream::retryable(error)))
                    {
                        failures += 1;
                        if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
                            eprintln!("[playback] prefetch_retry generation={} token={} attempt={}",
                                request.generation, request.token, failures + 1);
                        }
                        tokio::select! { biased;
                            changed = requests.changed() => {
                                if changed.is_err() { break; }
                                failures = 0;
                                retry_started = Instant::now();
                            }
                            _ = tokio::time::sleep(Duration::from_secs(
                                (2_u64 << failures.min(5).saturating_sub(1)).min(30)
                            ).min(Duration::from_secs(300).saturating_sub(retry_started.elapsed()))) => {}
                        }
                        continue;
                    }
                    // Speculation failing must not stop the song currently playing.
                    if requests.changed().await.is_err() { break; }
                    failures = 0;
                    retry_started = Instant::now();
                }
            }
        } else if requests.changed().await.is_err() {
            break;
        } else {
            failures = 0;
            retry_started = Instant::now();
        }
    }
}

pub(super) struct NextPcm {
    pub token: u64,
    pub source: PcmSource,
}
struct Lane {
    source: UniformSourceIterator<PcmSource>,
    stats: Arc<PcmStats>,
    channels: u16,
    rate: u32,
    offset: f64,
    duration: Option<f64>,
}
impl Lane {
    fn new(source: PcmSource, channels: u16, rate: u32) -> Self {
        let (native_channels, native_rate) = source.format();
        Self {
            stats: source.stats.clone(),
            channels: native_channels,
            rate: native_rate,
            offset: source.offset,
            duration: source.duration,
            source: UniformSourceIterator::new(source, channels, rate),
        }
    }
    fn position(&self) -> f64 {
        self.offset
            + self.stats.samples.load(Ordering::Relaxed) as f64
                / f64::from(self.channels)
                / f64::from(self.rate)
    }
}
pub(super) struct Continuous {
    active: Lane,
    next: Option<(u64, Lane)>,
    mailbox: Arc<Mutex<Option<NextPcm>>>,
    handoff: Arc<AtomicU64>,
    token: Arc<AtomicU64>,
    mode: Arc<AtomicU64>,
    commands: mpsc::Sender<Command>,
    generation: u64,
    spectrum_generation: Arc<AtomicU64>,
    channels: u16,
    rate: u32,
    fade: Option<(u64, u64)>,
    frame_sample: u64,
}
impl Continuous {
    pub fn new(
        source: PcmSource,
        mailbox: Arc<Mutex<Option<NextPcm>>>,
        handoff: Arc<AtomicU64>,
        token: Arc<AtomicU64>,
        mode: Arc<AtomicU64>,
        commands: mpsc::Sender<Command>,
        generation: u64,
        spectrum_generation: Arc<AtomicU64>,
        output_format: (u16, u32),
    ) -> Self {
        let (channels, rate) = output_format;
        Self {
            active: Lane::new(source, channels, rate),
            next: None,
            mailbox,
            handoff,
            token,
            mode,
            commands,
            generation,
            spectrum_generation,
            channels,
            rate,
            fade: None,
            frame_sample: 0,
        }
    }
    fn swap(&mut self) -> bool {
        let Some((token, _)) = self.next.as_ref() else {
            return false;
        };
        let token = *token;
        if self
            .handoff
            .compare_exchange(0, token, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }
        // Claim the lane before the worker can replace its matching metadata.
        if token != self.token.load(Ordering::SeqCst) {
            self.handoff.store(0, Ordering::SeqCst);
            self.next = None;
            self.fade = None;
            return false;
        }
        if let Some((_, lane)) = self.next.take() {
            self.active = lane;
            self.fade = None;
            let _ = self
                .commands
                .send(Command::Transitioned(self.generation, token));
            self.generation += 1;
            self.spectrum_generation
                .store(self.generation, Ordering::SeqCst);
            return true;
        }
        false
    }
}
impl Iterator for Continuous {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        // Observe changes once per frame, never block on a producer or an I/O lock.
        if self.frame_sample % u64::from(self.channels) == 0 {
            let prepared = self
                .mailbox
                .try_lock()
                .ok()
                .and_then(|mut slot| slot.take());
            if let Some(next) = prepared {
                if next.token == self.token.load(Ordering::SeqCst) {
                    self.next =
                        Some((next.token, Lane::new(next.source, self.channels, self.rate)));
                    self.fade = None;
                }
            }
            let mode = self.mode.load(Ordering::Relaxed);
            if self
                .next
                .as_ref()
                .is_some_and(|(token, _)| *token != self.token.load(Ordering::SeqCst))
                || mode == 0
            {
                self.next = None;
                self.fade = None;
            }
            if self.fade.is_none()
                && mode > 1
                && !self.active.stats.buffering.load(Ordering::Relaxed)
                && !self.active.stats.failed.load(Ordering::Relaxed)
                && self
                    .next
                    .as_ref()
                    .is_some_and(|(_, lane)| !lane.stats.failed.load(Ordering::Relaxed))
                && self.active.duration.is_some_and(|duration| {
                    let remaining = duration - self.active.position();
                    remaining > 0. && remaining <= mode as f64 / 1000.
                })
            {
                let remaining = self.active.duration.unwrap() - self.active.position();
                let seconds = remaining.min(mode as f64 / 1000.);
                self.fade = Some((
                    0,
                    ((seconds * f64::from(self.rate)).floor() as u64).max(1)
                        * u64::from(self.channels),
                ));
            }
        }
        self.frame_sample = self.frame_sample.wrapping_add(1);
        let outgoing = self.active.source.next();
        if self.active.stats.failed.load(Ordering::Relaxed) {
            self.next = None;
            self.fade = None;
        }
        if let Some((elapsed, total)) = self.fade {
            if let Some((_, next)) = self.next.as_mut() {
                match next.source.next() {
                    Some(incoming) if !next.stats.buffering.load(Ordering::Relaxed) => {
                        let progress = (elapsed / u64::from(self.channels)) as f32
                            / (total / u64::from(self.channels)).max(1) as f32;
                        let sample = outgoing.unwrap_or(0.) * (1. - progress) + incoming * progress;
                        if elapsed + 1 >= total {
                            self.swap();
                        } else {
                            self.fade = Some((elapsed + 1, total));
                        }
                        return Some(sample);
                    }
                    Some(_) => {
                        // Keep the outgoing track audible; restart within its remaining time when ready.
                        self.fade = None;
                        if outgoing.is_some() {
                            return outgoing;
                        }
                    }
                    None => {
                        // A broken/short prefetch never cuts off the outgoing track.
                        self.next = None;
                        self.fade = None;
                    }
                }
            }
        }
        if outgoing.is_none()
            && !self.active.stats.failed.load(Ordering::Relaxed)
            && self.next.is_some()
        {
            if self.swap() {
                return self.active.source.next();
            }
            if self.next.is_some() {
                return Some(0.);
            }
        }
        outgoing
    }
}
impl Source for Continuous {
    fn current_span_len(&self) -> Option<usize> {
        // Both lanes are converted to this fixed format, including at handoff.
        // Keep the output resampler's fractional phase across PCM packets.
        None
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
