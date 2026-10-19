#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod audio_device;
mod backup;
mod directories;
mod dlna;
mod hotkeys;
mod image_http;
mod legacy_import;
mod legacy_leveldb;
mod media_keys;
mod navigation;
#[cfg(feature = "parity")]
mod parity;
mod platform;
mod playlist_file;
mod recognition;
mod shell;
mod skin;
mod tray;
mod updater;

use mio_core::{
    audio_cache, background_config, download, effects, equalizer, i18n, image_cache, library,
    list_plugins, local_music, lyrics, online, playback, quality, song, storage, tags,
};

use gpui_kit::component::{Root, Theme, ThemeMode};
use gpui_kit::*;
use shell::Mio;

/// The main window, for the Dock icon's "reopen" while it is hidden in the tray.
struct MainWindow(AnyWindowHandle);
impl Global for MainWindow {}

fn main() {
    mio_core::host::set_system_language_resolver(platform::system_is_english);
    // The original's WebView (WebKit) sets every glyph about 0.28px wider than this text system
    // does, at any size; without it lines come out 2–4% narrower and wrap differently. Measured
    // against the reference captures (see tools/golden); other platforms have not been measured.
    #[cfg(target_os = "macos")]
    gpui_kit::set_text_advance_adjustment(0.28, 0.28);
    let app = application().with_assets(assets::Assets);
    // Clicking the Dock icon brings a window hidden in the tray back (macOS habit; the original
    // has no handler for it).
    app.on_reopen(|cx| {
        if let Some(MainWindow(handle)) = cx.try_global::<MainWindow>().map(|w| MainWindow(w.0)) {
            let _ = cx.update_window(handle, |_, window, _| {
                let _ = platform::show_window(window);
            });
        }
    });
    app.run(|cx| {
        cx.set_http_client(std::sync::Arc::new(image_http::ImageHttpClient));
        init(cx);
        cx.text_system()
            .add_fonts(assets::fonts())
            .expect("Unable to load original fonts");
        Theme::change(ThemeMode::Light, None, cx);
        Theme::update(cx, |theme| {
            theme.font_family = if cfg!(target_os = "macos") {
                "PingFang SC"
            } else if cfg!(target_os = "windows") {
                "Microsoft YaHei"
            } else {
                "Ubuntu"
            }
            .into();
        });
        #[cfg(feature = "parity")]
        let capture_size = std::env::var("MIO_GPUI_CAPTURE_WIDTH")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(1200.);
        #[cfg(not(feature = "parity"))]
        let capture_size = 1200.;
        #[cfg(feature = "parity")]
        let capture_height = std::env::var("MIO_GPUI_CAPTURE_HEIGHT")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite() && *v >= 320. && *v <= 2160.)
            .unwrap_or(800.);
        #[cfg(not(feature = "parity"))]
        let capture_height = 800.;
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(capture_size), px(capture_height)),
                cx,
            ))),
            titlebar: if cfg!(target_os = "macos") {
                Some(TitlebarOptions {
                    title: Some("音乐".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                })
            } else {
                None
            },
            app_owns_titlebar_drag: true,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: Some("com.vant.Mio.Music.GPUI".into()),
            ..Default::default()
        };
        // Component Root supplies dialogs, notifications, and the shared Base root.
        let main = cx
            .open_window(options, |window, cx| {
                window.set_window_title("音乐");
                platform::configure(window).expect("Unable to configure native window");
                let view = cx.new(|cx| {
                    #[cfg(feature = "parity")]
                    if std::env::var_os("MIO_GPUI_CAPTURE_DIR").is_some()
                        || std::env::var_os("MIO_GPUI_FIXTURE_PORT").is_some()
                    {
                        return Mio::capture(window, cx);
                    }
                    Mio::new(window, cx)
                });
                #[cfg(feature = "parity")]
                parity::schedule(window, cx);
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("Unable to open Mio Music");
        // The desktop lyric window is a second window; closing the main one ends the app.
        cx.set_global(MainWindow(main.into()));
        let main_window = main.window_id();
        cx.on_window_closed(move |cx, closed| {
            if closed == main_window || cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
