pub mod kw;
pub mod wy;
pub mod kg;
pub mod tx;
pub mod mg;
pub mod bd;
pub mod xm;
pub mod git;

use crate::music_sdk::client::{PlaylistDetailResult, PlaylistResult, SearchResult};
use serde_json::json;

pub async fn dispatch(source: &str, method: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    match source {
        "kw" => kw::handle(method, args).await,
        "bd" => bd::handle(method, args).await,
        "wy" => wy::handle(method, args).await,
        "kg" => kg::handle(method, args).await,
        "tx" => tx::handle(method, args).await,
        "mg" => mg::handle(method, args).await,
        "xm" => xm::handle(method, args).await,
        "git" => git::handle(method, args).await,
        _ => stub_response(method, source, args),
    }
}

fn stub_response(method: &str, source: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    let src = args.get("source").and_then(|v| v.as_str()).unwrap_or(source).to_string();
    match method {
        "search" => Ok(json!(SearchResult { list: vec![], all_page: 0, limit: 30, total: 0, source: src })),
        "tipSearch" | "hotSearch" => Ok(json!({ "list": [] })),
        "getHotSonglist" | "getHotPlaylists" | "searchPlaylist" | "getCategoryPlaylists" =>
            Ok(json!(PlaylistResult { list: vec![], all_page: 0, limit: 30, total: 0, source: src })),
        "getPlaylistTags" | "getSongboardTags" => Ok(json!({ "list": [] })),
        "getPlaylistDetail" | "getPlaylistDetailById" | "getLeaderboardDetail" =>
            Ok(json!(PlaylistDetailResult { list: vec![], info: json!({}), all_page: 0, limit: 30, total: 0, source: src })),
        "getLeaderboards" => Ok(json!({ "list": [], "source": src })),
        "getMusicUrl" => Ok(json!({ "url": "" })),
        "getPic" => Ok(json!({ "url": "" })),
        "getLyric" => Ok(json!({ "lrc": "" })),
        "getHotComment" | "getComment" => Ok(json!({ "comments": [], "total": 0 })),
        _ => Err(format!("Unknown SDK method: {}", method)),
    }
}
