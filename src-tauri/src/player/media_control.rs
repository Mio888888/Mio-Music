use crate::player::SharedPlayer;
use parking_lot::Mutex;

pub struct MediaControl {
    initialized: bool,
}

impl MediaControl {
    pub fn new() -> Self {
        Self { initialized: false }
    }

    pub fn update_now_playing(
        &mut self,
        _title: &str,
        _artist: &str,
        _album: &str,
        _duration_secs: f64,
        _cover_url: Option<&str>,
    ) {
        #[cfg(target_os = "macos")]
        {
            self.update_now_playing_macos(_title, _artist, _album, _duration_secs, _cover_url);
        }
        self.initialized = true;
    }

    pub fn set_playback_state(&self, _playing: bool) {
        #[cfg(target_os = "macos")]
        {
            self.set_playback_state_macos(_playing);
        }
    }

    #[allow(dead_code)]
    pub fn setup_command_handlers(&self, _player: SharedPlayer) {
        #[cfg(target_os = "macos")]
        {
            self.setup_macos_command_handlers(_player);
        }
    }
}

#[cfg(target_os = "macos")]
use objc::runtime::Object;

#[cfg(target_os = "macos")]
impl MediaControl {
    fn update_now_playing_macos(
        &self,
        title: &str,
        artist: &str,
        album: &str,
        duration_secs: f64,
        _cover_url: Option<&str>,
    ) {
        use objc::runtime::{Class, Object};
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            let cls = match Class::get("MPNowPlayingInfoCenter") {
                Some(c) => c,
                None => return,
            };
            let center: *mut Object = msg_send![cls, defaultCenter];
            if center.is_null() {
                return;
            }

            let dict_cls = match Class::get("NSMutableDictionary") {
                Some(c) => c,
                None => return,
            };
            let dict: *mut Object = msg_send![dict_cls, dictionary];
            if dict.is_null() {
                return;
            }

            let set_str = |dict: *mut Object, key: &str, val: &str| {
                let ns_key = Self::ns_string(key);
                let ns_val = Self::ns_string(val);
                let _: () = msg_send![dict, setObject: ns_val forKey: ns_key];
            };

            set_str(dict, "title", title);
            set_str(dict, "artist", artist);
            set_str(dict, "album", album);

            // 持续时间
            let num_cls = match Class::get("NSNumber") {
                Some(c) => c,
                None => return,
            };
            let duration_ms = (duration_secs * 1000.0) as i64;
            let dur_val: *mut Object =
                msg_send![num_cls, numberWithLongLong: duration_ms];
            let dur_key = Self::ns_string("elapsedPlaybackTime");
            let _: () = msg_send![dict, setObject: dur_val forKey: dur_key];

            // 播放速率
            let rate_val: *mut Object = msg_send![num_cls, numberWithDouble: 1.0];
            let rate_key = Self::ns_string("playbackRate");
            let _: () = msg_send![dict, setObject: rate_val forKey: rate_key];

            let _: () = msg_send![center, setNowPlayingInfo: dict];
        }
    }

    fn set_playback_state_macos(&self, playing: bool) {
        use objc::runtime::{Class, Object};
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            let cls = match Class::get("MPNowPlayingInfoCenter") {
                Some(c) => c,
                None => return,
            };
            let center: *mut Object = msg_send![cls, defaultCenter];
            if center.is_null() {
                return;
            }

            // MPNowPlayingPlaybackStatePlaying = 1, Paused = 2
            let state: u64 = if playing { 1 } else { 2 };
            let num_cls = match Class::get("NSNumber") {
                Some(c) => c,
                None => return,
            };
            let ns_state: *mut Object = msg_send![num_cls, numberWithUnsignedLongLong: state];
            let _: () = msg_send![center, setPlaybackState: ns_state];
        }
    }

    #[allow(dead_code)]
    fn setup_macos_command_handlers(&self, player: SharedPlayer) {
        use objc::runtime::{Class, Object};
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            let cls = match Class::get("MPRemoteCommandCenter") {
                Some(c) => c,
                None => return,
            };
            let center: *mut Object = msg_send![cls, sharedCommandCenter];
            if center.is_null() {
                return;
            }

            // 启用 play/pause 命令按钮
            // 注意：完整的 handler 注册需要 Objective-C block FFI (block crate)
            // 这里仅启用命令按钮
            for _cmd_sel in ["playCommand", "pauseCommand", "nextTrackCommand", "previousTrackCommand"] {
                let _sel = sel!(playCommand);
                let cmd: *mut Object = msg_send![center, playCommand];
                if !cmd.is_null() {
                    let _: () = msg_send![cmd, setEnabled: true];
                }
            }

            let _ = player; // 抑制未使用警告，后续接入 block FFI 时使用
        }
    }

    unsafe fn ns_string(s: &str) -> *mut Object {
        use objc::runtime::Class;
        use objc::{msg_send, sel, sel_impl};
        use std::ffi::CString;

        let c_str = CString::new(s).unwrap_or_default();
        let cls = Class::get("NSString").unwrap();
        msg_send![cls, stringWithUTF8String: c_str.as_ptr()]
    }
}

lazy_static::lazy_static! {
    pub static ref MEDIA_CONTROL: Mutex<MediaControl> = Mutex::new(MediaControl::new());
}
