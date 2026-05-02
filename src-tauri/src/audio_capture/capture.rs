use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::sync::mpsc::{self, Sender};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

static STOP_SENDER: Mutex<Option<Sender<()>>> = Mutex::new(None);

#[tauri::command]
pub fn audio_capture_start(
    app: AppHandle,
    chunk_duration_ms: Option<u64>,
) -> Result<(), String> {
    stop_internal();

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    *STOP_SENDER.lock() = Some(stop_tx);

    let chunk_ms = chunk_duration_ms.unwrap_or(3000);

    std::thread::spawn(move || {
        if let Err(e) = capture_thread(app, chunk_ms, stop_rx) {
            eprintln!("Audio capture thread error: {}", e);
        }
    });

    Ok(())
}

fn capture_thread(
    app: AppHandle,
    chunk_duration_ms: u64,
    stop_rx: mpsc::Receiver<()>,
) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("未找到麦克风设备".to_string())?;

    let config = device
        .default_input_config()
        .map_err(|e| format!("获取音频配置失败: {}", e))?;

    let native_sample_rate = config.sample_rate().0 as f64;
    let channels = config.channels() as usize;
    let chunk_samples_8k = (chunk_duration_ms as f64 * 8000.0 / 1000.0) as usize;

    let buffer: parking_lot::Mutex<Vec<f32>> =
        parking_lot::Mutex::new(Vec::with_capacity(chunk_samples_8k * 2));
    let last_level_emit: Mutex<Instant> = Mutex::new(Instant::now());

    let _stream = device
        .build_input_stream(
            &config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Calculate RMS for level visualization
                let rms = {
                    let sum: f64 = data.iter().map(|&s| (s as f64) * (s as f64)).sum();
                    (sum / data.len() as f64).sqrt()
                };
                let level = (rms * 5.0).min(1.0) as f32;

                // Emit level at ~15fps
                {
                    let mut last = last_level_emit.lock();
                    if last.elapsed().as_millis() >= 66 {
                        let _ = app.emit(
                            "audio-capture:level",
                            serde_json::json!({ "level": level }),
                        );
                        *last = Instant::now();
                    }
                }

                let mono: Vec<f32> = if channels > 1 {
                    data.chunks(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect()
                } else {
                    data.to_vec()
                };

                let ratio = 8000.0 / native_sample_rate;
                let target_len = (mono.len() as f64 * ratio) as usize;
                let resampled: Vec<f32> = if target_len > 0 {
                    (0..target_len)
                        .map(|i| {
                            let src_idx = i as f64 / ratio;
                            let idx = src_idx as usize;
                            if idx + 1 < mono.len() {
                                let frac = src_idx - idx as f64;
                                (mono[idx] as f64 * (1.0 - frac)
                                    + mono[idx + 1] as f64 * frac)
                                    as f32
                            } else {
                                mono[idx.min(mono.len() - 1)]
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                };

                let mut buf = buffer.lock();
                buf.extend_from_slice(&resampled);

                if buf.len() >= chunk_samples_8k {
                    let chunk: Vec<f32> = buf.drain(..chunk_samples_8k).collect();
                    let _ = app.emit(
                        "audio-capture:chunk",
                        serde_json::json!({
                            "data": chunk,
                            "sampleRate": 8000u32,
                        }),
                    );
                }
            },
            |err| eprintln!("Audio capture error: {}", err),
            None,
        )
        .map_err(|e| format!("创建音频流失败: {}", e))?;

    _stream.play().map_err(|e| format!("启动音频流失败: {}", e))?;

    let _ = stop_rx.recv();

    Ok(())
}

#[tauri::command]
pub fn audio_capture_stop() -> Result<(), String> {
    stop_internal();
    Ok(())
}

fn stop_internal() {
    if let Some(sender) = STOP_SENDER.lock().take() {
        let _ = sender.send(());
    }
}
