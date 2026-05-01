use crate::player::effects::{BalanceSource, BassBoostSource, EqSource, EqState};
use crate::player::spectrum::{PositionSource, PositionState, SpectrumSource, SpectrumState};
use crate::player::{AudioSlot, PlaybackState, PlayerSnapshot, SharedPlayer};
use base64::Engine;
use parking_lot::Mutex;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub struct SlotPipeline {
    sink: Sink,
    url: String,
    eq_state: Arc<Mutex<EqState>>,
    spectrum_state: Arc<Mutex<SpectrumState>>,
    position_state: Arc<Mutex<PositionState>>,
    balance: Arc<Mutex<f64>>,
    bass_boost_gain: Arc<Mutex<f64>>,
    #[allow(dead_code)]
    temp_file: Option<PathBuf>,
}

impl SlotPipeline {
    pub fn new(
        stream_handle: &OutputStreamHandle,
        url: &str,
        volume: f64,
    ) -> Result<Self, String> {
        let file_path = resolve_audio_file(url)?;

        // 先尝试 rodio 解码（MP3/WAV/FLAC/OGG），失败则回退到 symphonia（AAC 等）
        let (channels, sample_rate, duration, source): (u16, u32, f64, PcmSource) =
            match decode_with_rodio(&file_path) {
                Ok(result) => result,
                Err(_) => decode_with_symphonia(&file_path)?,
            };

        let eq_state = Arc::new(Mutex::new(EqState::new(channels, sample_rate)));
        let spectrum_state = Arc::new(Mutex::new(SpectrumState::new()));
        let position_state = Arc::new(Mutex::new(PositionState::new()));
        let balance = Arc::new(Mutex::new(0.0));
        let bass_boost_gain = Arc::new(Mutex::new(0.0));

        {
            let mut ps = position_state.lock();
            ps.sample_rate = sample_rate;
            ps.channels = channels;
            ps.duration_secs = duration;
        }

        // 管线: Decoder → BassBoost → EQ → Balance → Spectrum → Position
        let source = BassBoostSource::new(source, bass_boost_gain.clone(), channels, sample_rate);
        let source = EqSource::new(source, eq_state.clone());
        let source = BalanceSource::new(source, balance.clone());
        let source = SpectrumSource::new(source, spectrum_state.clone());
        let source = PositionSource::new(source, position_state.clone());

        let sink = Sink::try_new(stream_handle)
            .map_err(|e| format!("创建 Sink 失败: {e}"))?;
        sink.set_volume(volume as f32);
        sink.append(source);

        Ok(Self {
            sink,
            url: url.to_string(),
            eq_state,
            spectrum_state,
            position_state,
            balance,
            bass_boost_gain,
            temp_file: None,
        })
    }

    pub fn pause(&self) { self.sink.pause() }
    pub fn resume(&self) { self.sink.play() }
    pub fn seek(&self, pos: Duration) { let _ = self.sink.try_seek(pos); }
    pub fn set_volume(&self, vol: f64) { self.sink.set_volume(vol as f32) }
    pub fn position(&self) -> f64 { self.position_state.lock().position_secs() }
    pub fn duration(&self) -> f64 { self.position_state.lock().duration_secs }
    pub fn is_playing(&self) -> bool { !self.sink.is_paused() && !self.sink.empty() }
    pub fn url(&self) -> &str { &self.url }
    pub fn take_spectrum(&self) -> Option<Vec<f64>> { self.spectrum_state.lock().take_spectrum() }
}

// ==================== URL 解析 ====================

fn resolve_audio_file(url: &str) -> Result<PathBuf, String> {
    if url.starts_with("file://") {
        return Ok(PathBuf::from(&url[7..]));
    }
    if url.starts_with('/') || url.starts_with('~') {
        return Ok(PathBuf::from(url));
    }
    if url.starts_with("data:") {
        return data_uri_to_temp(url);
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return download_to_temp(url);
    }
    Ok(PathBuf::from(url))
}

fn download_to_temp(url: &str) -> Result<PathBuf, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    // 从 URL 域名派生 Referer，酷我/QQ 等 CDN 需要此头才能正常返回音频
    let referer = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| {
            let host = u.host_str()?;
            let parts: Vec<&str> = host.split('.').collect();
            if parts.len() >= 2 {
                Some(format!("http://{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]))
            } else {
                Some(format!("http://{}/", host))
            }
        })
        .unwrap_or_else(|| "http://localhost/".to_string());

    let resp = client.get(url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")
        .header("Referer", &referer)
        .send()
        .map_err(|e| format!("下载音频失败: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("下载音频失败: HTTP {}", status));
    }

    let bytes = resp.bytes()
        .map_err(|e| format!("读取音频数据失败: {e}"))?;

    let temp_dir = std::env::temp_dir().join("lanyin_player");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("创建临时目录失败: {e}"))?;

    let path = temp_dir.join(format!("audio_{}", uuid::Uuid::new_v4()));
    std::fs::write(&path, &bytes)
        .map_err(|e| format!("写入临时文件失败: {e}"))?;

    Ok(path)
}

fn data_uri_to_temp(url: &str) -> Result<PathBuf, String> {
    let parts: Vec<&str> = url.splitn(2, ',').collect();
    if parts.len() != 2 { return Err("无效的 data URI".into()) }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(parts[1].trim())
        .map_err(|e| format!("解码 base64 失败: {e}"))?;

    let temp_dir = std::env::temp_dir().join("lanyin_player");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("创建临时目录失败: {e}"))?;

    let path = temp_dir.join(format!("data_{}", uuid::Uuid::new_v4()));
    std::fs::write(&path, &bytes)
        .map_err(|e| format!("写入临时文件失败: {e}"))?;

    Ok(path)
}

// ==================== 通用 PCM 源（rodio::Source 包装） ====================

type DecodeResult = (u16, u32, f64, PcmSource);

struct PcmSource {
    samples: std::vec::IntoIter<f32>,
    channels: u16,
    sample_rate: u32,
    duration_secs: f64,
}

impl Iterator for PcmSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> { self.samples.next() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.samples.size_hint() }
}

impl rodio::Source for PcmSource {
    fn current_frame_len(&self) -> Option<usize> { Some(self.samples.len()) }
    fn channels(&self) -> u16 { self.channels }
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn total_duration(&self) -> Option<Duration> {
        if self.duration_secs > 0.0 { Some(Duration::from_secs_f64(self.duration_secs)) } else { None }
    }
}

// ==================== rodio 解码（MP3/WAV/FLAC/OGG） ====================

fn decode_with_rodio(file_path: &std::path::Path) -> Result<DecodeResult, String> {
    let file = std::fs::File::open(file_path)
        .map_err(|e| format!("打开音频文件失败: {e}"))?;
    let reader = BufReader::new(file);
    let decoder = Decoder::new(reader)
        .map_err(|e| format!("rodio 解码失败: {e}"))?;

    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let duration_secs = decoder.total_duration().map(|d| d.as_secs_f64()).unwrap_or(0.0);

    let samples: Vec<f32> = decoder.convert_samples().collect();

    Ok((channels, sample_rate, duration_secs, PcmSource {
        samples: samples.into_iter(),
        channels,
        sample_rate,
        duration_secs,
    }))
}

// ==================== symphonia 解码（AAC/M4A 等） ====================

fn decode_with_symphonia(file_path: &std::path::Path) -> Result<DecodeResult, String> {
    let file = std::fs::File::open(file_path)
        .map_err(|e| format!("打开音频文件失败: {e}"))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("探测音频格式失败: {e}"))?;

    let mut format = probed.format;
    let track = format.default_track()
        .ok_or_else(|| "音频文件无默认音轨".to_string())?
        .to_owned();
    let channels = track.codec_params.channels
        .map(|c| c.count() as u16)
        .unwrap_or(2);
    let sample_rate = track.codec_params.sample_rate
        .unwrap_or(44100);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("创建 symphonia 解码器失败: {e}"))?;

    let mut all_samples: Vec<f32> = Vec::new();

    loop {
        match format.next_packet() {
            Ok(packet) => {
                match decoder.decode(&packet) {
                    Ok(decoded) => {
                        let spec = *decoded.spec();
                        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                        buf.copy_interleaved_ref(decoded);
                        all_samples.extend_from_slice(buf.samples());
                    }
                    Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                    Err(e) => return Err(format!("symphonia 解码失败: {e}")),
                }
            }
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(e) => return Err(format!("读取音频包失败: {e}")),
        }
    }

    if all_samples.is_empty() {
        return Err("symphonia 解码得到空音频数据".to_string());
    }

    // 从总采样数反算时长（AAC 容器中 n_frames 通常为 None）
    let duration_secs = all_samples.len() as f64 / channels as f64 / sample_rate as f64;

    Ok((channels, sample_rate, duration_secs, PcmSource {
        samples: all_samples.into_iter(),
        channels,
        sample_rate,
        duration_secs,
    }))
}

// ==================== 交叉淡入淡出 ====================

struct CrossfadeState {
    active: bool,
    start: Instant,
    duration_ms: u64,
}

// ==================== 播放引擎 ====================

pub struct PlayerEngine {
    stream_handle: OutputStreamHandle,
    primary: Option<SlotPipeline>,
    secondary: Option<SlotPipeline>,
    volume: f64,
    shutdown: bool,
    crossfade: Option<CrossfadeState>,
    #[allow(dead_code)]
    shutdown_tx: std::sync::mpsc::Sender<()>,
    app_handle: tauri::AppHandle,
    poll_tick: u32,
}

impl PlayerEngine {
    pub fn new(
        app_handle: tauri::AppHandle,
        stream_handle: OutputStreamHandle,
        shutdown_tx: std::sync::mpsc::Sender<()>,
    ) -> Self {
        Self {
            stream_handle,
            primary: None,
            secondary: None,
            volume: 80.0,
            shutdown: false,
            crossfade: None,
            shutdown_tx,
            app_handle,
            poll_tick: 0,
        }
    }

    pub fn play(&mut self, url: &str, _slot: Option<AudioSlot>) -> Result<(), String> {
        self.primary = Some(SlotPipeline::new(
            &self.stream_handle, url, self.volume / 100.0,
        )?);
        self.emit_state();
        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(ref p) = self.primary { p.pause() }
        self.emit_state();
    }

    pub fn resume(&mut self) {
        if let Some(ref p) = self.primary { p.resume() }
        self.emit_state();
    }

    pub fn stop_slot(&mut self, _slot: AudioSlot) {
        self.primary = None;
        self.emit_state();
    }

    pub fn stop_all(&mut self) {
        self.primary = None;
        self.secondary = None;
        self.emit_state();
    }

    pub fn seek(&self, position_secs: f64) {
        if let Some(ref p) = self.primary {
            p.seek(Duration::from_secs_f64(position_secs));
        }
    }

    pub fn set_volume(&mut self, vol: f64) {
        self.volume = vol.clamp(0.0, 100.0);
        let n = self.volume / 100.0;
        if let Some(ref p) = self.primary { p.set_volume(n) }
        if let Some(ref s) = self.secondary { s.set_volume(n) }
    }

    #[allow(dead_code)]
    pub fn volume(&self) -> f64 { self.volume }

    pub fn crossfade_to(&mut self, url: &str, duration_ms: u64) -> Result<(), String> {
        let pipeline = SlotPipeline::new(&self.stream_handle, url, 0.0)?;
        self.secondary = Some(pipeline);
        self.crossfade = Some(CrossfadeState { active: true, start: Instant::now(), duration_ms });
        Ok(())
    }

    pub fn swap_primary(&mut self) {
        std::mem::swap(&mut self.primary, &mut self.secondary);
        self.emit_state();
    }

    pub fn set_eq_band(&self, index: usize, gain: f64) {
        if let Some(ref p) = self.primary { p.eq_state.lock().set_band(index, gain) }
        if let Some(ref s) = self.secondary { s.eq_state.lock().set_band(index, gain) }
    }

    pub fn get_eq_bands(&self) -> Vec<f64> {
        self.primary.as_ref()
            .map(|p| p.eq_state.lock().gains.to_vec())
            .unwrap_or_else(|| vec![0.0; 10])
    }

    pub fn set_bass_boost(&self, gain: f64) {
        if let Some(ref p) = self.primary { *p.bass_boost_gain.lock() = gain }
        if let Some(ref s) = self.secondary { *s.bass_boost_gain.lock() = gain }
    }

    pub fn set_balance(&self, value: f64) {
        if let Some(ref p) = self.primary { *p.balance.lock() = value }
        if let Some(ref s) = self.secondary { *s.balance.lock() = value }
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        let p = self.primary.as_ref();
        let state = p.map(|p| {
            if p.is_playing() { PlaybackState::Playing }
            else { PlaybackState::Paused }
        }).unwrap_or(PlaybackState::Stopped);

        PlayerSnapshot {
            state,
            position: p.map(|p| p.position()).unwrap_or(0.0),
            duration: p.map(|p| p.duration()).unwrap_or(0.0),
            volume: self.volume,
            primary_slot: AudioSlot::A,
            url: p.map(|p| p.url().to_string()).unwrap_or_default(),
            is_playing: state == PlaybackState::Playing,
        }
    }

    pub fn is_shutdown(&self) -> bool { self.shutdown }
    pub fn shutdown(&mut self) { self.shutdown = true; self.stop_all() }

    pub fn poll(&mut self) {
        if self.shutdown { return }
        self.poll_tick = self.poll_tick.wrapping_add(1);
        self.tick_crossfade();

        if let Some(ref p) = self.primary {
            // 频谱：每 2 tick 发一次（~60Hz poll / 2 = ~30FPS）
            if self.poll_tick % 2 == 0 {
                if let Some(bands) = p.take_spectrum() {
                    let _ = self.app_handle.emit("player:spectrum", serde_json::json!({ "bands": bands }));
                }
            }

            // 时间：每 3 tick 发一次（~20Hz）
            if self.poll_tick % 3 == 0 && p.is_playing() {
                let _ = self.app_handle.emit("player:time", serde_json::json!({
                    "position": p.position(), "duration": p.duration(),
                }));
            }

            if p.sink.empty() && !p.sink.is_paused() {
                let _ = self.app_handle.emit("player:ended", serde_json::json!({ "slot": "A" }));
            }
        }
    }

    fn tick_crossfade(&mut self) {
        let Some(ref mut cf) = self.crossfade else { return };
        if !cf.active { return }

        let progress = (cf.start.elapsed().as_millis() as f64 / cf.duration_ms as f64).min(1.0);
        let vol = self.volume / 100.0;

        if let Some(ref p) = self.primary { p.set_volume(vol * (1.0 - progress)) }
        if let Some(ref s) = self.secondary { s.set_volume(vol * progress) }

        if progress >= 1.0 {
            self.primary = self.secondary.take();
            cf.active = false;
        }
    }

    fn emit_state(&self) {
        let s = self.snapshot();
        let _ = self.app_handle.emit("player:state", serde_json::json!({
            "state": format!("{:?}", s.state), "position": s.position, "duration": s.duration,
            "volume": s.volume, "url": s.url, "isPlaying": s.is_playing,
        }));
    }
}

// ==================== 初始化：专用音频线程保持 OutputStream 存活 ====================

pub fn start_bus_poller(shared: SharedPlayer) {
    std::thread::spawn(move || {
        loop {
            {
                let mut engine = shared.lock();
                if engine.is_shutdown() { break }
                engine.poll();
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    });
}

pub fn create_output_stream() -> Result<(OutputStreamHandle, std::sync::mpsc::Sender<()>), String> {
    let (handle_tx, handle_rx) = std::sync::mpsc::channel();
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel::<()>();

    std::thread::spawn(move || {
        match OutputStream::try_default() {
            Ok((_stream, handle)) => {
                if handle_tx.send(Ok(handle)).is_err() { return }
                let _ = shutdown_rx.recv();
                drop(_stream);
            }
            Err(e) => { let _ = handle_tx.send(Err(format!("{e}"))); }
        }
    });

    let handle = handle_rx.recv().map_err(|e| format!("音频线程通信失败: {e}"))??;
    Ok((handle, shutdown_tx))
}
