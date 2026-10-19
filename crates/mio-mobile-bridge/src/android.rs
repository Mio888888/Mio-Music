//! Process-lifetime Application context; never retain an Activity.
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JClass, JObject, JValue},
    sys::{jboolean, jdouble, jint, jlong, jstring},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

static CONTEXT: Mutex<Option<GlobalRef>> = Mutex::new(None);
static READY: AtomicBool = AtomicBool::new(false);
static DEVICE: AtomicU32 = AtomicU32::new(0);
pub(crate) fn capabilities() -> Result<crate::api::DeviceCapabilities, String> {
    if !context_ready() {
        return Err("media_host_unavailable".into());
    }
    let flags = DEVICE.load(Ordering::Acquire);
    let television = flags & 1 != 0;
    Ok(crate::api::DeviceCapabilities {
        television,
        limited_memory: flags & 4 != 0,
        microphone: flags & 2 != 0,
        tag_edit: !television,
        dlna: true,
    })
}
struct Host {
    vm: JavaVM,
    class: GlobalRef,
}
static HOST: Mutex<Option<Arc<Host>>> = Mutex::new(None);

pub(crate) fn prepare_playback(runtime_id: u64) -> Result<(), String> {
    let host = HOST
        .lock()
        .map_err(|_| "media_host_failed")?
        .clone()
        .ok_or("media_host_unavailable")?;
    let mut env = host
        .vm
        .attach_current_thread()
        .map_err(|_| "media_host_failed")?;
    let class: &JClass<'_> = host.class.as_obj().into();
    let result = env.call_static_method(
        class,
        "preparePlayback",
        "(J)Z",
        &[JValue::Long(runtime_id as jlong)],
    );
    let ready = result.and_then(|v| v.z()).unwrap_or(false);
    let _ = env.exception_clear();
    if ready {
        Ok(())
    } else {
        Err("media_host_unavailable: foreground service or audio focus denied".into())
    }
}

pub(crate) fn context_ready() -> bool {
    READY.load(Ordering::Acquire)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_vant_mio_1music_NativeMedia_initializeContext(
    mut env: JNIEnv<'_>,
    _instance: JObject<'_>,
    application: JObject<'_>,
    device_flags: jint,
) -> jboolean {
    DEVICE.store(device_flags as u32 & 7, Ordering::Release);
    // Android invokes this on its main thread before Dart can initialize audio.
    let Ok(mut slot) = CONTEXT.lock() else {
        return 0;
    };
    if slot.is_some() {
        return 1;
    }
    let Ok(vm) = env.get_java_vm() else {
        return 0;
    };
    let Ok(context) = env.new_global_ref(application) else {
        return 0;
    };
    let Ok(class) = env.find_class("com/vant/mio_music/NativeMedia") else {
        return 0;
    };
    let Ok(class) = env.new_global_ref(class) else {
        return 0;
    };
    let Ok(mut host) = HOST.lock() else {
        return 0;
    };
    *host = Some(Arc::new(Host { vm, class }));
    let vm = host.as_ref().unwrap().vm.get_java_vm_pointer();
    let raw = context.as_obj().as_raw();
    // SAFETY: vm and the Application global reference remain valid for the
    // process lifetime. This is the sole context initializer, on the main thread.
    unsafe {
        ndk_context::initialize_android_context(vm.cast(), raw.cast());
    }
    *slot = Some(context);
    READY.store(true, Ordering::Release);
    1
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_vant_mio_1music_NativeMedia_snapshot(
    env: JNIEnv<'_>,
    _class: JClass<'_>,
    seen_artwork: jlong,
    lyrics_enabled: jboolean,
) -> jstring {
    std::panic::catch_unwind(|| {
        crate::runtime::media_snapshot(seen_artwork as u64, lyrics_enabled != 0)
            .ok()
            .and_then(|text| env.new_string(text).ok())
            .map_or(std::ptr::null_mut(), |s| s.into_raw())
    })
    .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_vant_mio_1music_NativeMedia_command(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    runtime_id: jlong,
    command: jint,
    value: jdouble,
) -> jboolean {
    crate::media::mio_media_command(runtime_id as u64, command as u32, value) as jboolean
}

/// Host validates an exact foreground permission ticket; no activity is retained in Rust.
pub(crate) fn recording(token: u64, begin: bool) -> Result<(), String> {
    let host = HOST
        .lock()
        .map_err(|_| "microphone_unavailable")?
        .clone()
        .ok_or("microphone_unavailable")?;
    let mut env = host
        .vm
        .attach_current_thread()
        .map_err(|_| "microphone_unavailable")?;
    let class: &JClass<'_> = host.class.as_obj().into();
    let result = env.call_static_method(
        class,
        if begin {
            "beginRecording"
        } else {
            "endRecording"
        },
        "(J)Z",
        &[JValue::Long(token as jlong)],
    );
    let ready = result.and_then(|v| v.z()).unwrap_or(false);
    let _ = env.exception_clear();
    if ready {
        Ok(())
    } else {
        Err("microphone_unavailable".into())
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_vant_mio_1music_NativeMedia_cancelRecognition(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    token: jlong,
) {
    if token > 0 {
        crate::microphone::cancel(token as u64);
    }
}

pub(crate) fn prepare_capture_resume(runtime_id: u64, token: u64) -> Result<(), String> {
    let host = HOST
        .lock()
        .map_err(|_| "microphone_restore_failed")?
        .clone()
        .ok_or("microphone_restore_failed")?;
    let mut env = host
        .vm
        .attach_current_thread()
        .map_err(|_| "microphone_restore_failed")?;
    let class: &JClass<'_> = host.class.as_obj().into();
    let result = env.call_static_method(
        class,
        "prepareCaptureResume",
        "(JJ)Z",
        &[
            JValue::Long(runtime_id as jlong),
            JValue::Long(token as jlong),
        ],
    );
    let ready = result.and_then(|v| v.z()).unwrap_or(false);
    let _ = env.exception_clear();
    if ready {
        Ok(())
    } else {
        Err("microphone_restore_failed".into())
    }
}

/// Rust holds the multicast lock until every SSDP/description worker is joined.
pub(crate) fn multicast(token: u64, acquire: bool) -> Result<(), String> {
    let host = HOST
        .lock()
        .map_err(|_| "dlna_host_failed")?
        .clone()
        .ok_or("dlna_host_unavailable")?;
    let mut env = host
        .vm
        .attach_current_thread()
        .map_err(|_| "dlna_host_failed")?;
    let class: &JClass<'_> = host.class.as_obj().into();
    let result = env.call_static_method(
        class,
        if acquire {
            "beginDlnaSearch"
        } else {
            "endDlnaSearch"
        },
        "(J)Z",
        &[JValue::Long(token as jlong)],
    );
    let ready = result.and_then(|value| value.z()).unwrap_or(false);
    let _ = env.exception_clear();
    if ready {
        Ok(())
    } else {
        Err("dlna_multicast_unavailable".into())
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_vant_mio_1music_NativeMedia_dlnaHandoffActive(
    _env: JNIEnv<'_>,
    _class: JClass<'_>,
    runtime_id: jlong,
) -> jboolean {
    (runtime_id > 0 && crate::runtime::dlna_handoff_active(runtime_id as u64)) as jboolean
}

pub(crate) fn prepare_dlna_resume(runtime_id: u64) -> Result<(), String> {
    let host = HOST
        .lock()
        .map_err(|_| "dlna_restore_failed")?
        .clone()
        .ok_or("dlna_restore_failed")?;
    let mut env = host
        .vm
        .attach_current_thread()
        .map_err(|_| "dlna_restore_failed")?;
    let class: &JClass<'_> = host.class.as_obj().into();
    let result = env.call_static_method(
        class,
        "prepareDlnaResume",
        "(J)Z",
        &[JValue::Long(runtime_id as jlong)],
    );
    let ready = result.and_then(|value| value.z()).unwrap_or(false);
    let _ = env.exception_clear();
    if ready {
        Ok(())
    } else {
        Err("dlna_restore_failed: original audio focus unavailable".into())
    }
}
