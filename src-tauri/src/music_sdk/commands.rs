#![allow(non_snake_case)]

use crate::music_sdk::client;

#[tauri::command]
pub async fn service_music_sdk_request(
    method: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    client::handle_request(&method, args).await
}

#[tauri::command]
pub async fn service_music_tip_search(
    source: String,
    keyword: String,
) -> Result<serde_json::Value, String> {
    client::handle_request("tipSearch", serde_json::json!({ "source": source, "keyword": keyword })).await
}
