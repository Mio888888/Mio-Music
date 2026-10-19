use serde::{Deserialize, Serialize};
use std::collections::HashMap;
// --- Shared types matching CeruMusic's MusicItem ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityInfo {
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicItem {
    #[serde(default)]
    pub songmid: serde_json::Value,
    #[serde(default)]
    pub singer: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub album_name: String,
    #[serde(default)]
    pub album_id: serde_json::Value,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub interval: String,
    #[serde(default)]
    pub img: String,
    #[serde(default)]
    pub lrc: Option<String>,
    #[serde(default)]
    pub types: Option<Vec<String>>,
    #[serde(default, rename = "_types")]
    pub types_map: Option<HashMap<String, QualityInfo>>,
    #[serde(default)]
    pub type_url: Option<serde_json::Value>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub singer_id: Option<String>,
    // Provider-specific extra fields
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub song_id: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub str_media_mid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album_mid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copyright_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lrc_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mrc_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trc_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub list: Vec<MusicItem>,
    pub all_page: i64,
    pub limit: i64,
    pub total: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistItem {
    pub id: serde_json::Value,
    pub name: String,
    pub img: String,
    pub source: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub play_count: serde_json::Value,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub total: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistResult {
    pub list: Vec<PlaylistItem>,
    pub all_page: i64,
    pub limit: i64,
    pub total: i64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub img: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistDetail {
    pub list: Vec<MusicItem>,
    pub info: PlaylistInfo,
    pub all_page: i64,
    pub limit: i64,
    pub total: i64,
    pub source: String,
}

/// Profile of a singer page (`SingerInfo` of the original). Sources without a profile endpoint
/// have none and the page falls back to the name of the song it was opened from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SingerInfo {
    pub id: String,
    pub source: String,
    pub name: String,
    pub desc: String,
    pub avatar: String,
    pub gender: Option<String>,
    pub music_count: i64,
    pub album_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SingerAlbum {
    pub id: String,
    pub count: i64,
    pub name: String,
    pub author: String,
    pub img: String,
    pub desc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SingerAlbumPage {
    pub list: Vec<SingerAlbum>,
    pub all_page: i64,
    pub limit: i64,
    pub total: i64,
    pub source: String,
}
