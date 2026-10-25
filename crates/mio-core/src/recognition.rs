//! Bounded native audio capture. Identifying the audio is the job of a recognition plugin (the
//! `recognize` capability of a list plugin); this module only supplies it 8 kHz mono samples.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::Source;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

pub const MAX_SECONDS: usize = 15;
#[derive(Clone, Serialize, Deserialize)]
pub struct Match {
    pub item: mio_music_sdk::MusicItem,
    pub timestamp: i64,
    #[serde(default)]
    pub start_time_ms: u64,
}
impl Match {
    pub fn validate(&self) -> Result<(), String> {
        let item = &self.item;
        if !mio_music_sdk::is_custom_source_id(&item.source)
            || item
                .songmid
                .as_str()
                .is_none_or(|id| id.is_empty() || id.len() > 512)
            || item.name.is_empty()
            || item.name.len() > 2048
            || item.singer.len() > 4096
            || item.album_name.len() > 2048
            || item.img.len() > 4096
            || self.timestamp < 0
            || self.start_time_ms > 86400000
            || item
                .types
                .as_ref()
                .is_some_and(|types| types.len() > 10 || types.iter().any(|v| v.len() > 64))
        {
            return Err("recognition_invalid".into());
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Progress {
    pub seconds: AtomicU32,
    pub level: AtomicU32,
}
pub fn recognize_file(path: &Path, cancelled: &AtomicBool) -> Result<Vec<Match>, String> {
    let file = std::fs::File::open(path).map_err(|_| "无法读取音频文件")?;
    if file.metadata().map_err(|_| "无法读取文件信息")?.len() > 128 * 1024 * 1024 {
        return Err("音频文件过大（上限 128 MB）".into());
    }
    let source = rodio::Decoder::try_from(file).map_err(|_| "无法解码音频文件")?;
    let rate = source.sample_rate();
    let channels = source.channels() as usize;
    if rate == 0 || rate > 384000 || channels == 0 || channels > 32 {
        return Err("不支持的音频格式".into());
    }
    let mut pcm = Vec::with_capacity(120000);
    let mut samples = source.take(rate as usize * channels * MAX_SECONDS);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut phase = 0u64;
    loop {
        check(cancelled)?;
        if Instant::now() > deadline {
            return Err("音频解码超时".into());
        }
        let mut sum = 0.;
        let mut count = 0;
        for _ in 0..channels {
            if let Some(value) = samples.next() {
                sum += value;
                count += 1;
            }
        }
        if count == 0 {
            break;
        }
        phase += 8000;
        while phase >= rate as u64 {
            pcm.push(sum / count as f32);
            phase -= rate as u64;
        }
        if pcm.len() >= 120000 {
            break;
        }
    }
    query(&pcm, cancelled)
}
fn check(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err("识别已取消".into())
    } else {
        Ok(())
    }
}

/// The host grants microphone permission and configures its audio session here.
/// Authorization runs after capture serialization and before accessing a device.
pub fn recognize_microphone_with<G>(
    cancelled: Arc<AtomicBool>,
    progress: Arc<Progress>,
    authorize: impl FnOnce(&AtomicBool) -> Result<G, String>,
) -> Result<Vec<Match>, String> {
    static CAPTURE: Mutex<()> = Mutex::new(());
    let deadline = Instant::now() + Duration::from_secs(1);
    let _exclusive = loop {
        check(&cancelled)?;
        match CAPTURE.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::Poisoned(_)) => return Err("麦克风采集状态不可用".into()),
            Err(_) if Instant::now() >= deadline => return Err("麦克风仍在停止，请稍后重试".into()),
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let authorization = authorize(&cancelled)?;
    check(&cancelled)?;
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("未找到麦克风设备")?;
    let config = device
        .default_input_config()
        .map_err(|_| "无法访问麦克风设备")?;
    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    if rate == 0 || rate > 384000 || channels == 0 || channels > 32 {
        return Err("不支持的麦克风格式".into());
    }
    let buffer = Arc::new(Mutex::new((Vec::<f32>::with_capacity(120000), 0u64)));
    let failed = Arc::new(AtomicBool::new(false));
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => input::<f32>(
            &device,
            &config.into(),
            channels,
            rate,
            buffer.clone(),
            cancelled.clone(),
            progress.clone(),
            failed.clone(),
        ),
        cpal::SampleFormat::I16 => input::<i16>(
            &device,
            &config.into(),
            channels,
            rate,
            buffer.clone(),
            cancelled.clone(),
            progress.clone(),
            failed.clone(),
        ),
        cpal::SampleFormat::U16 => input::<u16>(
            &device,
            &config.into(),
            channels,
            rate,
            buffer.clone(),
            cancelled.clone(),
            progress.clone(),
            failed.clone(),
        ),
        _ => return Err("不支持的麦克风采样格式".into()),
    }
    .map_err(|_| "无法启动麦克风，请检查系统权限")?;
    stream.play().map_err(|_| "无法启动麦克风")?;
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(15) {
        check(&cancelled)?;
        if failed.load(Ordering::Relaxed) {
            return Err("麦克风设备已断开".into());
        }
        progress
            .seconds
            .store(began.elapsed().as_secs() as u32, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(50));
    }
    // Capture ends before fingerprinting or network I/O; dropping also releases it on all errors.
    drop(stream);
    drop(authorization);
    drop(_exclusive);
    progress.level.store(0, Ordering::Relaxed);
    let pcm = std::mem::take(&mut buffer.lock().map_err(|_| "音频采集失败")?.0);
    query(&pcm, &cancelled)
}
fn input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    rate: u32,
    buffer: Arc<Mutex<(Vec<f32>, u64)>>,
    cancelled: Arc<AtomicBool>,
    progress: Arc<Progress>,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }
            let Ok(mut state) = buffer.try_lock() else {
                return;
            };
            let mut peak = 0f32;
            for frame in data.chunks_exact(channels) {
                let sample = frame
                    .iter()
                    .map(|v| <f32 as cpal::FromSample<T>>::from_sample_(*v))
                    .sum::<f32>()
                    / channels as f32;
                peak = peak.max(sample.abs());
                state.1 += 8000;
                while state.1 >= rate as u64 {
                    if state.0.len() < 120000 {
                        state.0.push(sample);
                    }
                    state.1 -= rate as u64;
                }
            }
            progress
                .level
                .store((peak.clamp(0., 1.) * 100.) as u32, Ordering::Relaxed);
        },
        move |_| {
            failed.store(true, Ordering::Relaxed);
        },
        None,
    )
}

/// Whether a plugin that identifies songs from audio is installed and enabled. Without one
/// there is nothing to offer, so the interface hides recognition.
pub fn available() -> bool {
    mio_music_sdk::recognizer().is_some()
}
fn query(pcm: &[f32], cancelled: &AtomicBool) -> Result<Vec<Match>, String> {
    check(cancelled)?;
    if pcm.len() < 8000 || !pcm.iter().any(|s| s.abs() > 0.002) {
        return Err("未检测到足够的声音，请重试".into());
    }
    let provider = mio_music_sdk::recognizer().ok_or("未安装或未启用支持识曲的插件")?;
    let rows = provider.recognize(pcm, cancelled)?;
    check(cancelled)?;
    let timestamp = chrono::Utc::now().timestamp_millis();
    let matches: Vec<Match> = rows
        .into_iter()
        .map(|row| Match {
            item: row.item,
            timestamp,
            start_time_ms: row.start_time_ms,
        })
        .filter(|row| row.validate().is_ok())
        .collect();
    if matches.is_empty() {
        return Err("未识别到歌曲，请重试".into());
    }
    Ok(matches)
}
