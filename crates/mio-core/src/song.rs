//! The song and album records the app passes around (queue, library, search results). They
//! began as the Subsonic client's types; the client is gone, the records remain.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub song_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub cover_art: Option<String>,
}

/// Quality tiers shown for a song, not stream properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SongQuality {
    Lossless,
    High,
    Standard,
}

#[derive(Clone, Deserialize)]
pub struct Lyrics {
    pub synced: bool,
    #[serde(default)]
    pub offset: i64,
    pub line: Vec<LyricLine>,
}

#[derive(Clone, Deserialize)]
pub struct LyricLine {
    pub start: Option<u64>,
    pub value: String,
}
