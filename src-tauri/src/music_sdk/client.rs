use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use once_cell::sync::Lazy;

#[allow(dead_code)]
static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .unwrap_or_default()
});

#[allow(dead_code)]
pub fn get_client() -> &'static Client {
    &HTTP_CLIENT
}

// --- Shared types matching CeruMusic's MusicItem ---

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
    #[serde(default)]
    pub type_url: Option<serde_json::Value>,
    #[serde(default)]
    pub hash: Option<String>,
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
pub struct PlaylistDetailResult {
    pub list: Vec<MusicItem>,
    pub info: serde_json::Value,
    pub all_page: i64,
    pub limit: i64,
    pub total: i64,
    pub source: String,
}

/// Source-specific SDK implementations.
/// Each source handles its own API endpoints.
/// For now, we use a registry pattern where source handlers can be added.

use std::sync::OnceLock;

#[allow(dead_code)]
type SdkHandler = fn(&str, serde_json::Value) -> Result<serde_json::Value, String>;

#[allow(dead_code)]
static SDK_HANDLERS: OnceLock<HashMap<String, SdkHandler>> = OnceLock::new();

#[allow(dead_code)]
fn get_handlers() -> &'static HashMap<String, SdkHandler> {
    SDK_HANDLERS.get_or_init(|| {
        let m = HashMap::new();
        // Handlers will be registered as they're implemented
        m
    })
}

/// Main entry point for SDK requests.
/// Routes to the appropriate source handler based on the current source.
pub async fn handle_request(method: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("kw").to_string();
    crate::music_sdk::sources::dispatch(&source, method, args).await
}
