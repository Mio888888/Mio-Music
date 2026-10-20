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
impl Input {
    pub(super) fn same_source(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Local(a), Self::Local(b)) => a == b,
            (Self::Online(a), Self::Online(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
#[derive(Clone)]
pub(super) struct Request {
    pub generation: u64,
    pub token: u64,
    pub index: usize,
    pub song: Song,
    pub input: Input,
    pub cache_key: Option<String>,
    pub reuse: Option<Arc<Track>>,
    pub prepared: Arc<Mutex<Option<Arc<Track>>>>,
}
impl Request {
    pub(super) fn publish(
        &self,
        tx: &mpsc::Sender<Command>,
        track: Arc<Track>,
    ) -> Result<(), String> {
        let mut prepared = self.prepared.lock().unwrap();
        if prepared
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &track))
        {
            return Ok(());
        }
        *prepared = Some(track.clone());
        tx.send(Command::NextPrepared(self.clone(), track))
            .map_err(|_| "播放已关闭".to_owned())
    }

    async fn cancelled(
        &self,
        mut updates: watch::Receiver<Option<Request>>,
        active_file: &ActiveFile,
    ) {
        loop {
            let staged = updates
                .borrow()
                .as_ref()
                .is_some_and(|next| next.token == self.token);
            if !staged && !self.is_active(active_file) {
                return;
            }
            tokio::select! {
                changed = updates.changed() => {
                    if changed.is_err() { return; }
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
        }
    }

    fn is_active(&self, active_file: &ActiveFile) -> bool {
        let prepared = self.prepared.lock().unwrap().clone();
        let active = active_file.lock().unwrap();
        match (
            prepared,
            active.as_ref().and_then(|(_, _, file)| file.as_ref()),
        ) {
            (Some(prepared), Some(active)) => Arc::ptr_eq(&prepared, active),
            _ => false,
        }
    }
}
pub(super) async fn loader(
    mut requests: watch::Receiver<Option<Request>>,
    tx: mpsc::Sender<Command>,
    active_file: ActiveFile,
) {
    let slots = Arc::new(tokio::sync::Semaphore::new(1));
    let client = online_stream::audio_client();
    let mut jobs = tokio::task::JoinSet::new();
    loop {
        let request = requests.borrow_and_update().clone();
        if let Some(request) = request {
            let updates = requests.clone();
            let active_file = active_file.clone();
            let tx = tx.clone();
            let slots = slots.clone();
            let client = client.clone();
            jobs.spawn(async move {
                tokio::select! { biased;
                    _ = request.cancelled(updates, &active_file) => {}
                    _ = prepare(&request, tx, slots, client.as_ref(), &active_file) => {}
                }
            });
        }
        loop {
            tokio::select! { biased;
                changed = requests.changed() => {
                    if changed.is_err() { return; }
                    break;
                }
                _ = jobs.join_next(), if !jobs.is_empty() => {}
            }
        }
    }
}

async fn prepare(
    request: &Request,
    tx: mpsc::Sender<Command>,
    slots: Arc<tokio::sync::Semaphore>,
    client: Option<&reqwest::Client>,
    active_file: &ActiveFile,
) {
    let mut failures = 0_u32;
    let retry_started = Instant::now();
    loop {
        let remaining = Duration::from_secs(300).saturating_sub(retry_started.elapsed());
        if remaining.is_zero() {
            return;
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
                        Some(request),
                        client,
                    )
                    .await
                }
            }
        };
        let result = tokio::time::timeout(Duration::from_secs(150).min(remaining), prepare)
            .await
            .unwrap_or_else(|_| Err("在线播放超时".into()));
        match result {
            Ok(track) => {
                let _ = request.publish(&tx, track);
                return;
            }
            Err(error)
                if matches!(request.input, Input::Online(_))
                    && request.reuse.is_none()
                    && !request.is_active(active_file)
                    && online_stream::retryable(&error) =>
            {
                failures += 1;
                if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
                    eprintln!(
                        "[playback] prefetch_retry generation={} token={} attempt={}",
                        request.generation,
                        request.token,
                        failures + 1
                    );
                }
                tokio::time::sleep(
                    Duration::from_secs((2_u64 << failures.min(5).saturating_sub(1)).min(30))
                        .min(Duration::from_secs(300).saturating_sub(retry_started.elapsed())),
                )
                .await;
            }
            Err(_) => return,
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
            let prepared = self.mailbox.try_lock().ok().and_then(|mut slot| {
                if slot
                    .as_ref()
                    .is_some_and(|next| next.source.stats.ready.load(Ordering::Acquire))
                {
                    slot.take()
                } else {
                    None
                }
            });
            if let Some(next) = prepared {
                if next.token == self.token.load(Ordering::SeqCst) {
                    self.next =
                        Some((next.token, Lane::new(next.source, self.channels, self.rate)));
                    self.fade = None;
                }
            }
            let mode = self.mode.load(Ordering::Relaxed);
            if self.next.as_ref().is_some_and(|(token, lane)| {
                *token != self.token.load(Ordering::SeqCst) || lane.stats.failed()
            }) || mode == 0
            {
                self.next = None;
                self.fade = None;
            }
            if self.fade.is_none()
                && mode > 1
                && !self.active.stats.buffering.load(Ordering::Relaxed)
                && !self.active.stats.failed()
                && self
                    .next
                    .as_ref()
                    .is_some_and(|(_, lane)| !lane.stats.failed())
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
        if self.active.stats.failed() {
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
        if outgoing.is_none() && !self.active.stats.failed() && self.next.is_some() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use stream::tests::pcm;

    fn request(track: Arc<Track>) -> Request {
        Request {
            generation: 4,
            token: 7,
            index: 1,
            song: Song {
                id: "next".into(),
                title: "Next".into(),
                artist: String::new(),
                album: String::new(),
                duration: 8,
                cover_art: None,
            },
            input: Input::Local(track.path().to_owned()),
            cache_key: None,
            reuse: None,
            prepared: Arc::new(Mutex::new(None)),
        }
    }

    #[test]
    fn progressive_track_is_published_once_before_download_finishes() {
        let track = Track::new().unwrap();
        let request = request(track.clone());
        let (tx, rx) = mpsc::channel();
        request.publish(&tx, track.clone()).unwrap();
        let Command::NextPrepared(received, prepared) = rx.try_recv().unwrap() else {
            panic!("next track must be published immediately");
        };
        assert_eq!(received.token, request.token);
        assert!(Arc::ptr_eq(&prepared, &track));
        assert!(!prepared.complete());
        track.finish(false);
        request.publish(&tx, track).unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn promoted_transfer_survives_next_request_and_cancels_when_stopped() {
        let track = Track::new().unwrap();
        let request = request(track.clone());
        *request.prepared.lock().unwrap() = Some(track.clone());
        let active_file = Arc::new(Mutex::new(Some((5, "next".into(), Some(track)))));
        let (updates, rx) = watch::channel(Some(request.clone()));
        let cancelled = request.cancelled(rx, &active_file);
        tokio::pin!(cancelled);
        updates.send(None).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut cancelled)
                .await
                .is_err()
        );
        *active_file.lock().unwrap() = Some((6, "next".into(), Some(Track::new().unwrap())));
        updates.send(None).unwrap();
        tokio::time::timeout(Duration::from_millis(200), cancelled)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn unused_transfer_and_closed_loader_are_cancellable() {
        let track = Track::new().unwrap();
        let request = request(track);
        let active_file = Arc::new(Mutex::new(None));
        let (updates, rx) = watch::channel(Some(request.clone()));
        updates.send(None).unwrap();
        tokio::time::timeout(
            Duration::from_millis(200),
            request.cancelled(rx, &active_file),
        )
        .await
        .unwrap();
        let (updates, rx) = watch::channel(Some(request.clone()));
        drop(updates);
        tokio::time::timeout(
            Duration::from_millis(200),
            request.cancelled(rx, &active_file),
        )
        .await
        .unwrap();
    }

    fn continuous(next: PcmSource, token: u64, mode: u64) -> (Continuous, mpsc::Receiver<Command>) {
        let active = pcm(vec![0.25; 1600], 1, 16000);
        let (commands, rx) = mpsc::channel();
        let source = Continuous::new(
            active,
            Arc::new(Mutex::new(Some(NextPcm {
                token,
                source: next,
            }))),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(7)),
            Arc::new(AtomicU64::new(mode)),
            commands,
            4,
            Arc::new(AtomicU64::new(4)),
            (1, 16000),
        );
        (source, rx)
    }

    #[test]
    fn gapless_handoff_has_no_inserted_silence_and_promotes_once() {
        let (mut source, rx) = continuous(pcm(vec![0.75; 1600], 1, 16000), 7, 1);
        let samples: Vec<_> = source.by_ref().collect();
        assert_eq!(samples.len(), 3200);
        assert!(samples[..1600].iter().all(|sample| *sample == 0.25));
        assert!(samples[1600..].iter().all(|sample| *sample == 0.75));
        assert!(matches!(rx.try_recv(), Ok(Command::Transitioned(4, 7))));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn unbuffered_next_waits_without_fading_out_current_track() {
        let next = pcm(vec![0.75; 1600], 1, 16000);
        let ready = next.stats.clone();
        ready.ready.store(false, Ordering::Release);
        let (mut source, rx) = continuous(next, 7, 500);
        for _ in 0..800 {
            assert_eq!(source.next(), Some(0.25));
        }
        assert!(rx.try_recv().is_err());
        ready.ready.store(true, Ordering::Release);
        let samples: Vec<_> = source.collect();
        assert!(samples.iter().all(|sample| *sample > 0.));
        assert!(matches!(rx.try_recv(), Ok(Command::Transitioned(4, 7))));
    }

    #[test]
    fn incoming_transfer_failure_aborts_crossfade_without_cutting_current_track() {
        let next = pcm(vec![0.75; 1600], 1, 16000);
        let stats = next.stats.clone();
        let (mut source, rx) = continuous(next, 7, 500);
        for _ in 0..400 {
            assert!(source.next().unwrap() > 0.);
        }
        assert!(source.fade.is_some());
        stats.transfer_failed.store(true, Ordering::Release);
        let remaining: Vec<_> = source.collect();
        assert_eq!(remaining.len(), 1200);
        assert!(remaining.iter().all(|sample| *sample == 0.25));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn stale_failed_and_disabled_prefetch_never_replace_current_audio() {
        for (token, mode, failed) in [(6, 1, false), (7, 0, false), (7, 1, true)] {
            let next = pcm(vec![0.75; 1600], 1, 16000);
            next.stats.failed.store(failed, Ordering::Relaxed);
            let (source, rx) = continuous(next, token, mode);
            let samples: Vec<_> = source.collect();
            assert_eq!(samples.len(), 1600);
            assert!(samples.iter().all(|sample| *sample == 0.25));
            assert!(rx.try_recv().is_err());
        }
    }
}
