//! Bounded native audio capture and the original application's AFP fingerprint algorithm.
use base64::Engine as _;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::Source;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Read,
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
        if item.source != "wy"
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

#[derive(Default)]
struct Bindings {
    functions: HashMap<String, (u32, i32)>,
    byte_types: std::collections::HashSet<i32>,
    byte_values: HashMap<i32, (u8, u32)>,
    next_byte_handle: i32,
    limits: wasmi::StoreLimits,
}
impl Bindings {
    fn release_byte(&mut self, handle: i32) -> Result<u8, wasmi::Error> {
        let (value, references) = self
            .byte_values
            .get_mut(&handle)
            .ok_or_else(|| wasmi::Error::new("invalid byte handle"))?;
        *references = references
            .checked_sub(1)
            .ok_or_else(|| wasmi::Error::new("invalid byte reference count"))?;
        let byte = *value;
        if *references == 0 {
            self.byte_values.remove(&handle);
        }
        Ok(byte)
    }
}
fn fingerprint(pcm: &[f32], cancelled: &AtomicBool) -> Result<String, String> {
    use wasmi::{Caller, Config, Engine, ExternType, Linker, Module, Store, Val};
    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let binary = include_bytes!("../assets/recognition/afp.wasm");
    let module = Module::new(&engine, &binary[..]).map_err(|_| "无法加载指纹模块")?;
    let mut store = Store::new(
        &engine,
        Bindings {
            next_byte_handle: 5, // Emscripten reserves handles 0 through 4.
            limits: wasmi::StoreLimitsBuilder::new()
                .memory_size(64 * 1024 * 1024)
                .build(),
            ..Bindings::default()
        },
    );
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(600_000_000)
        .map_err(|_| "指纹计算初始化失败")?;
    let mut linker = Linker::new(&engine);
    for import in module.imports() {
        let ExternType::Func(ty) = import.ty() else {
            return Err("不支持的指纹模块导入".into());
        };
        let name = import.name().to_owned();
        linker
            .func_new(
                import.module(),
                import.name(),
                ty.clone(),
                move |mut caller: Caller<'_, Bindings>, args, result| {
                    let arg = |i: usize| args.get(i).and_then(Val::i32).unwrap_or(0);
                    match name.as_str() {
                        "b" => {
                            // Only one-byte integer types can cross the AFP vector boundary.
                            if arg(2) == 1 {
                                caller.data_mut().byte_types.insert(arg(0));
                            }
                        }
                        "o" => {
                            // _emval_take_value returns a handle, not the byte itself.
                            if !caller.data().byte_types.contains(&arg(0)) {
                                return Err(wasmi::Error::new("unsupported byte type"));
                            }
                            let memory = caller
                                .get_export("B")
                                .and_then(wasmi::Extern::into_memory)
                                .ok_or_else(|| wasmi::Error::new("missing memory"))?;
                            let mut byte = [0];
                            memory
                                .read(&caller, arg(1) as usize, &mut byte)
                                .map_err(|_| wasmi::Error::new("invalid byte address"))?;
                            let [value @ Val::I32(_)] = result else {
                                return Err(wasmi::Error::new("invalid byte result type"));
                            };
                            let state = caller.data_mut();
                            if state.next_byte_handle >= 5 + 65536 {
                                return Err(wasmi::Error::new("byte handle budget exceeded"));
                            }
                            let handle = state.next_byte_handle;
                            state.next_byte_handle += 1;
                            state.byte_values.insert(handle, (byte[0], 1));
                            *value = Val::I32(handle);
                            return Ok(());
                        }
                        "n" => {
                            let (_, references) = caller
                                .data_mut()
                                .byte_values
                                .get_mut(&arg(0))
                                .ok_or_else(|| wasmi::Error::new("invalid byte handle"))?;
                            *references = references
                                .checked_add(1)
                                .ok_or_else(|| wasmi::Error::new("byte reference overflow"))?;
                        }
                        "m" => {
                            caller.data_mut().release_byte(arg(0))?;
                        }
                        "l" | "c" => {
                            let memory = caller
                                .get_export("B")
                                .and_then(wasmi::Extern::into_memory)
                                .ok_or_else(|| wasmi::Error::new("missing memory"))?;
                            let ptr = arg(if name == "l" { 0 } else { 1 }) as usize;
                            let data = memory.data(&caller);
                            let tail = data
                                .get(ptr..)
                                .ok_or_else(|| wasmi::Error::new("invalid binding"))?;
                            let len = tail
                                .iter()
                                .take(512)
                                .position(|v| *v == 0)
                                .ok_or_else(|| wasmi::Error::new("invalid binding name"))?;
                            let binding = std::str::from_utf8(&tail[..len])
                                .map_err(|_| wasmi::Error::new("invalid binding name"))?
                                .to_owned();
                            let (wrapper, context) = if name == "l" {
                                (arg(4) as u32, arg(5))
                            } else {
                                (arg(5) as u32, arg(6))
                            };
                            caller
                                .data_mut()
                                .functions
                                .insert(binding, (wrapper, context));
                        }
                        "r" => {
                            let memory = caller
                                .get_export("B")
                                .and_then(wasmi::Extern::into_memory)
                                .ok_or_else(|| wasmi::Error::new("missing memory"))?;
                            let (dst, src, len) =
                                (arg(0) as usize, arg(1) as usize, arg(2) as usize);
                            let data = memory.data_mut(&mut caller);
                            if src.checked_add(len).is_none_or(|end| end > data.len())
                                || dst.checked_add(len).is_none_or(|end| end > data.len())
                            {
                                return Err(wasmi::Error::new("invalid copy"));
                            }
                            data.copy_within(src..src + len, dst);
                        }
                        "h" | "d" | "f" | "s" => {
                            return Err(wasmi::Error::new("fingerprint runtime error"));
                        }
                        // Embind type/class registration and unused libc output hooks.
                        _ => {}
                    }
                    for value in result {
                        *value = Val::default(value.ty());
                    }
                    Ok(())
                },
            )
            .map_err(|_| "指纹模块初始化失败")?;
    }
    let instance = linker
        .instantiate(&mut store, &module)
        .and_then(|i| i.start(&mut store))
        .map_err(|_| "指纹模块初始化失败")?;
    instance
        .get_typed_func::<(), ()>(&store, "C")
        .and_then(|f| f.call(&mut store, ()))
        .map_err(|_| "指纹绑定初始化失败")?;
    let memory = instance.get_memory(&store, "B").ok_or("指纹模块缺少内存")?;
    let malloc = instance
        .get_typed_func::<i32, i32>(&store, "E")
        .map_err(|_| "指纹模块缺少分配器")?;
    let length = pcm.len() * 4;
    let ptr = malloc
        .call(&mut store, (length + 5) as i32)
        .map_err(|_| "指纹内存分配失败")?;
    let mut bytes = Vec::with_capacity(length + 5);
    bytes.extend_from_slice(&(length as u32).to_le_bytes());
    for sample in pcm {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes.push(0);
    memory
        .write(&mut store, ptr as usize, &bytes)
        .map_err(|_| "指纹输入失败")?;
    let table = instance
        .get_table(&store, "D")
        .ok_or("指纹模块缺少函数表")?;
    let invoke = |store: &mut Store<Bindings>, name: &str, params: &[i32]| -> Result<i32, String> {
        let (index, context) = *store
            .data()
            .functions
            .get(name)
            .ok_or_else(|| format!("指纹模块缺少 {name}"))?;
        let function = table
            .get(&*store, index)
            .and_then(|v| v.funcref().and_then(|r| r.func().copied()))
            .ok_or("无效指纹函数")?;
        let mut args = vec![Val::I32(context)];
        args.extend(params.iter().copied().map(Val::I32));
        let mut result = [Val::I32(0)];
        function
            .call(store, &args, &mut result)
            .map_err(|_| "指纹计算失败")?;
        result[0].i32().ok_or("无效指纹结果".into())
    };
    check(cancelled)?;
    let vector = invoke(&mut store, "ExtractQueryFP", &[ptr])?;
    check(cancelled)?;
    let size = invoke(&mut store, "size", &[vector])?;
    if !(1..=65536).contains(&size) {
        return Err("未能提取音频指纹".into());
    }
    let mut output = Vec::with_capacity(size as usize);
    for i in 0..size {
        check(cancelled)?;
        let handle = invoke(&mut store, "get", &[vector, i])?;
        // Match emscripten::val's fromWireType: read and release the returned handle.
        output.push(
            store
                .data_mut()
                .release_byte(handle)
                .map_err(|_| "无效指纹结果")?,
        );
    }
    if !store.data().byte_values.is_empty() || output.iter().all(|byte| *byte == 0) {
        return Err("未能提取音频指纹".into());
    }
    // The entire isolated instance, input and vector allocations are released here.
    Ok(base64::engine::general_purpose::STANDARD.encode(output))
}
fn query(pcm: &[f32], cancelled: &AtomicBool) -> Result<Vec<Match>, String> {
    check(cancelled)?;
    if pcm.len() < 8000 || !pcm.iter().any(|s| s.abs() > 0.002) {
        return Err("未检测到足够的声音，请重试".into());
    }
    let fp = fingerprint(pcm, cancelled)?;
    check(cancelled)?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法初始化识别请求")?;
    let response = client
        .post("https://interface.music.163.com/api/music/audio/match")
        .query(&[
            ("sessionId", uuid::Uuid::new_v4().to_string()),
            ("algorithmCode", "shazam_v2".into()),
            ("duration", (pcm.len() as f64 / 8000.).to_string()),
            ("rawdata", fp),
            ("times", "1".into()),
            ("decrypt", "1".into()),
        ])
        .send()
        .map_err(|_| "识别服务请求失败，请稍后重试")?;
    if !response.status().is_success() {
        return Err("识别服务请求失败，请稍后重试".into());
    }
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取识别结果")?;
    check(cancelled)?;
    if bytes.len() > 1024 * 1024 {
        return Err("识别结果过大".into());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "识别服务返回无效数据")?;
    if json
        .get("code")
        .and_then(|v| v.as_i64())
        .is_some_and(|c| c != 200)
    {
        return Err("识别服务暂时不可用".into());
    }
    let rows = json
        .pointer("/data/result")
        .and_then(|v| v.as_array())
        .ok_or("未识别到歌曲，请重试")?;
    let mut matches = Vec::new();
    for row in rows.iter().take(20) {
        let song = &row["song"];
        let id = song["id"]
            .as_u64()
            .map(|id| id.to_string())
            .or_else(|| song["id"].as_str().map(str::to_owned))
            .unwrap_or_default();
        let name = song["name"].as_str().unwrap_or_default();
        if id.is_empty() || id.len() > 512 || name.is_empty() || name.len() > 2048 {
            continue;
        }
        let artists = song["artists"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|a| a["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_default();
        let album = &song["album"];
        let mut img = album["picUrl"]
            .as_str()
            .or_else(|| album["cover"].as_str())
            .or_else(|| album["img1v1Url"].as_str())
            .unwrap_or_default()
            .replace("http:", "https:");
        if !img.is_empty() && !img.starts_with("http") {
            img = format!("https://p1.music.126.net/{img}/{img}.jpg");
        }
        if !img.is_empty() && !img.contains("param=") {
            img.push_str("?param=100y100");
        }
        if artists.len() > 4096
            || album["name"].as_str().unwrap_or_default().len() > 2048
            || img.len() > 4096
        {
            continue;
        }
        let mut types = Vec::new();
        for (key, quality) in [
            ("hrMusic", "hires"),
            ("sqMusic", "flac"),
            ("hMusic", "320k"),
            ("mMusic", "320k"),
            ("lMusic", "128k"),
        ] {
            if song[key].is_object() && !types.contains(&quality) {
                types.push(quality);
            }
        }
        let item = serde_json::from_value(serde_json::json!({"songmid":id,"name":name,"singer":artists,"albumName":album["name"].as_str().unwrap_or_default(),"source":"wy","img":img,"types":types})).map_err(|_| "无效歌曲信息")?;
        matches.push(Match {
            item,
            timestamp: chrono::Utc::now().timestamp_millis(),
            start_time_ms: row["startTime"]
                .as_u64()
                .unwrap_or(0)
                .min(24 * 60 * 60 * 1000),
        });
    }
    if matches.is_empty() {
        return Err("未识别到歌曲，请重试".into());
    }
    Ok(matches)
}
