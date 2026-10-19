#[cfg(target_os = "macos")]
pub fn configure(window: &mut gpui_kit::Window) -> Result<(), String> {
    use cocoa::{
        appkit::{NSColor, NSWindow, NSWindowButton},
        base::{YES, id, nil},
    };
    use objc::{msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("Expected AppKit window".into());
    };
    // GPUI invokes this during window creation on the main thread. The borrowed
    // AppKit handle names its live NSView; no native pointer is retained.
    unsafe {
        let view = handle.ns_view.as_ptr() as id;
        let native_window: id = msg_send![view, window];
        if native_window == nil {
            return Err("GPUI view has no NSWindow".into());
        }
        for button in [
            NSWindowButton::NSWindowCloseButton,
            NSWindowButton::NSWindowMiniaturizeButton,
            NSWindowButton::NSWindowZoomButton,
        ] {
            let control = native_window.standardWindowButton_(button);
            if control != nil {
                let _: () = msg_send![control, setHidden: YES];
            }
        }
        native_window.setBackgroundColor_(NSColor::colorWithRed_green_blue_alpha_(
            nil,
            50. / 255.,
            158. / 255.,
            163.5 / 255.,
            0.45,
        ));
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
pub fn configure(_: &mut gpui_kit::Window) -> Result<(), String> {
    Ok(())
}

/// The native window of a GPUI window, on the main thread only.
#[cfg(target_os = "macos")]
fn native_window(window: &gpui_kit::Window) -> Result<cocoa::base::id, String> {
    use cocoa::base::{id, nil};
    use objc::{msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("Expected AppKit window".into());
    };
    // The borrowed AppKit handle names a live NSView for the duration of this call.
    unsafe {
        let view = handle.ns_view.as_ptr() as id;
        let native: id = msg_send![view, window];
        if native == nil {
            return Err("GPUI view has no NSWindow".into());
        }
        Ok(native)
    }
}

/// The Win32 window of a GPUI window.
#[cfg(target_os = "windows")]
fn native_window(
    window: &gpui_kit::Window,
) -> Result<windows_sys::Win32::Foundation::HWND, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("Expected Win32 window".into());
    };
    Ok(handle.hwnd.get() as _)
}

/// How many mouse clicks went to other applications since the start. A popup that has no
/// focus (the application is not frontmost) cannot see itself deactivated; it closes when this
/// number moves. A global monitor sees only events meant for other applications, so clicks in
/// this app's own windows and its tray icon do not count.
#[cfg(target_os = "macos")]
pub fn outside_clicks() -> u64 {
    use std::sync::{
        Once,
        atomic::{AtomicU64, Ordering},
    };
    static CLICKS: AtomicU64 = AtomicU64::new(0);
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        use block::ConcreteBlock;
        use objc::{class, msg_send, runtime::Object, sel, sel_impl};
        // NSEventMaskLeftMouseDown | RightMouseDown | OtherMouseDown
        let mask: u64 = (1 << 1) | (1 << 3) | (1 << 25);
        let block = ConcreteBlock::new(|_event: *mut Object| {
            CLICKS.fetch_add(1, Ordering::Relaxed);
        })
        .copy();
        // SAFETY: the handler only bumps a counter; the monitor lives for the whole process.
        unsafe {
            let _: *mut Object = msg_send![
                class!(NSEvent),
                addGlobalMonitorForEventsMatchingMask: mask
                handler: &*block
            ];
        }
        std::mem::forget(block);
    });
    CLICKS.load(Ordering::Relaxed)
}
#[cfg(not(target_os = "macos"))]
pub fn outside_clicks() -> u64 {
    0
}

/// The raw `HWND` the system media controls attach to; only Windows needs one.
#[cfg(target_os = "windows")]
pub fn window_handle(window: &gpui_kit::Window) -> Option<*mut std::ffi::c_void> {
    native_window(window)
        .ok()
        .map(|hwnd| hwnd as *mut std::ffi::c_void)
}
#[cfg(not(target_os = "windows"))]
pub fn window_handle(_: &gpui_kit::Window) -> Option<*mut std::ffi::c_void> {
    None
}

/// Makes the desktop lyric window an overlay like the original's: no titlebar controls or
/// shadow, a clear background, and it stays visible when the app is not frontmost (an `NSPanel`
/// hides on deactivate by default).
#[cfg(target_os = "macos")]
pub fn configure_overlay(window: &mut gpui_kit::Window) -> Result<(), String> {
    use cocoa::{
        appkit::{NSColor, NSWindow, NSWindowButton},
        base::{NO, YES, nil},
    };
    use objc::{msg_send, sel, sel_impl};
    let native = native_window(window)?;
    unsafe {
        for button in [
            NSWindowButton::NSWindowCloseButton,
            NSWindowButton::NSWindowMiniaturizeButton,
            NSWindowButton::NSWindowZoomButton,
        ] {
            let control = native.standardWindowButton_(button);
            if control != nil {
                let _: () = msg_send![control, setHidden: YES];
            }
        }
        // NSWindowTitleHidden
        let _: () = msg_send![native, setTitleVisibility: 1isize];
        native.setBackgroundColor_(NSColor::clearColor(nil));
        native.setHasShadow_(NO);
        let _: () = msg_send![native, setHidesOnDeactivate: NO];
    }
    Ok(())
}
/// The window of the desktop lyric stays above the others, without a taskbar button or an
/// Alt+Tab entry, like the original's `skip_taskbar` + `always_on_top` window.
#[cfg(target_os = "windows")]
pub fn configure_overlay(window: &mut gpui_kit::Window) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };
    let hwnd = native_window(window)?;
    // SAFETY: `hwnd` is the live window of this GPUI window, used on its own thread.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let style =
            (style | (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) as isize) & !(WS_EX_APPWINDOW as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    Ok(())
}
// Linux opens the window as a floating one already; nothing else is native to set.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn configure_overlay(_: &mut gpui_kit::Window) -> Result<(), String> {
    Ok(())
}

/// `set_ignore_cursor_events`: while on, the window lets every pointer event through to what is
/// behind it (the desktop lyric "lock").
#[cfg(target_os = "macos")]
pub fn set_click_through(window: &gpui_kit::Window, on: bool) -> Result<(), String> {
    use cocoa::{appkit::NSWindow, base::BOOL};
    let native = native_window(window)?;
    unsafe {
        native.setIgnoresMouseEvents_(on as BOOL);
    }
    Ok(())
}
#[cfg(target_os = "windows")]
pub fn set_click_through(window: &gpui_kit::Window, on: bool) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_LAYERED, WS_EX_TRANSPARENT,
    };
    let hwnd = native_window(window)?;
    let flags = (WS_EX_TRANSPARENT | WS_EX_LAYERED) as isize;
    // SAFETY: `hwnd` is the live window of this GPUI window, used on its own thread.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let style = if on { style | flags } else { style & !flags };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    }
    Ok(())
}
/// An empty X11 input shape: the window takes no pointer events. Wayland has no way for a
/// client to do this through its window handle here, so the lock reports an error there.
#[cfg(target_os = "linux")]
pub fn set_click_through(window: &gpui_kit::Window, on: bool) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::{
        connection::Connection,
        protocol::{
            shape::{ConnectionExt as _, SK, SO},
            xproto::ClipOrdering,
        },
    };
    let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
    let id = match handle.as_raw() {
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        RawWindowHandle::Xlib(handle) => handle.window as u32,
        _ => return Err("Click-through needs an X11 window".into()),
    };
    let (connection, _) = x11rb::connect(None).map_err(|e| e.to_string())?;
    if on {
        connection
            .shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, id, 0, 0, &[])
            .map_err(|e| e.to_string())?;
    } else {
        // No source bitmap: the input shape goes back to the window's own bounds.
        connection
            .shape_mask(SO::SET, SK::INPUT, id, 0, 0, x11rb::NONE)
            .map_err(|e| e.to_string())?;
    }
    connection.flush().map_err(|e| e.to_string())
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn set_click_through(_: &gpui_kit::Window, _: bool) -> Result<(), String> {
    Err("Click-through windows are not implemented on this platform yet".into())
}

/// Takes the window off the screen without closing it (`NSWindow.orderOut:`), the way the
/// original's tray mode hides the main window. Without the tray there would be no way back, so
/// callers only use it while a tray icon exists.
#[cfg(target_os = "macos")]
pub fn hide_window(window: &gpui_kit::Window) -> Result<(), String> {
    use cocoa::base::nil;
    use objc::{msg_send, sel, sel_impl};
    let native = native_window(window)?;
    unsafe {
        let _: () = msg_send![native, orderOut: nil];
    }
    Ok(())
}
#[cfg(target_os = "windows")]
pub fn hide_window(window: &gpui_kit::Window) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
    let hwnd = native_window(window)?;
    // SAFETY: `hwnd` is the live window of this GPUI window.
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
    }
    Ok(())
}
/// GPUI cannot unmap a window on Linux (Wayland has no such request), so it is minimized; with
/// the tray's StatusNotifierItem the window is still reachable and off the screen.
#[cfg(target_os = "linux")]
pub fn hide_window(window: &gpui_kit::Window) -> Result<(), String> {
    window.minimize_window();
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn hide_window(_: &gpui_kit::Window) -> Result<(), String> {
    Err("Hiding the window is not implemented on this platform yet".into())
}

/// Shows the window again and gives it the focus: brings the app forward, restores a minimized
/// window and puts it in front (`window.show(); window.set_focus()` of the original's tray click).
#[cfg(target_os = "macos")]
pub fn show_window(window: &gpui_kit::Window) -> Result<(), String> {
    use cocoa::{
        appkit::{NSApp, NSApplication},
        base::{BOOL, YES, id, nil},
    };
    use objc::{msg_send, sel, sel_impl};
    let native = native_window(window)?;
    unsafe {
        let miniaturized: BOOL = msg_send![native, isMiniaturized];
        if miniaturized == YES {
            let _: () = msg_send![native, deminiaturize: nil];
        }
        let app: id = NSApp();
        app.activateIgnoringOtherApps_(YES);
        let _: () = msg_send![native, makeKeyAndOrderFront: nil];
    }
    Ok(())
}
#[cfg(target_os = "windows")]
pub fn show_window(window: &gpui_kit::Window) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow,
    };
    let hwnd = native_window(window)?;
    // SAFETY: `hwnd` is the live window of this GPUI window.
    unsafe {
        let command = if IsIconic(hwnd) != 0 {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        ShowWindow(hwnd, command);
        SetForegroundWindow(hwnd);
    }
    window.activate_window();
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn show_window(window: &gpui_kit::Window) -> Result<(), String> {
    window.activate_window();
    Ok(())
}

/// The system's preferred language is not Chinese: what the original's `system` language does
/// (`zh*` is `zh-CN`, anything else `en-US`; Chinese when it cannot tell).
pub fn system_is_english() -> bool {
    match system_language() {
        Some(language) => !language.to_ascii_lowercase().starts_with("zh"),
        None => false,
    }
}

#[cfg(target_os = "macos")]
fn system_language() -> Option<String> {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CStr;
    // SAFETY: plain message sends to Foundation classes; the returned strings are autoreleased
    // objects read immediately.
    unsafe {
        let languages: id = msg_send![class!(NSLocale), preferredLanguages];
        if languages == nil {
            return None;
        }
        let first: id = msg_send![languages, firstObject];
        if first == nil {
            return None;
        }
        let utf8: *const std::os::raw::c_char = msg_send![first, UTF8String];
        (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }
}
#[cfg(not(target_os = "macos"))]
fn system_language() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
}
