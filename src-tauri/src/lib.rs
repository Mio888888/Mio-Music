mod db;
mod commands;
mod music_sdk;
mod local_music;
mod download;
mod plugin;

use db::AppDb;
use download::manager::DownloadManager;
use plugin::manager::PluginManager;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_data_dir = db::get_app_data_dir();
    let app_db = AppDb::new(&app_data_dir).expect("Failed to initialize databases");
    let plugin_manager = PluginManager::new(&app_data_dir);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_db)
        .manage(plugin_manager)
        .setup(|app| {
            let app_handle = app.handle().clone();
            let app_data_dir = db::get_app_data_dir();
            let download_manager = DownloadManager::new(&app_data_dir, app_handle);
            app.manage(download_manager);
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
            // Music SDK
            music_sdk::commands::service_music_sdk_request,
            music_sdk::commands::service_music_tip_search,
            // Local Music
            local_music::commands::local_music__scan,
            local_music::commands::local_music__get_cover,
            local_music::commands::local_music__get_covers,
            local_music::commands::local_music__write_tags,
            local_music::commands::local_music__get_tags,
            local_music::commands::local_music__get_lyric,
            local_music::commands::local_music__clear_index,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
