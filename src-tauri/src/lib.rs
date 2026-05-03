mod audio_capture;
mod audio_device;
mod db;
mod commands;
mod music_sdk;
mod local_music;
mod download;
mod plugin;
mod player;

use db::AppDb;
use download::manager::DownloadManager;
use plugin::manager::PluginManager;
#[allow(unused_imports)]
use player::SharedPlayer;
use tauri::{Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder};
use commands::hotkey_commands;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_data_dir = db::get_app_data_dir();
    let app_db = AppDb::new(&app_data_dir).expect("Failed to initialize databases");
    let plugin_manager = PluginManager::new(&app_data_dir);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(app_db)
        .manage(plugin_manager)
        .manage(Mutex::new(commands::power_save::power_save_blocker_state()))
        .manage(commands::DesktopLyricState {
            is_open: Mutex::new(false),
            is_locked: Mutex::new(false),
        })
        .setup(|app| {
            let win_builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("音乐")
                .inner_size(1200.0, 800.0)
                .center();

            #[cfg(target_os = "macos")]
            let win_builder = win_builder.title_bar_style(TitleBarStyle::Overlay);

            let window = win_builder.build().expect("Failed to create main window");

            #[cfg(target_os = "macos")]
            {
                use cocoa::appkit::{NSColor, NSWindow, NSWindowButton, NSWindowTitleVisibility};
                use cocoa::base::{id, nil, YES};
                use objc::{msg_send, sel, sel_impl};
                use objc::runtime::Object;

                unsafe fn hide_button(button: id) {
                    let _: () = msg_send![button as *mut Object, setHidden: YES];
                }

                let ns_window = window.ns_window().expect("Failed to get NSWindow") as id;
                unsafe {
                    let bg_color = NSColor::colorWithRed_green_blue_alpha_(
                        nil,
                        50.0 / 255.0,
                        158.0 / 255.0,
                        163.5 / 255.0,
                        0.45,
                    );
                    ns_window.setBackgroundColor_(bg_color);
                    ns_window.setTitleVisibility_(NSWindowTitleVisibility::NSWindowTitleHidden);
                    ns_window.setTitlebarAppearsTransparent_(YES);

                    for button in [
                        NSWindowButton::NSWindowCloseButton,
                        NSWindowButton::NSWindowMiniaturizeButton,
                        NSWindowButton::NSWindowZoomButton,
                    ] {
                        let btn = ns_window.standardWindowButton_(button);
                        if btn != nil {
                            hide_button(btn);
                        }
                    }
                }
            }

            let app_handle = app.handle().clone();
            let app_data_dir = db::get_app_data_dir();
            let download_manager = DownloadManager::new(&app_data_dir, app_handle.clone());
            app.manage(download_manager);

            // Start audio device change listener
            audio_device::start_device_listener(app_handle.clone());

            // Initialize native player engine (GStreamer)
            let shared_player = player::init_player(app_handle.clone());
            app.manage(shared_player);

            // Store AppHandle for hotkey re-registration
            hotkey_commands::set_app_handle(app_handle.clone());

            // Register OS-level shortcuts from saved config
            {
                let db_ref = app_handle.state::<AppDb>();
                let conn = db_ref.playlist.lock().expect("DB lock poisoned");
                let config = hotkey_commands::load_config_from_db(&conn);
                drop(conn);
                hotkey_commands::re_register_shortcuts(&app_handle, &config);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Track commands
            commands::music_commands::track__get_all,
            commands::music_commands::track__get_by_id,
            commands::music_commands::track__get_by_path,
            commands::music_commands::track__upsert,
            commands::music_commands::track__upsert_batch,
            commands::music_commands::track__delete_by_path,
            commands::music_commands::track__delete_by_songmid,
            commands::music_commands::track__clear,
            commands::music_commands::track__get_stat,
            commands::music_commands::track__get_all_stats,
            commands::music_commands::track__prune_outside,
            // Dir commands
            commands::music_commands::dir__get_all,
            commands::music_commands::dir__set,
            // Songlist (playlist) commands
            commands::playlist_commands::songlist__get_all,
            commands::playlist_commands::songlist__get,
            commands::playlist_commands::songlist__create,
            commands::playlist_commands::songlist__delete,
            commands::playlist_commands::songlist__update,
            commands::playlist_commands::songlist__update_cover,
            commands::playlist_commands::songlist__search,
            commands::playlist_commands::songlist__exists,
            // Songlist song commands
            commands::playlist_commands::songlist__list_songs,
            commands::playlist_commands::songlist__count_songs,
            commands::playlist_commands::songlist__has_song,
            commands::playlist_commands::songlist__add_songs,
            commands::playlist_commands::songlist__add_head,
            commands::playlist_commands::songlist__remove_song,
            commands::playlist_commands::songlist__remove_batch,
            commands::playlist_commands::songlist__clear_songs,
            commands::playlist_commands::songlist__search_songs,
            // Batch delete & reorder
            commands::playlist_commands::songlist__batch_delete,
            commands::playlist_commands::songlist__move_song,
            // Favorites
            commands::playlist_commands::songlist__get_favorites_id,
            commands::playlist_commands::songlist__set_favorites_id,
            // Config (KV store)
            commands::config_commands::config__get,
            commands::config_commands::config__set,
            commands::config_commands::config__get_all,
            commands::config_commands::config__delete,
            // Hotkeys
            hotkey_commands::hotkeys__get,
            hotkey_commands::hotkeys__set,
            // Music SDK
            music_sdk::commands::service_music_sdk_request,
            music_sdk::commands::service_music_tip_search,
            music_sdk::commands::service_music_search_music,
            music_sdk::commands::service_music_find_music,
            // Local Music
            local_music::commands::local_music__scan,
            local_music::commands::local_music__get_cover,
            local_music::commands::local_music__get_covers,
            local_music::commands::local_music__write_tags,
            local_music::commands::local_music__get_tags,
            local_music::commands::local_music__get_lyric,
            local_music::commands::local_music__clear_index,
            local_music::commands::local_music__select_dirs,
            // Download Manager
            download::commands::download__add_task,
            download::commands::download__get_tasks,
            download::commands::download__pause_task,
            download::commands::download__resume_task,
            download::commands::download__cancel_task,
            download::commands::download__delete_task,
            download::commands::download__retry_task,
            download::commands::download__pause_all_tasks,
            download::commands::download__resume_all_tasks,
            download::commands::download__set_max_concurrent,
            download::commands::download__get_max_concurrent,
            download::commands::download__clear_tasks,
            download::commands::download__validate_files,
            download::commands::download__open_file_location,
            // Plugin System
            plugin::commands::plugin__initialize,
            plugin::commands::plugin__get_list,
            plugin::commands::plugin__add,
            plugin::commands::plugin__uninstall,
            plugin::commands::plugin__get_info,
            plugin::commands::plugin__call_method,
            plugin::commands::plugin__download_and_add,
            plugin::commands::plugin__get_type,
            plugin::commands::plugin__get_log,
            plugin::commands::plugin__get_config_schema,
            plugin::commands::plugin__get_config,
            plugin::commands::plugin__save_config,
            plugin::commands::plugin__test_connection,
            plugin::commands::plugin__select_and_add,
            plugin::commands::plugin__get_code,
            // Directory Settings
            commands::directory_commands::get_directories,
            commands::directory_commands::save_directories,
            commands::directory_commands::reset_directories,
            commands::directory_commands::get_directory_size,
            commands::directory_commands::open_directory,
            commands::directory_commands::get_cache_info,
            commands::directory_commands::clear_cache,
            // Audio Device
            audio_device::audio__enumerate_devices,
            // Audio Capture (microphone)
            audio_capture::audio_capture_start,
            audio_capture::audio_capture_stop,
            audio_device::audio__set_output_device,
            audio_device::audio__get_device_volume,
            audio_device::audio__set_device_volume,
            // HTTP Proxy (bypass CORS for plugins)
            commands::http_proxy,
            // Audio Proxy (fetch remote audio → data URI, bypasses CORS for <audio>)
            commands::audio_proxy,
            // Performance telemetry
            commands::performance__memory,
            // Desktop lyric window
            commands::change_desktop_lyric,
            commands::toogle_desktop_lyric_lock,
            commands::get_lyric_open_state,
            commands::get_lyric_lock_state,
            commands::get_font_list,
            commands::get_desktop_lyric_option,
            commands::set_desktop_lyric_option,
            // Power Save Blocker
            commands::power_save::power_save_blocker__start,
            commands::power_save::power_save_blocker__stop,
            // Native Player (rodio + cpal)
            player::commands::player__play,
            player::commands::player__pause,
            player::commands::player__resume,
            player::commands::player__stop,
            player::commands::player__seek,
            player::commands::player__set_volume,
            player::commands::player__get_state,
            player::commands::player__crossfade,
            player::commands::player__swap_slot,
            player::commands::player__set_eq_band,
            player::commands::player__get_eq_bands,
            player::commands::player__set_bass_boost,
            player::commands::player__set_balance,
            player::commands::player__update_now_playing,
            player::commands::player__shutdown,
            player::commands::player__preload,
            player::commands::player__gapless_swap,
            player::commands::player__clear_secondary,
            player::commands::player__set_seamless_config,
            player::commands::player__set_cache_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
