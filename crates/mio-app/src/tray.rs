//! The system tray icon: a tooltip "Mio Music" and a left click that opens the mini player
//! panel (`shell/tray_panel.rs`). There is no menu: the panel has the desktop lyric switch
//! (which also unlocks a locked lyric window) and the quit button.
//!
//! The icon and its menu live in the process' main thread; their events arrive on channels the
//! shell drains from a polling task.

/// What the user asked for through the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Left click on the icon, with the icon's `(x, y, width, height)` in physical pixels: the
    /// mini player opens under it.
    Panel(i32, i32, u32, u32),
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod imp {
    use super::Event;
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };

    pub struct Tray {
        // Dropping the icon removes it from the menu bar.
        _icon: TrayIcon,
    }

    fn icon() -> Result<Icon, String> {
        let data = crate::assets::bytes("images/tray.png").ok_or("missing tray icon")?;
        let decoded = image::load_from_memory(&data).map_err(|e| e.to_string())?;
        let rgba = decoded.into_rgba8();
        let (width, height) = rgba.dimensions();
        Icon::from_rgba(rgba.into_raw(), width, height).map_err(|e| e.to_string())
    }

    /// Creates the icon: no menu, so no click opens anything but the panel.
    pub fn create() -> Result<Tray, String> {
        let icon = TrayIconBuilder::new()
            .with_icon(icon()?)
            .with_tooltip("Mio Music")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Tray { _icon: icon })
    }

    /// Everything that happened since the last call.
    pub fn poll() -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                events.push(Event::Panel(
                    rect.position.x.round() as i32,
                    rect.position.y.round() as i32,
                    rect.size.width,
                    rect.size.height,
                ));
            }
        }
        events
    }
}

/// A StatusNotifierItem on the session D-Bus (KDE, and GNOME with the AppIndicator extension).
/// The service runs on its own thread; its callbacks only queue an [`Event`].
#[cfg(target_os = "linux")]
mod imp {
    use super::Event;
    use ksni::{
        Icon, Tray as Item,
        blocking::{Handle, TrayMethods},
    };
    use std::sync::Mutex;

    static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());

    fn push(event: Event) {
        if let Ok(mut events) = EVENTS.lock() {
            events.push(event);
        }
    }

    struct Entry {
        icon: Vec<Icon>,
    }

    impl Item for Entry {
        fn id(&self) -> String {
            "com.vant.Mio.Music.GPUI".into()
        }
        fn title(&self) -> String {
            "Mio Music".into()
        }
        fn icon_pixmap(&self) -> Vec<Icon> {
            self.icon.clone()
        }
        // Left click: show and focus the main window.
        fn activate(&mut self, x: i32, y: i32) {
            push(Event::Panel(x, y, 0, 0));
        }
    }

    pub struct Tray {
        // Dropping the handle ends the service and removes the icon.
        _handle: Handle<Entry>,
    }

    fn icon() -> Result<Icon, String> {
        let data = crate::assets::bytes("images/tray.png").ok_or("missing tray icon")?;
        let decoded = image::load_from_memory(&data).map_err(|e| e.to_string())?;
        let rgba = decoded.into_rgba8();
        let (width, height) = rgba.dimensions();
        // The pixmap is ARGB32 in network byte order.
        let data = rgba
            .pixels()
            .flat_map(|p| [p.0[3], p.0[0], p.0[1], p.0[2]])
            .collect();
        Ok(Icon {
            width: width as i32,
            height: height as i32,
            data,
        })
    }

    pub fn create() -> Result<Tray, String> {
        let entry = Entry {
            icon: vec![icon()?],
        };
        let handle = entry
            .spawn()
            .map_err(|error| format!("no StatusNotifierItem host: {error}"))?;
        Ok(Tray { _handle: handle })
    }

    pub fn poll() -> Vec<Event> {
        EVENTS
            .lock()
            .map(|mut events| std::mem::take(&mut *events))
            .unwrap_or_default()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod imp {
    use super::Event;

    pub struct Tray;
    pub fn create() -> Result<Tray, String> {
        Err("The tray icon is not implemented on this platform yet".into())
    }
    pub fn poll() -> Vec<Event> {
        Vec::new()
    }
}

pub use imp::{Tray, create, poll};
