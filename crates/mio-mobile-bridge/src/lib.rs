#[cfg(target_os = "android")]
mod android;
pub mod api;
mod catalog;
mod downloads;
mod equalizer;
mod favorites;
mod frb_generated; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */
mod imports;
mod media;
mod media_artwork;
mod media_lyrics;
#[cfg(target_env = "ohos")]
mod ohos;
mod player_details;
mod playlist_search;
mod plugins;
mod queue;
mod runtime;
mod search;
mod settings;
mod spectrum;

mod discovery;
mod source_selection;

mod comments;
mod online_save;
mod singer;

mod playlist_import;

mod queue_document;

mod playlist_cover;

mod local_tags;

mod collection_download;

mod backup_restore;

mod backup_upload;

mod releases;

mod recognition;

mod microphone;

mod dlna;

mod dlna_cast;

mod dlna_handoff;

mod lan_transfer;
