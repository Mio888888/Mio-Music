//! System media keys and the "Now Playing" entry. The original's `media_control.rs` only
//! publishes the title and playing state on macOS and leaves its remote-command setup as an
//! unused stub, so the keys never reach the app there; here the keys (play, pause, play/pause,
//! next, previous) and the lock-screen/Control-Center position slider drive the player.
//!
//! macOS uses `MPRemoteCommandCenter` / `MPNowPlayingInfoCenter`; Windows the System Media
//! Transport Controls and Linux MPRIS over D-Bus, both through `souvlaki`. Handlers run on a
//! system thread and only queue a [`Command`]; the shell drains the queue from its polling task.

/// What the user asked for through a media key or the system's now-playing controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    /// Absolute position in seconds.
    Seek(f64),
    /// Relative to the current position, in seconds (negative goes back). Windows and Linux.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    SeekBy(f64),
}

/// What the system shows as "now playing".
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub position: f64,
    pub playing: bool,
    /// The already decoded player cover; no network or disk work in the media publisher.
    pub cover: Option<std::sync::Arc<gpui_kit::Image>>,
}

use std::sync::Mutex;

static QUEUE: Mutex<Vec<Command>> = Mutex::new(Vec::new());

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn push(command: Command) {
    if let Ok(mut queue) = QUEUE.lock() {
        // A held key repeats; the queue is drained every few tens of milliseconds.
        if queue.len() < 32 {
            queue.push(command);
        }
    }
}

/// Everything the system asked for since the last call.
pub fn drain() -> Vec<Command> {
    QUEUE
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Command, NowPlaying, push};
    use block::ConcreteBlock;
    use cocoa::foundation::NSSize;
    use objc::rc::StrongPtr;
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};
    use std::ffi::{CString, c_void};

    #[link(name = "MediaPlayer", kind = "framework")]
    unsafe extern "C" {
        static MPMediaItemPropertyArtwork: *mut Object;
    }

    unsafe fn artwork(cover: &gpui_kit::Image) -> Option<StrongPtr> {
        unsafe {
            let data_class = Class::get("NSData")?;
            let image_class = Class::get("NSImage")?;
            let artwork_class = Class::get("MPMediaItemArtwork")?;
            let bytes = cover.bytes();
            let data: *mut Object = msg_send![data_class,
                dataWithBytes: bytes.as_ptr() length: bytes.len()];
            let image: *mut Object = msg_send![image_class, alloc];
            let image: *mut Object = msg_send![image, initWithData: data];
            if image.is_null() {
                return None;
            }
            let image = StrongPtr::new(image);
            let size: NSSize = msg_send![*image, size];
            // The artwork owns a copy of this block; its captured retain keeps the image
            // alive when the system asks for it after this call or after a track switch.
            let trace = std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some();
            let cover_id = cover.id;
            let handler = ConcreteBlock::new(move |requested: NSSize| -> *mut Object {
                if trace {
                    eprintln!(
                        "[media] artwork_requested cover_id={cover_id} width={} height={}",
                        requested.width, requested.height
                    );
                }
                *image
            })
            .copy();
            let artwork: *mut Object = msg_send![artwork_class, alloc];
            let artwork: *mut Object = msg_send![artwork,
                initWithBoundsSize: size requestHandler: &*handler];
            (!artwork.is_null()).then(|| StrongPtr::new(artwork))
        }
    }

    unsafe fn ns_string(value: &str) -> *mut Object {
        let c = CString::new(value.replace('\0', "")).unwrap_or_default();
        let class = Class::get("NSString").expect("NSString");
        unsafe { msg_send![class, stringWithUTF8String: c.as_ptr()] }
    }

    unsafe fn ns_number(value: f64) -> *mut Object {
        let class = Class::get("NSNumber").expect("NSNumber");
        unsafe { msg_send![class, numberWithDouble: value] }
    }

    /// Keeps the registered handlers' lifetime; the system holds the blocks after `copy`.
    pub struct MediaControl {
        center: *mut Object,
    }

    impl MediaControl {
        pub fn new(_window: Option<*mut c_void>) -> Result<Self, String> {
            unsafe {
                let class =
                    Class::get("MPRemoteCommandCenter").ok_or("MPRemoteCommandCenter missing")?;
                let center: *mut Object = msg_send![class, sharedCommandCenter];
                if center.is_null() {
                    return Err("no remote command center".into());
                }
                let simple: [(&str, Command); 5] = [
                    ("playCommand", Command::Play),
                    ("pauseCommand", Command::Pause),
                    ("togglePlayPauseCommand", Command::Toggle),
                    ("nextTrackCommand", Command::Next),
                    ("previousTrackCommand", Command::Previous),
                ];
                for (name, command) in simple {
                    let cmd: *mut Object = match name {
                        "playCommand" => msg_send![center, playCommand],
                        "pauseCommand" => msg_send![center, pauseCommand],
                        "togglePlayPauseCommand" => msg_send![center, togglePlayPauseCommand],
                        "nextTrackCommand" => msg_send![center, nextTrackCommand],
                        _ => msg_send![center, previousTrackCommand],
                    };
                    if cmd.is_null() {
                        continue;
                    }
                    let _: () = msg_send![cmd, setEnabled: true];
                    // MPRemoteCommandHandlerStatusSuccess = 0.
                    let block = ConcreteBlock::new(move |_event: *mut Object| -> isize {
                        push(command);
                        0
                    })
                    .copy();
                    let _: *mut Object = msg_send![cmd, addTargetWithHandler: &*block];
                    std::mem::forget(block);
                }
                let seek: *mut Object = msg_send![center, changePlaybackPositionCommand];
                if !seek.is_null() {
                    let _: () = msg_send![seek, setEnabled: true];
                    let block = ConcreteBlock::new(move |event: *mut Object| -> isize {
                        let seconds: f64 = msg_send![event, positionTime];
                        push(Command::Seek(seconds));
                        0
                    })
                    .copy();
                    let _: *mut Object = msg_send![seek, addTargetWithHandler: &*block];
                    std::mem::forget(block);
                }
                Ok(Self { center })
            }
        }

        /// Publishes the track, position and playing state. The system advances the elapsed
        /// time by itself while the rate is 1, so this is only called when something changed.
        pub fn set_now_playing(&self, info: Option<&NowPlaying>) {
            unsafe {
                let Some(center_class) = Class::get("MPNowPlayingInfoCenter") else {
                    return;
                };
                let center: *mut Object = msg_send![center_class, defaultCenter];
                if center.is_null() {
                    return;
                }
                let _ = self.center;
                let Some(info) = info else {
                    let nil: *mut Object = std::ptr::null_mut();
                    let _: () = msg_send![center, setNowPlayingInfo: nil];
                    // MPNowPlayingPlaybackStateStopped = 3.
                    let _: () = msg_send![center, setPlaybackState: 3u64];
                    return;
                };
                let dict_class = Class::get("NSMutableDictionary").expect("NSMutableDictionary");
                let dict: *mut Object = msg_send![dict_class, dictionary];
                let set = |key: &str, value: *mut Object| {
                    let key = ns_string(key);
                    let _: () = msg_send![dict, setObject: value forKey: key];
                };
                set("title", ns_string(&info.title));
                set("artist", ns_string(&info.artist));
                set("albumTitle", ns_string(&info.album));
                set("playbackDuration", ns_number(info.duration));
                if let Some(cover) = info.cover.as_deref().and_then(|cover| artwork(cover)) {
                    let _: () =
                        msg_send![dict, setObject: *cover forKey: MPMediaItemPropertyArtwork];
                }
                set(
                    "MPNowPlayingInfoPropertyElapsedPlaybackTime",
                    ns_number(info.position),
                );
                set(
                    "MPNowPlayingInfoPropertyPlaybackRate",
                    ns_number(if info.playing { 1. } else { 0. }),
                );
                let _: () = msg_send![center, setNowPlayingInfo: dict];
                // MPNowPlayingPlaybackStatePlaying = 1, Paused = 2.
                let state: u64 = if info.playing { 1 } else { 2 };
                let _: () = msg_send![center, setPlaybackState: state];
            }
        }
    }
}

/// SMTC on Windows (needs the window's HWND) and MPRIS on Linux.
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod imp {
    use super::{Command, NowPlaying, push};
    use souvlaki::{
        MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition,
        PlatformConfig, SeekDirection,
    };
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::time::Duration;

    /// Distance of the system's fast-forward / rewind buttons, which carry no amount.
    const SEEK_STEP: f64 = 5.;

    pub struct MediaControl {
        controls: RefCell<MediaControls>,
    }

    impl MediaControl {
        pub fn new(window: Option<*mut c_void>) -> Result<Self, String> {
            if cfg!(target_os = "windows") && window.is_none() {
                return Err("the system media controls need a window handle".into());
            }
            let mut controls = MediaControls::new(PlatformConfig {
                display_name: "Mio Music",
                dbus_name: "mio_music_gpui",
                hwnd: window,
            })
            .map_err(|e| format!("{e:?}"))?;
            controls
                .attach(|event| {
                    let command = match event {
                        MediaControlEvent::Play => Command::Play,
                        MediaControlEvent::Pause | MediaControlEvent::Stop => Command::Pause,
                        MediaControlEvent::Toggle => Command::Toggle,
                        MediaControlEvent::Next => Command::Next,
                        MediaControlEvent::Previous => Command::Previous,
                        MediaControlEvent::Seek(direction) => Command::SeekBy(match direction {
                            SeekDirection::Forward => SEEK_STEP,
                            SeekDirection::Backward => -SEEK_STEP,
                        }),
                        MediaControlEvent::SeekBy(direction, amount) => {
                            let seconds = amount.as_secs_f64();
                            Command::SeekBy(match direction {
                                SeekDirection::Forward => seconds,
                                SeekDirection::Backward => -seconds,
                            })
                        }
                        MediaControlEvent::SetPosition(MediaPosition(position)) => {
                            Command::Seek(position.as_secs_f64())
                        }
                        // The app has no remote volume, URI opening or raise/quit handling.
                        _ => return,
                    };
                    push(command);
                })
                .map_err(|e| format!("{e:?}"))?;
            Ok(Self {
                controls: RefCell::new(controls),
            })
        }

        /// Publishes the track, position and playing state; called when something changed.
        pub fn set_now_playing(&self, info: Option<&NowPlaying>) {
            let mut controls = self.controls.borrow_mut();
            let Some(info) = info else {
                let _ = controls.set_metadata(MediaMetadata::default());
                let _ = controls.set_playback(MediaPlayback::Stopped);
                return;
            };
            let seconds = |value: f64| Duration::from_secs_f64(value.max(0.));
            let _ = controls.set_metadata(MediaMetadata {
                title: Some(&info.title),
                artist: Some(&info.artist),
                album: Some(&info.album),
                cover_url: None,
                duration: (info.duration > 0.).then(|| seconds(info.duration)),
            });
            let progress = Some(MediaPosition(seconds(info.position)));
            let _ = controls.set_playback(if info.playing {
                MediaPlayback::Playing { progress }
            } else {
                MediaPlayback::Paused { progress }
            });
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod imp {
    use super::NowPlaying;
    use std::ffi::c_void;

    pub struct MediaControl;
    impl MediaControl {
        pub fn new(_: Option<*mut c_void>) -> Result<Self, String> {
            Err("System media keys are not implemented on this platform".into())
        }
        pub fn set_now_playing(&self, _info: Option<&NowPlaying>) {}
    }
}

pub use imp::MediaControl;
