//! Desktop microphone authorization; the bounded recognition engine is shared.
pub use mio_core::recognition::{Match, Progress, recognize_file};
use std::sync::{Arc, atomic::AtomicBool};
#[cfg(target_os = "macos")]
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

pub fn recognize_microphone(
    cancelled: Arc<AtomicBool>,
    progress: Arc<Progress>,
) -> Result<Vec<Match>, String> {
    mio_core::recognition::recognize_microphone_with(cancelled, progress, microphone_permission)
}

#[cfg(target_os = "macos")]
fn check(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err("识别已取消".into())
    } else {
        Ok(())
    }
}
#[cfg(target_os = "macos")]
fn microphone_permission(cancelled: &AtomicBool) -> Result<(), String> {
    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {}
    unsafe {
        use objc::{class, msg_send, sel, sel_impl};
        let media: *mut objc::runtime::Object =
            msg_send![class!(NSString), stringWithUTF8String: c"soun".as_ptr()];
        let status: i64 =
            msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: media];
        if status == 3 {
            return Ok(());
        }
        if status != 0 {
            return Err("请在系统设置 > 隐私与安全性 > 麦克风中开启权限".into());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let block = block::ConcreteBlock::new(move |granted: i8| {
            let _ = tx.send(granted != 0);
        })
        .copy();
        let _: () = msg_send![class!(AVCaptureDevice), requestAccessForMediaType: media completionHandler: &*block];
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            check(cancelled)?;
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(true) => return Ok(()),
                Ok(false) => return Err("麦克风权限未获授权".into()),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("麦克风授权失败".into());
                }
                Err(_) if Instant::now() >= deadline => return Err("麦克风授权超时".into()),
                Err(_) => {}
            }
        }
    }
}
#[cfg(not(target_os = "macos"))]
fn microphone_permission(_: &AtomicBool) -> Result<(), String> {
    Ok(())
}
