//! The global hotkeys of the original (`hotkey_commands.rs`, `types/hotkeys.ts`,
//! `utils/hotkeys/recording.ts`): eleven actions with an accelerator each, registered with the
//! operating system while the app runs, and the accelerator text the recorder produces.
//!
//! The configuration keeps the original's shape (`{enabled, bindings}` with accelerators like
//! `CommandOrControl+Alt+P`) so the settings page and the saved file read like the original's.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The actions, in the order of the original's settings list.
pub const ACTIONS: [Action; 11] = [
    Action::Toggle,
    Action::PlayPrev,
    Action::PlayNext,
    Action::SeekBackward,
    Action::SeekForward,
    Action::ToggleDesktopLyric,
    Action::VolumeDown,
    Action::VolumeUp,
    Action::SetPlayModeSequence,
    Action::SetPlayModeRandom,
    Action::TogglePlayModeSingle,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Toggle,
    PlayPrev,
    PlayNext,
    SeekBackward,
    SeekForward,
    ToggleDesktopLyric,
    VolumeDown,
    VolumeUp,
    SetPlayModeSequence,
    SetPlayModeRandom,
    TogglePlayModeSingle,
}

impl Action {
    /// The key of `bindings` and of the locale entries (`action<Id>`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::PlayPrev => "playPrev",
            Self::PlayNext => "playNext",
            Self::SeekBackward => "seekBackward",
            Self::SeekForward => "seekForward",
            Self::ToggleDesktopLyric => "toggleDesktopLyric",
            Self::VolumeDown => "volumeDown",
            Self::VolumeUp => "volumeUp",
            Self::SetPlayModeSequence => "setPlayModeSequence",
            Self::SetPlayModeRandom => "setPlayModeRandom",
            Self::TogglePlayModeSingle => "togglePlayModeSingle",
        }
    }
    /// The locale key stem: `settings.hotkey.action<Stem>` and `...Desc`.
    pub fn locale_stem(self) -> &'static str {
        match self {
            Self::Toggle => "actionToggle",
            Self::PlayPrev => "actionPlayPrev",
            Self::PlayNext => "actionPlayNext",
            Self::SeekBackward => "actionSeekBackward",
            Self::SeekForward => "actionSeekForward",
            Self::ToggleDesktopLyric => "actionToggleDesktopLyric",
            Self::VolumeDown => "actionVolumeDown",
            Self::VolumeUp => "actionVolumeUp",
            Self::SetPlayModeSequence => "actionSetPlayModeSequence",
            Self::SetPlayModeRandom => "actionSetPlayModeRandom",
            Self::TogglePlayModeSingle => "actionTogglePlayModeSingle",
        }
    }
    fn default_binding(self) -> &'static str {
        match self {
            Self::Toggle => "CommandOrControl+Alt+P",
            Self::PlayPrev => "CommandOrControl+Alt+Left",
            Self::PlayNext => "CommandOrControl+Alt+Right",
            Self::SeekBackward => "CommandOrControl+Alt+J",
            Self::SeekForward => "CommandOrControl+Alt+L",
            Self::ToggleDesktopLyric => "CommandOrControl+Alt+D",
            Self::VolumeDown => "CommandOrControl+Alt+Down",
            Self::VolumeUp => "CommandOrControl+Alt+Up",
            Self::SetPlayModeSequence => "CommandOrControl+Alt+4",
            Self::SetPlayModeRandom => "CommandOrControl+Alt+5",
            Self::TogglePlayModeSingle => "CommandOrControl+Alt+6",
        }
    }
}

/// `HotkeyConfig`: `enabled` is the master switch, `bindings` maps an action id to its
/// accelerator (empty = not set).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub enabled: bool,
    pub bindings: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            bindings: ACTIONS
                .iter()
                .map(|a| (a.id().to_owned(), a.default_binding().to_owned()))
                .collect(),
        }
    }
}

impl Config {
    pub fn binding(&self, action: Action) -> &str {
        self.bindings.get(action.id()).map_or("", String::as_str)
    }
    pub fn set_binding(&mut self, action: Action, accelerator: &str) {
        self.bindings
            .insert(action.id().to_owned(), accelerator.to_owned());
    }
    /// The other action that already uses `accelerator` (compared without case), if any.
    pub fn conflict(&self, action: Action, accelerator: &str) -> Option<Action> {
        if accelerator.is_empty() {
            return None;
        }
        ACTIONS.iter().copied().find(|other| {
            *other != action && self.binding(*other).eq_ignore_ascii_case(accelerator)
        })
    }
}

/// `acceleratorToDisplay` without the "not set" text (the caller supplies it).
pub fn display(accelerator: &str) -> String {
    accelerator.trim().replace("CommandOrControl", "Ctrl")
}

/// `isCompleteAccelerator`: something other than a lone modifier at the end.
pub fn is_complete(accelerator: &str) -> bool {
    accelerator
        .split('+')
        .rfind(|s| !s.is_empty())
        .is_some_and(|last| !matches!(last, "Alt" | "Shift" | "CommandOrControl"))
}

/// `normalizeKeyPart`: the accelerator text for the pressed modifiers and key. `key` is
/// GPUI's key name (`"a"`, `"left"`, `"space"`…); `None` while only modifiers are held.
pub fn normalize(control_or_command: bool, alt: bool, shift: bool, key: Option<&str>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if control_or_command {
        parts.push("CommandOrControl".into());
    }
    if alt {
        parts.push("Alt".into());
    }
    if shift {
        parts.push("Shift".into());
    }
    if let Some(key) = key {
        let mut chars = key.chars();
        let named = match key {
            " " | "space" => Some("Space"),
            "up" => Some("Up"),
            "down" => Some("Down"),
            "left" => Some("Left"),
            "right" => Some("Right"),
            "escape" => Some("Esc"),
            "enter" => Some("Enter"),
            "tab" => Some("Tab"),
            "backspace" => Some("Backspace"),
            "delete" => Some("Delete"),
            "home" => Some("Home"),
            "end" => Some("End"),
            "pageup" => Some("PageUp"),
            "pagedown" => Some("PageDown"),
            "insert" => Some("Insert"),
            _ => None,
        };
        if let Some(named) = named {
            parts.push(named.into());
        } else if chars.clone().count() == 1 {
            parts.push(key.to_uppercase());
        } else if let Some(first) = chars.next() {
            // `f5` → `F5`, like the DOM's `key`.
            parts.push(first.to_uppercase().chain(chars).collect());
        }
    }
    parts.join("+")
}

/// What registering the shortcuts with the system came to.
#[derive(Debug, Default, Clone)]
pub struct Status {
    /// Actions whose accelerator could not be registered (`failedActions`).
    pub failed: Vec<Action>,
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod imp {
    use super::{ACTIONS, Action, Config, Status};
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
    use std::collections::HashMap;

    /// The system registrations; must be created and used on the main thread.
    pub struct Registry {
        manager: GlobalHotKeyManager,
        registered: Vec<HotKey>,
        actions: HashMap<u32, Action>,
    }

    impl Registry {
        pub fn new() -> Result<Self, String> {
            Ok(Self {
                manager: GlobalHotKeyManager::new().map_err(|e| e.to_string())?,
                registered: Vec::new(),
                actions: HashMap::new(),
            })
        }

        /// `unregister_all` and registering the configured accelerators again
        /// (`re_register_shortcuts`); nothing is registered while the master switch is off.
        pub fn apply(&mut self, config: &Config) -> Status {
            self.clear();
            let mut status = Status::default();
            if !config.enabled {
                return status;
            }
            for action in ACTIONS {
                let accelerator = config.binding(action);
                if accelerator.is_empty() {
                    continue;
                }
                let Ok(hotkey) = accelerator.parse::<HotKey>() else {
                    status.failed.push(action);
                    continue;
                };
                // The same combination twice: the system takes it once, the second action fails.
                if self.actions.contains_key(&hotkey.id()) {
                    status.failed.push(action);
                    continue;
                }
                match self.manager.register(hotkey) {
                    Ok(()) => {
                        self.actions.insert(hotkey.id(), action);
                        self.registered.push(hotkey);
                    }
                    Err(error) => {
                        eprintln!("Unable to register the {} hotkey: {error}", action.id());
                        status.failed.push(action);
                    }
                }
            }
            status
        }

        pub fn clear(&mut self) {
            if !self.registered.is_empty() {
                let _ = self.manager.unregister_all(&self.registered);
            }
            self.registered.clear();
            self.actions.clear();
        }

        /// Actions whose key was pressed since the last call.
        pub fn poll(&self) -> Vec<Action> {
            let mut triggered = Vec::new();
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.state == HotKeyState::Pressed
                    && let Some(action) = self.actions.get(&event.id)
                {
                    triggered.push(*action);
                }
            }
            triggered
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod imp {
    use super::{Action, Config, Status};

    pub struct Registry;
    impl Registry {
        pub fn new() -> Result<Self, String> {
            Err("Global hotkeys are not implemented on this platform".into())
        }
        pub fn apply(&mut self, _config: &Config) -> Status {
            Status::default()
        }
        pub fn clear(&mut self) {}
        pub fn poll(&self) -> Vec<Action> {
            Vec::new()
        }
    }
}

pub use imp::Registry;
