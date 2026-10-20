//! Network waits and decoding run off the audio callback; PCM storage is bounded.
use super::Command;
use rodio::{Decoder, Source};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tempfile::NamedTempFile;

#[derive(Default)]
struct DownloadState {
    bytes: u64,
    done: bool,
    failed: bool,
}
pub(super) struct Track {
    file: TrackFile,
    state: Mutex<DownloadState>,
    changed: Condvar,
    duration_hint: AtomicU64,
    transfer_failed: Arc<AtomicBool>,
}
enum TrackFile {
    Temporary(NamedTempFile),
    Local(std::path::PathBuf),
}
impl Track {
    pub fn new() -> io::Result<Arc<Self>> {
        Ok(Arc::new(Self {
            file: TrackFile::Temporary(NamedTempFile::new()?),
            state: Mutex::default(),
            changed: Condvar::new(),
            duration_hint: AtomicU64::new(0),
            transfer_failed: Arc::new(AtomicBool::new(false)),
        }))
    }
    pub fn local(path: std::path::PathBuf) -> io::Result<Arc<Self>> {
        let metadata = File::open(&path)?.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::other("not a regular audio file"));
        }
        Ok(Arc::new(Self {
            file: TrackFile::Local(path),
            state: Mutex::new(DownloadState {
                bytes: metadata.len(),
                done: true,
                failed: false,
            }),
            changed: Condvar::new(),
            duration_hint: AtomicU64::new(0),
            transfer_failed: Arc::new(AtomicBool::new(false)),
        }))
    }
    pub fn reopen(&self) -> io::Result<File> {
        match &self.file {
            TrackFile::Temporary(file) => file.reopen(),
            TrackFile::Local(path) => File::open(path),
        }
    }
    pub fn path(&self) -> &std::path::Path {
        match &self.file {
            TrackFile::Temporary(file) => file.path(),
            TrackFile::Local(path) => path,
        }
    }
    pub fn progress(&self, bytes: u64) {
        self.state.lock().unwrap().bytes = bytes;
        self.changed.notify_all();
    }
    pub fn set_duration_hint(&self, seconds: u64) {
        self.duration_hint.store(seconds, Ordering::Relaxed);
    }
    pub fn complete(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.done && !state.failed
    }
    pub fn finish(&self, failed: bool) {
        let mut state = self.state.lock().unwrap();
        state.done = true;
        state.failed = failed;
        self.transfer_failed.store(failed, Ordering::Release);
        self.changed.notify_all();
    }
    pub fn failed(&self) -> bool {
        self.transfer_failed.load(Ordering::Acquire)
    }
    fn truncated_wav(&self) -> bool {
        let state = self.state.lock().unwrap();
        if !state.done || state.failed {
            return false;
        }
        let bytes = state.bytes;
        drop(state);
        let mut header = [0u8; 12];
        if self
            .reopen()
            .and_then(|mut file| file.read_exact(&mut header))
            .is_err()
            || &header[..4] != b"RIFF"
            || &header[8..] != b"WAVE"
        {
            return false;
        }
        // RIFF declares a byte count. Decoder duration can be a VBR estimate;
        // it must never decide whether a normally completed stream is truncated.
        // Ignore the all-ones size used by streaming WAV writers.
        let declared = u32::from_le_bytes(header[4..8].try_into().unwrap());
        declared != u32::MAX && u64::from(declared) + 8 > bytes
    }
    #[cfg(test)]
    pub fn is_complete(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.done && !state.failed
    }
    fn reader(self: &Arc<Self>, epoch: Arc<AtomicU64>, version: u64) -> io::Result<GrowingReader> {
        Ok(GrowingReader {
            file: self.reopen()?,
            track: self.clone(),
            epoch,
            version,
            position: 0,
        })
    }
}
// Aborting the download future must wake the decoder too.
pub(super) struct TransferGuard(pub Arc<Track>);
impl Drop for TransferGuard {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        if !state.done {
            state.done = true;
            state.failed = true;
            self.0.transfer_failed.store(true, Ordering::Release);
        }
        self.0.changed.notify_all();
    }
}
struct GrowingReader {
    file: File,
    track: Arc<Track>,
    epoch: Arc<AtomicU64>,
    version: u64,
    position: u64,
}
impl GrowingReader {
    fn check(&self) -> io::Result<()> {
        if self.epoch.load(Ordering::SeqCst) != self.version {
            Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "audio cancelled",
            ))
        } else {
            Ok(())
        }
    }
}
impl Read for GrowingReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let mut state = self.track.state.lock().unwrap();
        loop {
            self.check()?;
            if state.failed {
                return Err(io::Error::other("audio transfer failed"));
            }
            if self.position < state.bytes {
                let len = bytes.len().min((state.bytes - self.position) as usize);
                drop(state);
                let count = self.file.read(&mut bytes[..len])?;
                self.position += count as u64;
                return Ok(count);
            }
            if state.done {
                return Ok(0);
            }
            state = self
                .track
                .changed
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap()
                .0;
        }
    }
}
impl Seek for GrowingReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.check()?;
        let target = match from {
            SeekFrom::Start(pos) => i128::from(pos),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
            SeekFrom::End(offset) => {
                let mut state = self.track.state.lock().unwrap();
                while !state.done {
                    self.check()?;
                    state = self
                        .track
                        .changed
                        .wait_timeout(state, Duration::from_millis(20))
                        .unwrap()
                        .0;
                }
                if state.failed {
                    return Err(io::Error::other("audio transfer failed"));
                }
                i128::from(state.bytes) + i128::from(offset)
            }
        };
        let pos = u64::try_from(target).map_err(|_| io::Error::other("invalid audio position"))?;
        self.file.seek(SeekFrom::Start(pos))?;
        self.position = pos;
        Ok(pos)
    }
}

pub(super) struct Decode {
    pub generation: u64,
    pub version: u64,
    pub track: Arc<Track>,
    pub seconds: f64,
}
pub(super) struct PcmStats {
    pub samples: AtomicU64,
    pub buffering: AtomicBool,
    pub failed: AtomicBool,
    pub ready: AtomicBool,
    pub transfer_failed: Arc<AtomicBool>,
}
impl PcmStats {
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed) || self.transfer_failed.load(Ordering::Acquire)
    }
}
pub(super) struct PcmSource {
    packets: mpsc::Receiver<Vec<f32>>,
    packet: Vec<f32>,
    cursor: usize,
    /// Drained packets go back to the decoder: freeing them here would take the allocator's
    /// lock on the audio callback.
    recycle: mpsc::SyncSender<Vec<f32>>,
    /// This source is the only writer of `stats.samples` and `stats.buffering`.
    consumed: u64,
    buffering: bool,
    channels: u16,
    rate: u32,
    silence: u16,
    pub stats: Arc<PcmStats>,
    pub duration: Option<f64>,
    pub offset: f64,
}
impl PcmSource {
    #[cfg(test)]
    pub fn position(&self) -> f64 {
        self.offset
            + self.stats.samples.load(Ordering::Relaxed) as f64
                / self.channels as f64
                / self.rate as f64
    }
    pub fn format(&self) -> (u16, u32) {
        (self.channels, self.rate)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Instant;

    pub(in crate::playback) fn wav() -> Vec<u8> {
        wav_format(128000, 1, 16000)
    }

    fn wav_format(frames: u32, channels: u16, rate: u32) -> Vec<u8> {
        let samples = frames * u32::from(channels);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend_from_slice(&(channels * 2).to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(samples * 2).to_le_bytes());
        for _ in 0..samples {
            bytes.extend_from_slice(&1000i16.to_le_bytes());
        }
        bytes
    }

    pub(in crate::playback) fn pcm(samples: Vec<f32>, channels: u16, rate: u32) -> PcmSource {
        let duration = samples.len() as f64 / f64::from(channels) / f64::from(rate);
        let (tx, packets) = mpsc::sync_channel(1);
        tx.send(samples).unwrap();
        let (recycle, _) = mpsc::sync_channel(1);
        PcmSource {
            packets,
            packet: Vec::new(),
            cursor: 0,
            recycle,
            consumed: 0,
            buffering: false,
            channels,
            rate,
            silence: 0,
            stats: Arc::new(PcmStats {
                samples: AtomicU64::new(0),
                buffering: AtomicBool::new(false),
                failed: AtomicBool::new(false),
                ready: AtomicBool::new(true),
                transfer_failed: Arc::new(AtomicBool::new(false)),
            }),
            duration: Some(duration),
            offset: 0.,
        }
    }

    #[test]
    fn pcm_warmup_is_time_based_and_a_full_lane_remains_cancellable() {
        for (rate, channels) in [(8000, 1), (48000, 2), (192000, 2), (384000, 2)] {
            let bytes = wav_format(rate, channels, rate);
            let track = Track::new().unwrap();
            track.reopen().unwrap().write_all(&bytes).unwrap();
            track.progress(bytes.len() as u64);
            track.finish(false);
            let epoch = Arc::new(AtomicU64::new(1));
            let (jobs, rx) = mpsc::channel();
            let (commands, events) = mpsc::channel();
            let worker_epoch = epoch.clone();
            let worker = std::thread::spawn(move || decoder_worker(rx, worker_epoch, commands));
            jobs.send(Decode {
                generation: 1,
                version: 1,
                track: track.clone(),
                seconds: 0.,
            })
            .unwrap();
            let Command::Decoded(1, 1, Ok(source)) =
                events.recv_timeout(Duration::from_secs(2)).unwrap()
            else {
                panic!("valid PCM");
            };
            let deadline = Instant::now() + Duration::from_secs(2);
            while !source.stats.ready.load(Ordering::Acquire) {
                assert!(
                    Instant::now() < deadline,
                    "warmup must fit in the bounded lane at {rate} Hz"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(source.position(), 0.);
            std::thread::sleep(Duration::from_millis(10));
            epoch.store(2, Ordering::SeqCst);
            jobs.send(Decode {
                generation: 2,
                version: 2,
                track,
                seconds: 0.,
            })
            .unwrap();
            assert!(matches!(
                events.recv_timeout(Duration::from_secs(2)).unwrap(),
                Command::Decoded(2, 2, Ok(_))
            ));
            drop(source);
            epoch.store(0, Ordering::SeqCst);
            drop(jobs);
            worker.join().unwrap();
        }
    }

    #[test]
    fn short_tracks_become_ready_at_eof_before_the_warmup_threshold() {
        let bytes = wav_format(80, 1, 16000);
        let track = Track::new().unwrap();
        track.reopen().unwrap().write_all(&bytes).unwrap();
        track.progress(bytes.len() as u64);
        track.finish(false);
        let epoch = Arc::new(AtomicU64::new(1));
        let (jobs, rx) = mpsc::channel();
        let (commands, events) = mpsc::channel();
        let worker_epoch = epoch.clone();
        let worker = std::thread::spawn(move || decoder_worker(rx, worker_epoch, commands));
        jobs.send(Decode {
            generation: 1,
            version: 1,
            track,
            seconds: 0.,
        })
        .unwrap();
        let Command::Decoded(1, 1, Ok(mut source)) =
            events.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("short PCM must be playable");
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while !source.stats.ready.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(source.by_ref().count(), 80);
        assert!(!source.stats.failed.load(Ordering::Relaxed));
        epoch.store(0, Ordering::SeqCst);
        drop(jobs);
        worker.join().unwrap();
    }

    #[test]
    fn transfer_failure_reaches_buffered_pcm_without_waiting_for_decoder_eof() {
        for abort in [false, true] {
            let bytes = wav();
            let track = Track::new().unwrap();
            let guard = TransferGuard(track.clone());
            track.reopen().unwrap().write_all(&bytes[..65536]).unwrap();
            track.progress(65536);
            let epoch = Arc::new(AtomicU64::new(1));
            let (jobs, rx) = mpsc::channel();
            let (commands, events) = mpsc::channel();
            let worker_epoch = epoch.clone();
            let worker = std::thread::spawn(move || decoder_worker(rx, worker_epoch, commands));
            jobs.send(Decode {
                generation: 1,
                version: 1,
                track: track.clone(),
                seconds: 0.,
            })
            .unwrap();
            let Command::Decoded(1, 1, Ok(source)) =
                events.recv_timeout(Duration::from_secs(2)).unwrap()
            else {
                panic!("valid progressive PCM");
            };
            let deadline = Instant::now() + Duration::from_secs(2);
            while !source.stats.ready.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(!source.stats.failed());
            if !abort {
                track.finish(true);
            }
            drop(guard);
            assert!(
                source.stats.failed(),
                "queued PCM must expose a failed or aborted transfer immediately"
            );
            epoch.store(0, Ordering::SeqCst);
            drop(source);
            drop(jobs);
            worker.join().unwrap();
        }
    }

    #[test]
    fn partial_file_starts_pcm_without_blocking_output_and_seek_is_cancellable() {
        let bytes = wav();
        let track = Track::new().unwrap();
        let mut writer = track.reopen().unwrap();
        writer.write_all(&bytes[..65536]).unwrap();
        writer.flush().unwrap();
        track.progress(65536);
        let epoch = Arc::new(AtomicU64::new(1));
        let (jobs, rx) = mpsc::channel();
        let (commands, events) = mpsc::channel();
        let worker_epoch = epoch.clone();
        let worker = std::thread::spawn(move || decoder_worker(rx, worker_epoch, commands));
        jobs.send(Decode {
            generation: 1,
            version: 1,
            track: track.clone(),
            seconds: 0.,
        })
        .unwrap();
        let Command::Decoded(1, 1, Ok(mut source)) =
            events.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("progressive decode must start before EOF")
        };
        assert!(!track.is_complete());
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let started = Instant::now();
            assert!(source.next().is_some());
            assert!(
                started.elapsed() < Duration::from_millis(100),
                "callback must not wait on network"
            );
            if source.stats.buffering.load(Ordering::Relaxed) {
                break;
            }
            assert!(Instant::now() < deadline);
        }
        for _ in 0..100 {
            let position = source.position();
            if source.next() == Some(0.) {
                assert_eq!(
                    position,
                    source.position(),
                    "underrun silence is not playback progress"
                );
            }
        }
        epoch.store(2, Ordering::SeqCst);
        drop(source);
        jobs.send(Decode {
            generation: 1,
            version: 2,
            track: track.clone(),
            seconds: 6.,
        })
        .unwrap();
        assert!(
            events.recv_timeout(Duration::from_millis(100)).is_err(),
            "seek must wait for missing bytes"
        );
        epoch.store(3, Ordering::SeqCst);
        let fresh = Track::new().unwrap();
        fresh.reopen().unwrap().write_all(&bytes).unwrap();
        fresh.progress(bytes.len() as u64);
        fresh.finish(false);
        jobs.send(Decode {
            generation: 2,
            version: 3,
            track: fresh,
            seconds: 1.,
        })
        .unwrap();
        loop {
            if let Command::Decoded(2, 3, Ok(source)) =
                events.recv_timeout(Duration::from_secs(2)).unwrap()
            {
                assert_eq!(source.offset, 1.);
                drop(source);
                break;
            }
        }
        epoch.store(4, Ordering::SeqCst);
        drop(jobs);
        worker.join().unwrap();
        let path = track.path().to_owned();
        drop(writer);
        drop(track);
        assert!(!path.exists());
    }
    #[test]
    fn truncated_declared_duration_is_failure_not_normal_eos() {
        let bytes = wav(); // Header declares 8s, HTTP/file will finish after 2s.
        let track = Track::new().unwrap();
        let truncated = &bytes[..44 + 32000 * 2];
        track.reopen().unwrap().write_all(truncated).unwrap();
        track.progress(truncated.len() as u64);
        track.finish(false);
        let epoch = Arc::new(AtomicU64::new(1));
        let (jobs, rx) = mpsc::channel();
        let (commands, events) = mpsc::channel();
        let worker_epoch = epoch.clone();
        let worker = std::thread::spawn(move || decoder_worker(rx, worker_epoch, commands));
        jobs.send(Decode {
            generation: 1,
            version: 1,
            track,
            seconds: 0.,
        })
        .unwrap();
        let Command::Decoded(1, 1, Ok(mut source)) =
            events.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("valid prefix")
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while source.next().is_some() {
            assert!(Instant::now() < deadline);
            if source.stats.buffering.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(source.stats.failed.load(Ordering::Relaxed));
        assert!((source.position() - 2.).abs() < 0.01);
        drop(source);
        epoch.store(2, Ordering::SeqCst);
        drop(jobs);
        worker.join().unwrap();
    }

    #[test]
    fn complete_vbr_mp3_estimated_duration_does_not_mark_eos_failed() {
        // 1s silence + 3s noise, VBR without Xing: estimated length is about 9.8s.
        let bytes = include_bytes!("../../tests/fixtures/vbr-no-xing.mp3");
        let track = Track::new().unwrap();
        track.reopen().unwrap().write_all(bytes).unwrap();
        track.progress(bytes.len() as u64);
        track.finish(false);
        let epoch = Arc::new(AtomicU64::new(1));
        let (jobs, rx) = mpsc::channel();
        let (commands, events) = mpsc::channel();
        let worker_epoch = epoch.clone();
        let worker = std::thread::spawn(move || decoder_worker(rx, worker_epoch, commands));
        jobs.send(Decode {
            generation: 1,
            version: 1,
            track,
            seconds: 0.,
        })
        .unwrap();
        let Command::Decoded(1, 1, Ok(mut source)) =
            events.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("valid complete MP3")
        };
        let estimated = source.duration.unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while source.next().is_some() {
            assert!(Instant::now() < deadline);
            if source.stats.buffering.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!((source.position() - 4.049).abs() < 0.01);
        assert!(
            estimated - source.position() > 1.,
            "fixture must exercise duration estimation"
        );
        assert!(!source.stats.failed.load(Ordering::Relaxed));
        drop(source);
        epoch.store(2, Ordering::SeqCst);
        drop(jobs);
        worker.join().unwrap();
    }
}
impl Iterator for PcmSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.silence > 0 {
            self.silence -= 1;
            return Some(0.);
        }
        loop {
            if let Some(&value) = self.packet.get(self.cursor) {
                self.cursor += 1;
                self.consumed += 1;
                self.stats.samples.store(self.consumed, Ordering::Relaxed);
                if self.buffering {
                    self.buffering = false;
                    self.stats.buffering.store(false, Ordering::Relaxed);
                }
                return Some(value);
            }
            match self.packets.try_recv() {
                Ok(packet) => {
                    let spent = std::mem::replace(&mut self.packet, packet);
                    self.cursor = 0;
                    let _ = self.recycle.try_send(spent);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    if !self.buffering {
                        self.buffering = true;
                        self.stats.buffering.store(true, Ordering::Relaxed);
                    }
                    self.silence = self.channels - 1;
                    return Some(0.);
                }
                Err(mpsc::TryRecvError::Disconnected) => return None,
            }
        }
    }
}
impl Source for PcmSource {
    fn current_span_len(&self) -> Option<usize> {
        // The format is fixed for this source. Artificial spans restart Rodio's
        // resampler and round every 256 frames, accumulating clock drift.
        None
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        self.duration
            .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
    }
}

pub(super) fn decoder_worker(
    rx: mpsc::Receiver<Decode>,
    epoch: Arc<AtomicU64>,
    commands: mpsc::Sender<Command>,
) {
    while let Ok(mut job) = rx.recv() {
        for newer in rx.try_iter() {
            job = newer;
        }
        if job.version != epoch.load(Ordering::SeqCst) {
            continue;
        }
        let result = (|| {
            let reader = job
                .track
                .reader(epoch.clone(), job.version)
                .map_err(|_| "无法读取音频临时文件")?;
            let state = job.track.state.lock().unwrap();
            let complete = state.done && !state.failed;
            let bytes = state.bytes;
            drop(state);
            let mut builder = Decoder::builder().with_data(reader).with_seekable(complete);
            if complete {
                builder = builder.with_byte_len(bytes);
            }
            let mut decoder = builder.build().map_err(|_| "无法解码此音频格式")?;
            let (channels, rate) = (decoder.channels(), decoder.sample_rate());
            if channels == 0 || channels > 8 || !(8000..=384000).contains(&rate) {
                return Err("不支持此音频采样格式");
            }
            let duration = decoder
                .total_duration()
                .map(|d| d.as_secs_f64())
                .or_else(|| {
                    let hint = job.track.duration_hint.load(Ordering::Relaxed);
                    (hint > 0).then_some(hint as f64)
                });
            let offset = duration
                .map(|d| job.seconds.min((d - 0.001).max(0.)))
                .unwrap_or(job.seconds);
            if offset > 0. {
                if complete {
                    decoder
                        .try_seek(Duration::try_from_secs_f64(offset).map_err(|_| "无效播放位置")?)
                        .map_err(|_| "此音频无法跳转到指定位置")?;
                } else {
                    // Until the file is complete, decode/discard to the requested time.
                    // This waits on the decoder thread and remains epoch-cancellable.
                    let skip = (offset * f64::from(rate) * f64::from(channels)) as u64;
                    for sample in 0..skip {
                        if sample % 4096 == 0 && job.version != epoch.load(Ordering::SeqCst) {
                            return Err("播放已取消");
                        }
                        if decoder.next().is_none() {
                            return Err("此音频无法跳转到指定位置");
                        }
                    }
                }
            }
            Ok((decoder, channels, rate, duration, offset))
        })();
        let (mut decoder, channels, rate, duration, offset) = match result {
            Ok(value) => value,
            Err(message) => {
                let _ = commands.send(Command::Decoded(
                    job.generation,
                    job.version,
                    Err(message.into()),
                ));
                continue;
            }
        };
        let stats = Arc::new(PcmStats {
            samples: AtomicU64::new(0),
            buffering: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            transfer_failed: job.track.transfer_failed.clone(),
        });
        let (packets, receiver) = mpsc::sync_channel(8);
        let (recycle, recycled) = mpsc::sync_channel::<Vec<f32>>(16);
        let source = PcmSource {
            packets: receiver,
            packet: Vec::new(),
            cursor: 0,
            recycle,
            consumed: 0,
            buffering: false,
            channels,
            rate,
            silence: 0,
            stats: stats.clone(),
            duration,
            offset,
        };
        let mut source = Some(source);
        let size = (rate as usize / 25).max(256) * usize::from(channels);
        let warmup = u64::from(rate) * u64::from(channels) / 4;
        let mut produced = 0_u64;
        'pump: loop {
            if job.version != epoch.load(Ordering::SeqCst) {
                break;
            }
            let mut packet = recycled
                .try_recv()
                .ok()
                .filter(|packet| packet.capacity() >= size)
                .map(|mut packet| {
                    packet.clear();
                    packet
                })
                .unwrap_or_else(|| Vec::with_capacity(size));
            for sample in 0..size {
                if sample % 512 == 0 && job.version != epoch.load(Ordering::SeqCst) {
                    break 'pump;
                }
                match decoder.next() {
                    Some(value) => packet.push(value),
                    None => break,
                }
            }
            if packet.is_empty() {
                stats.failed.store(
                    job.track.failed() || job.track.truncated_wav(),
                    Ordering::Relaxed,
                );
                stats.ready.store(true, Ordering::Release);
                if source.is_some() {
                    let _ = commands.send(Command::Decoded(
                        job.generation,
                        job.version,
                        Err("音频没有可播放数据".into()),
                    ));
                }
                break;
            }
            let samples = packet.len() as u64;
            loop {
                if job.version != epoch.load(Ordering::SeqCst) {
                    break 'pump;
                }
                match packets.try_send(packet) {
                    Ok(()) => break,
                    Err(mpsc::TrySendError::Disconnected(_)) => break 'pump,
                    Err(mpsc::TrySendError::Full(value)) => {
                        packet = value;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            }
            produced += samples;
            if produced >= warmup {
                stats.ready.store(true, Ordering::Release);
            }
            if let Some(source) = source.take() {
                if commands
                    .send(Command::Decoded(job.generation, job.version, Ok(source)))
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}
