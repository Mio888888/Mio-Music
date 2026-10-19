//! The subset of `ohaudio/native_audiostream*.h` this host uses. OHAudio enums are C
//! `int`s, so they cross as `i32`.
#![allow(non_camel_case_types)]

use std::ffi::c_void;

pub const AUDIOSTREAM_SUCCESS: i32 = 0;

pub const AUDIOSTREAM_TYPE_RENDERER: i32 = 1;
pub const AUDIOSTREAM_TYPE_CAPTURER: i32 = 2;

pub const AUDIOSTREAM_SAMPLE_S16LE: i32 = 1;
pub const AUDIOSTREAM_SAMPLE_F32LE: i32 = 4;

pub const AUDIOSTREAM_ENCODING_TYPE_RAW: i32 = 0;
pub const AUDIOSTREAM_USAGE_MUSIC: i32 = 1;
pub const AUDIOSTREAM_SOURCE_TYPE_MIC: i32 = 0;

pub const AUDIOSTREAM_INTERRUPT_SHARE: i32 = 1;
pub const AUDIOSTREAM_INTERRUPT_HINT_RESUME: i32 = 1;
pub const AUDIOSTREAM_INTERRUPT_HINT_PAUSE: i32 = 2;
pub const AUDIOSTREAM_INTERRUPT_HINT_STOP: i32 = 3;

#[repr(C)]
pub struct OH_AudioStreamBuilder {
    _private: [u8; 0],
}

#[repr(C)]
pub struct OH_AudioRenderer {
    _private: [u8; 0],
}

#[repr(C)]
pub struct OH_AudioCapturer {
    _private: [u8; 0],
}

#[repr(C)]
pub struct OH_AudioRenderer_Callbacks {
    pub on_write_data: Option<
        unsafe extern "C" fn(*mut OH_AudioRenderer, *mut c_void, *mut c_void, i32) -> i32,
    >,
    pub on_stream_event: Option<unsafe extern "C" fn(*mut OH_AudioRenderer, *mut c_void, i32) -> i32>,
    pub on_interrupt_event:
        Option<unsafe extern "C" fn(*mut OH_AudioRenderer, *mut c_void, i32, i32) -> i32>,
    pub on_error: Option<unsafe extern "C" fn(*mut OH_AudioRenderer, *mut c_void, i32) -> i32>,
}

#[repr(C)]
pub struct OH_AudioCapturer_Callbacks {
    pub on_read_data: Option<
        unsafe extern "C" fn(*mut OH_AudioCapturer, *mut c_void, *mut c_void, i32) -> i32,
    >,
    pub on_stream_event: Option<unsafe extern "C" fn(*mut OH_AudioCapturer, *mut c_void, i32) -> i32>,
    pub on_interrupt_event:
        Option<unsafe extern "C" fn(*mut OH_AudioCapturer, *mut c_void, i32, i32) -> i32>,
    pub on_error: Option<unsafe extern "C" fn(*mut OH_AudioCapturer, *mut c_void, i32) -> i32>,
}

#[link(name = "ohaudio")]
extern "C" {
    pub fn OH_AudioStreamBuilder_Create(builder: *mut *mut OH_AudioStreamBuilder, kind: i32) -> i32;
    pub fn OH_AudioStreamBuilder_Destroy(builder: *mut OH_AudioStreamBuilder) -> i32;
    pub fn OH_AudioStreamBuilder_SetSamplingRate(builder: *mut OH_AudioStreamBuilder, rate: i32)
        -> i32;
    pub fn OH_AudioStreamBuilder_SetChannelCount(
        builder: *mut OH_AudioStreamBuilder,
        channels: i32,
    ) -> i32;
    pub fn OH_AudioStreamBuilder_SetSampleFormat(
        builder: *mut OH_AudioStreamBuilder,
        format: i32,
    ) -> i32;
    pub fn OH_AudioStreamBuilder_SetEncodingType(
        builder: *mut OH_AudioStreamBuilder,
        encoding: i32,
    ) -> i32;
    pub fn OH_AudioStreamBuilder_SetFrameSizeInCallback(
        builder: *mut OH_AudioStreamBuilder,
        frames: i32,
    ) -> i32;
    pub fn OH_AudioStreamBuilder_SetRendererInfo(builder: *mut OH_AudioStreamBuilder, usage: i32)
        -> i32;
    pub fn OH_AudioStreamBuilder_SetCapturerInfo(
        builder: *mut OH_AudioStreamBuilder,
        source: i32,
    ) -> i32;
    pub fn OH_AudioStreamBuilder_SetRendererCallback(
        builder: *mut OH_AudioStreamBuilder,
        callbacks: OH_AudioRenderer_Callbacks,
        user_data: *mut c_void,
    ) -> i32;
    pub fn OH_AudioStreamBuilder_SetCapturerCallback(
        builder: *mut OH_AudioStreamBuilder,
        callbacks: OH_AudioCapturer_Callbacks,
        user_data: *mut c_void,
    ) -> i32;
    pub fn OH_AudioStreamBuilder_GenerateRenderer(
        builder: *mut OH_AudioStreamBuilder,
        renderer: *mut *mut OH_AudioRenderer,
    ) -> i32;
    pub fn OH_AudioStreamBuilder_GenerateCapturer(
        builder: *mut OH_AudioStreamBuilder,
        capturer: *mut *mut OH_AudioCapturer,
    ) -> i32;

    pub fn OH_AudioRenderer_Start(renderer: *mut OH_AudioRenderer) -> i32;
    pub fn OH_AudioRenderer_Pause(renderer: *mut OH_AudioRenderer) -> i32;
    pub fn OH_AudioRenderer_Stop(renderer: *mut OH_AudioRenderer) -> i32;
    pub fn OH_AudioRenderer_Release(renderer: *mut OH_AudioRenderer) -> i32;

    pub fn OH_AudioCapturer_Start(capturer: *mut OH_AudioCapturer) -> i32;
    pub fn OH_AudioCapturer_Stop(capturer: *mut OH_AudioCapturer) -> i32;
    pub fn OH_AudioCapturer_Release(capturer: *mut OH_AudioCapturer) -> i32;
}
