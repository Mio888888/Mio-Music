//! HarmonyOS host gates. ArkTS cannot call into Rust directly, so Dart forwards what
//! the ArkTS host decided (microphone permission, foreground state) over FFI. Capture
//! may begin only for the token issued while the app is in the foreground — the same
//! contract the Android host keeps through JNI.
use std::sync::Mutex;

struct Gate {
    issued: u64,
    active: u64,
    foreground: bool,
}

static GATE: Mutex<Gate> = Mutex::new(Gate {
    issued: 0,
    active: 0,
    foreground: true,
});

pub(crate) fn recording(token: u64, begin: bool) -> Result<(), String> {
    let mut gate = GATE.lock().map_err(|_| "microphone_unavailable")?;
    if begin {
        if token == 0 || gate.active != 0 || gate.issued != token || !gate.foreground {
            return Err("microphone_unavailable".into());
        }
        gate.issued = 0;
        gate.active = token;
    } else if gate.active == token {
        gate.active = 0;
    }
    Ok(())
}

/// Grants one capture to `token` after ArkTS obtained the microphone permission.
#[unsafe(no_mangle)]
pub extern "C" fn mio_ohos_microphone_issue(token: u64) -> i32 {
    let Ok(mut gate) = GATE.lock() else {
        return 0;
    };
    if token == 0 || !gate.foreground || gate.issued != 0 || gate.active != 0 {
        return 0;
    }
    gate.issued = token;
    1
}

/// Withdraws an unused grant; a running capture stops when its owner drops it.
#[unsafe(no_mangle)]
pub extern "C" fn mio_ohos_microphone_release(token: u64) {
    let Ok(mut gate) = GATE.lock() else {
        return;
    };
    if token != 0 && gate.issued == token {
        gate.issued = 0;
        crate::microphone::cancel(token);
    }
}

/// Leaving the foreground revokes the grant and cancels a capture in progress.
#[unsafe(no_mangle)]
pub extern "C" fn mio_ohos_foreground(foreground: u8) {
    let Ok(mut gate) = GATE.lock() else {
        return;
    };
    gate.foreground = foreground != 0;
    if !gate.foreground {
        let token = if gate.issued != 0 {
            gate.issued
        } else {
            gate.active
        };
        gate.issued = 0;
        if token != 0 {
            crate::microphone::cancel(token);
        }
    }
}

/// OHAudio interruptions (calls, other apps taking audio) become the same media
/// events Android's audio focus produces, so Rust's player state follows the system.
pub(crate) fn install_interruptions() {
    cpal::ohaudio::set_interrupt_listener(|interrupt| {
        let command = match interrupt {
            cpal::ohaudio::Interrupt::Pause => 8,
            cpal::ohaudio::Interrupt::Stop => 9,
            cpal::ohaudio::Interrupt::Resume => 10,
        };
        if let Some(runtime_id) = crate::runtime::media_runtime_id() {
            let _ = crate::media::command(runtime_id, command, 0.0);
        }
    });
}
