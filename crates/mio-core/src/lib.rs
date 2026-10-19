//! Shared application rules. Desktop and mobile hosts own presentation and
//! platform integrations; business rules live here.
pub mod app_core;
pub mod audio_cache;
pub mod audio_effects;
pub mod background_config;
pub mod backup;
pub mod backup_restore;
pub mod cover;
pub mod cover_palette;
pub mod download;
pub mod effects;
pub mod equalizer;
pub mod host;
pub mod http;
pub mod i18n;
pub mod image_cache;
pub mod library;
pub mod list_plugins;
pub mod local_music;
pub mod lyric_style;
pub mod lyrics;
pub mod network_import;
pub mod online;
pub mod paths;
pub mod playback;
pub mod playlist_file;
pub mod plugin_catalog;
pub mod quality;
pub mod song;
pub mod song_sort;
pub mod song_theme;
pub mod storage;
pub mod storage_settings;
pub mod tags;

pub mod mobile_release;

pub mod recognition;

pub mod recognition_history;

pub mod dlna;

pub mod lan_transfer;

pub mod visuals;
