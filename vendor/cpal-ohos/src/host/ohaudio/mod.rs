//! OpenHarmony host built on OHAudio (`libohaudio.so`).
//!
//! Streams use the callback-struct API from API 10 so they run on every HarmonyOS NEXT
//! release; the per-event setters that replace it need API 20. OHAudio owns the buffer it
//! passes to each callback, so cpal reads or fills it in place.

use std::ffi::c_void;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use std::vec::IntoIter as VecIntoIter;

use crate::traits::{DeviceTrait, HostTrait, StreamTrait};
use crate::{
    BackendSpecificError, BufferSize, BuildStreamError, Data, DefaultStreamConfigError,
    DeviceNameError, DevicesError, InputCallbackInfo, InputStreamTimestamp, OutputCallbackInfo,
    OutputStreamTimestamp, PauseStreamError, PlayStreamError, SampleFormat, SampleRate,
    StreamConfig, StreamError, StreamInstant, SupportedBufferSize, SupportedStreamConfig,
    SupportedStreamConfigRange, SupportedStreamConfigsError,
};

mod ffi;

// OHAudio accepts any rate in this range and up to two channels for every stream type.
const MIN_SAMPLE_RATE: u32 = 8000;
const MAX_SAMPLE_RATE: u32 = 192_000;
const DEFAULT_SAMPLE_RATE: u32 = 48_000;

pub struct Host;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device;

pub type Devices = VecIntoIter<Device>;
pub type SupportedInputConfigs = VecIntoIter<SupportedStreamConfigRange>;
pub type SupportedOutputConfigs = VecIntoIter<SupportedStreamConfigRange>;

impl Host {
    pub fn new() -> Result<Self, crate::HostUnavailable> {
        Ok(Host)
    }
}

impl HostTrait for Host {
    type Devices = Devices;
    type Device = Device;

    fn is_available() -> bool {
        true
    }

    fn devices(&self) -> Result<Self::Devices, DevicesError> {
        Ok(vec![Device].into_iter())
    }

    fn default_input_device(&self) -> Option<Device> {
        Some(Device)
    }

    fn default_output_device(&self) -> Option<Device> {
        Some(Device)
    }
}

fn supported_configs(formats: &[SampleFormat]) -> VecIntoIter<SupportedStreamConfigRange> {
    let mut configs = Vec::with_capacity(formats.len() * 2);
    for &sample_format in formats {
        for channels in [2, 1] {
            configs.push(SupportedStreamConfigRange::new(
                channels,
                SampleRate(MIN_SAMPLE_RATE),
                SampleRate(MAX_SAMPLE_RATE),
                SupportedBufferSize::Unknown,
                sample_format,
            ));
        }
    }
    configs.into_iter()
}

fn ohaudio_format(sample_format: SampleFormat) -> Result<i32, BuildStreamError> {
    match sample_format {
        SampleFormat::I16 => Ok(ffi::AUDIOSTREAM_SAMPLE_S16LE),
        SampleFormat::F32 => Ok(ffi::AUDIOSTREAM_SAMPLE_F32LE),
        other => Err(backend(format!("{other} format is not supported by OHAudio")).into()),
    }
}

fn backend(description: String) -> BackendSpecificError {
    BackendSpecificError { description }
}

fn check(result: i32, call: &str) -> Result<(), BackendSpecificError> {
    if result == ffi::AUDIOSTREAM_SUCCESS {
        Ok(())
    } else {
        Err(backend(format!("{call} failed with OHAudio error {result}")))
    }
}

fn to_stream_instant(duration: Duration) -> StreamInstant {
    StreamInstant::new(duration.as_secs() as i64, duration.subsec_nanos())
}

/// Owns an `OH_AudioStreamBuilder` so every early return destroys it.
struct Builder(*mut ffi::OH_AudioStreamBuilder);

impl Builder {
    fn new(
        stream_type: i32,
        config: &StreamConfig,
        sample_format: SampleFormat,
    ) -> Result<Self, BuildStreamError> {
        let format = ohaudio_format(sample_format)?;
        if !(1..=2).contains(&config.channels) {
            return Err(backend("OHAudio streams support one or two channels".to_owned()).into());
        }
        let rate = config.sample_rate.0;
        if !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&rate) {
            return Err(BuildStreamError::StreamConfigNotSupported);
        }
        let mut raw = std::ptr::null_mut();
        check(
            unsafe { ffi::OH_AudioStreamBuilder_Create(&mut raw, stream_type) },
            "OH_AudioStreamBuilder_Create",
        )?;
        let builder = Builder(raw);
        unsafe {
            check(
                ffi::OH_AudioStreamBuilder_SetSamplingRate(raw, rate as i32),
                "SetSamplingRate",
            )?;
            check(
                ffi::OH_AudioStreamBuilder_SetChannelCount(raw, config.channels as i32),
                "SetChannelCount",
            )?;
            check(
                ffi::OH_AudioStreamBuilder_SetSampleFormat(raw, format),
                "SetSampleFormat",
            )?;
            check(
                ffi::OH_AudioStreamBuilder_SetEncodingType(raw, ffi::AUDIOSTREAM_ENCODING_TYPE_RAW),
                "SetEncodingType",
            )?;
            if let BufferSize::Fixed(frames) = config.buffer_size {
                // A hint only: OHAudio may keep its own period, and cpal tolerates that.
                ffi::OH_AudioStreamBuilder_SetFrameSizeInCallback(raw, frames as i32);
            }
        }
        Ok(builder)
    }
}

impl Drop for Builder {
    fn drop(&mut self) {
        unsafe { ffi::OH_AudioStreamBuilder_Destroy(self.0) };
    }
}

type ErrorCallback = Mutex<Box<dyn FnMut(StreamError) + Send>>;

/// What the system did to a playing stream when other audio (a call, another app)
/// took over. OHAudio pauses or stops the renderer itself; the player should follow
/// so its state matches what the user hears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interrupt {
    Pause,
    Resume,
    Stop,
}

static INTERRUPTS: Mutex<Option<fn(Interrupt)>> = Mutex::new(None);
static OUTPUTS: Mutex<Vec<Weak<Mutex<Renderer>>>> = Mutex::new(Vec::new());

/// Receives renderer interruptions, in addition to the renderer's own handling.
pub fn set_interrupt_listener(listener: fn(Interrupt)) {
    if let Ok(mut slot) = INTERRUPTS.lock() {
        *slot = Some(listener);
    }
}

/// Restarts renderers that an interruption paused or stopped. A player calls this
/// when the user resumes, the counterpart of regaining audio focus on Android;
/// OHAudio never restarts a stopped renderer on its own.
pub fn restart_interrupted() {
    let Ok(mut outputs) = OUTPUTS.lock() else {
        return;
    };
    outputs.retain(|output| match output.upgrade() {
        Some(renderer) => {
            if let Ok(mut renderer) = renderer.lock() {
                renderer.restart();
            }
            true
        }
        None => false,
    });
}

/// A renderer handle shared by its stream, its callbacks and `restart_interrupted`.
/// `live` turns false before the stream releases the handle.
struct Renderer {
    handle: *mut ffi::OH_AudioRenderer,
    live: bool,
    interrupted: bool,
}

unsafe impl Send for Renderer {}

impl Renderer {
    fn restart(&mut self) {
        if self.live && self.interrupted {
            self.interrupted = false;
            unsafe { ffi::OH_AudioRenderer_Start(self.handle) };
        }
    }
}

struct OutputState {
    data: Mutex<Box<dyn FnMut(&mut Data, &OutputCallbackInfo) + Send>>,
    error: ErrorCallback,
    sample_format: SampleFormat,
    created: Instant,
    renderer: Arc<Mutex<Renderer>>,
}

struct InputState {
    data: Mutex<Box<dyn FnMut(&Data, &InputCallbackInfo) + Send>>,
    error: ErrorCallback,
    sample_format: SampleFormat,
    created: Instant,
}

fn report(error: &ErrorCallback, code: i32) {
    if let Ok(mut callback) = error.lock() {
        callback(StreamError::BackendSpecific {
            err: backend(format!("OHAudio stream error {code}")),
        });
    }
}

unsafe extern "C" fn on_write_data(
    _renderer: *mut ffi::OH_AudioRenderer,
    user_data: *mut c_void,
    buffer: *mut c_void,
    length: i32,
) -> i32 {
    let state = &*(user_data as *const OutputState);
    let len = length.max(0) as usize / state.sample_format.sample_size();
    let mut data = Data::from_parts(buffer as *mut (), len, state.sample_format);
    let now = to_stream_instant(state.created.elapsed());
    let info = OutputCallbackInfo {
        timestamp: OutputStreamTimestamp {
            callback: now,
            playback: now,
        },
    };
    match state.data.lock() {
        Ok(mut callback) => callback(&mut data, &info),
        // A panicked callback leaves garbage in the buffer; play silence instead.
        Err(_) => std::ptr::write_bytes(buffer as *mut u8, 0, length.max(0) as usize),
    }
    0
}

unsafe extern "C" fn on_renderer_error(
    _renderer: *mut ffi::OH_AudioRenderer,
    user_data: *mut c_void,
    error: i32,
) -> i32 {
    report(&(*(user_data as *const OutputState)).error, error);
    0
}

unsafe extern "C" fn on_read_data(
    _capturer: *mut ffi::OH_AudioCapturer,
    user_data: *mut c_void,
    buffer: *mut c_void,
    length: i32,
) -> i32 {
    let state = &*(user_data as *const InputState);
    let len = length.max(0) as usize / state.sample_format.sample_size();
    let data = Data::from_parts(buffer as *mut (), len, state.sample_format);
    let now = to_stream_instant(state.created.elapsed());
    let info = InputCallbackInfo {
        timestamp: InputStreamTimestamp {
            callback: now,
            capture: now,
        },
    };
    if let Ok(mut callback) = state.data.lock() {
        callback(&data, &info);
    }
    0
}

unsafe extern "C" fn on_capturer_error(
    _capturer: *mut ffi::OH_AudioCapturer,
    user_data: *mut c_void,
    error: i32,
) -> i32 {
    report(&(*(user_data as *const InputState)).error, error);
    0
}

// Routing changes need no action: OHAudio moves the stream to the new device itself.
unsafe extern "C" fn on_renderer_event(
    _renderer: *mut ffi::OH_AudioRenderer,
    _user_data: *mut c_void,
    _event: i32,
) -> i32 {
    0
}

unsafe extern "C" fn on_renderer_interrupt(
    _renderer: *mut ffi::OH_AudioRenderer,
    user_data: *mut c_void,
    force_type: i32,
    hint: i32,
) -> i32 {
    let interrupt = match hint {
        ffi::AUDIOSTREAM_INTERRUPT_HINT_RESUME => Interrupt::Resume,
        ffi::AUDIOSTREAM_INTERRUPT_HINT_PAUSE => Interrupt::Pause,
        ffi::AUDIOSTREAM_INTERRUPT_HINT_STOP => Interrupt::Stop,
        // Ducking and muting are applied by the system and need no player change.
        _ => return 0,
    };
    let state = &*(user_data as *const OutputState);
    // try_lock: the stream may be dropping, and it never waits for this callback.
    if let Ok(mut renderer) = state.renderer.try_lock() {
        match interrupt {
            Interrupt::Resume => renderer.restart(),
            Interrupt::Pause | Interrupt::Stop if renderer.live => {
                renderer.interrupted = true;
                // A shared interruption leaves pausing to the app.
                if force_type == ffi::AUDIOSTREAM_INTERRUPT_SHARE {
                    ffi::OH_AudioRenderer_Pause(renderer.handle);
                }
            }
            _ => {}
        }
    }
    if let Some(listener) = INTERRUPTS.lock().ok().and_then(|slot| *slot) {
        listener(interrupt);
    }
    0
}

unsafe extern "C" fn on_capturer_event(
    _capturer: *mut ffi::OH_AudioCapturer,
    _user_data: *mut c_void,
    _event: i32,
) -> i32 {
    0
}

unsafe extern "C" fn on_capturer_interrupt(
    _capturer: *mut ffi::OH_AudioCapturer,
    _user_data: *mut c_void,
    _force_type: i32,
    _hint: i32,
) -> i32 {
    0
}

enum Inner {
    Output {
        renderer: *mut ffi::OH_AudioRenderer,
        // Read by the audio thread through `userData` until the renderer is released.
        state: Box<OutputState>,
    },
    Input {
        capturer: *mut ffi::OH_AudioCapturer,
        _state: Box<InputState>,
    },
}

pub struct Stream(Inner);

// The renderer and capturer handles may be driven from any thread; their callbacks only
// touch the boxed state, which is itself `Send`.
unsafe impl Send for Stream {}

impl Drop for Stream {
    fn drop(&mut self) {
        // Release stops the callbacks before the boxed state they point at is dropped.
        unsafe {
            match &self.0 {
                Inner::Output { renderer, state } => {
                    if let Ok(mut shared) = state.renderer.lock() {
                        shared.live = false;
                    }
                    ffi::OH_AudioRenderer_Stop(*renderer);
                    ffi::OH_AudioRenderer_Release(*renderer);
                }
                Inner::Input { capturer, .. } => {
                    ffi::OH_AudioCapturer_Stop(*capturer);
                    ffi::OH_AudioCapturer_Release(*capturer);
                }
            }
        }
    }
}

impl DeviceTrait for Device {
    type SupportedInputConfigs = SupportedInputConfigs;
    type SupportedOutputConfigs = SupportedOutputConfigs;
    type Stream = Stream;

    fn name(&self) -> Result<String, DeviceNameError> {
        Ok("default".to_owned())
    }

    fn supported_input_configs(
        &self,
    ) -> Result<Self::SupportedInputConfigs, SupportedStreamConfigsError> {
        Ok(supported_configs(&[SampleFormat::I16]))
    }

    fn supported_output_configs(
        &self,
    ) -> Result<Self::SupportedOutputConfigs, SupportedStreamConfigsError> {
        Ok(supported_configs(&[SampleFormat::F32, SampleFormat::I16]))
    }

    // The microphone delivers 16-bit PCM on every device; float capture is not guaranteed.
    fn default_input_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError> {
        Ok(SupportedStreamConfig::new(
            1,
            SampleRate(DEFAULT_SAMPLE_RATE),
            SupportedBufferSize::Unknown,
            SampleFormat::I16,
        ))
    }

    fn default_output_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError> {
        Ok(SupportedStreamConfig::new(
            2,
            SampleRate(DEFAULT_SAMPLE_RATE),
            SupportedBufferSize::Unknown,
            SampleFormat::F32,
        ))
    }

    fn build_input_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        data_callback: D,
        error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        D: FnMut(&Data, &InputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        let builder = Builder::new(ffi::AUDIOSTREAM_TYPE_CAPTURER, config, sample_format)?;
        let state = Box::new(InputState {
            data: Mutex::new(Box::new(data_callback)),
            error: Mutex::new(Box::new(error_callback)),
            sample_format,
            created: Instant::now(),
        });
        let callbacks = ffi::OH_AudioCapturer_Callbacks {
            on_read_data: Some(on_read_data),
            on_stream_event: Some(on_capturer_event),
            on_interrupt_event: Some(on_capturer_interrupt),
            on_error: Some(on_capturer_error),
        };
        let mut capturer = std::ptr::null_mut();
        unsafe {
            check(
                ffi::OH_AudioStreamBuilder_SetCapturerInfo(
                    builder.0,
                    ffi::AUDIOSTREAM_SOURCE_TYPE_MIC,
                ),
                "SetCapturerInfo",
            )?;
            check(
                ffi::OH_AudioStreamBuilder_SetCapturerCallback(
                    builder.0,
                    callbacks,
                    &*state as *const InputState as *mut c_void,
                ),
                "SetCapturerCallback",
            )?;
            check(
                ffi::OH_AudioStreamBuilder_GenerateCapturer(builder.0, &mut capturer),
                "GenerateCapturer",
            )?;
        }
        Ok(Stream(Inner::Input {
            capturer,
            _state: state,
        }))
    }

    fn build_output_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        data_callback: D,
        error_callback: E,
        _timeout: Option<Duration>,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        D: FnMut(&mut Data, &OutputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        let builder = Builder::new(ffi::AUDIOSTREAM_TYPE_RENDERER, config, sample_format)?;
        let state = Box::new(OutputState {
            data: Mutex::new(Box::new(data_callback)),
            error: Mutex::new(Box::new(error_callback)),
            sample_format,
            created: Instant::now(),
            renderer: Arc::new(Mutex::new(Renderer {
                handle: std::ptr::null_mut(),
                live: false,
                interrupted: false,
            })),
        });
        let callbacks = ffi::OH_AudioRenderer_Callbacks {
            on_write_data: Some(on_write_data),
            on_stream_event: Some(on_renderer_event),
            on_interrupt_event: Some(on_renderer_interrupt),
            on_error: Some(on_renderer_error),
        };
        let mut renderer = std::ptr::null_mut();
        unsafe {
            check(
                ffi::OH_AudioStreamBuilder_SetRendererInfo(builder.0, ffi::AUDIOSTREAM_USAGE_MUSIC),
                "SetRendererInfo",
            )?;
            check(
                ffi::OH_AudioStreamBuilder_SetRendererCallback(
                    builder.0,
                    callbacks,
                    &*state as *const OutputState as *mut c_void,
                ),
                "SetRendererCallback",
            )?;
            check(
                ffi::OH_AudioStreamBuilder_GenerateRenderer(builder.0, &mut renderer),
                "GenerateRenderer",
            )?;
        }
        if let Ok(mut shared) = state.renderer.lock() {
            shared.handle = renderer;
            shared.live = true;
        }
        if let Ok(mut outputs) = OUTPUTS.lock() {
            outputs.push(Arc::downgrade(&state.renderer));
        }
        Ok(Stream(Inner::Output { renderer, state }))
    }
}

impl StreamTrait for Stream {
    fn play(&self) -> Result<(), PlayStreamError> {
        let result = unsafe {
            match &self.0 {
                Inner::Output { renderer, state } => {
                    if let Ok(mut shared) = state.renderer.lock() {
                        shared.interrupted = false;
                    }
                    ffi::OH_AudioRenderer_Start(*renderer)
                }
                Inner::Input { capturer, .. } => ffi::OH_AudioCapturer_Start(*capturer),
            }
        };
        check(result, "Start").map_err(Into::into)
    }

    fn pause(&self) -> Result<(), PauseStreamError> {
        let result = unsafe {
            match &self.0 {
                Inner::Output { renderer, .. } => ffi::OH_AudioRenderer_Pause(*renderer),
                // Capturers cannot pause; stopping keeps them restartable with `play`.
                Inner::Input { capturer, .. } => ffi::OH_AudioCapturer_Stop(*capturer),
            }
        };
        check(result, "Pause").map_err(Into::into)
    }
}
