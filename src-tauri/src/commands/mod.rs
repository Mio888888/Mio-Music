pub mod music_commands;
pub mod playlist_commands;
pub mod config_commands;
pub mod hotkey_commands;
pub mod directory_commands;

use crate::db::AppDb;
use base64::Engine;
use serde_json::Value;
use tauri::State;

pub type DbState<'a> = State<'a, AppDb>;

/// Stub: desktop lyric window open state (not yet implemented)
#[tauri::command]
pub async fn get_lyric_open_state() -> Result<bool, String> {
    Ok(false)
}

/// Stub: desktop lyric lock state (not yet implemented)
#[tauri::command]
pub async fn get_lyric_lock_state() -> Result<bool, String> {
    Ok(false)
}

/// 获取系统已安装字体列表
#[tauri::command]
pub async fn get_font_list() -> Result<Vec<String>, String> {
    let mut fonts: Vec<String> = Vec::new();

    // macOS 系统字体目录
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    dirs.push(std::path::PathBuf::from("/System/Library/Fonts"));
    dirs.push(std::path::PathBuf::from("/Library/Fonts"));
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("Library/Fonts"));
    }

    let font_exts = ["ttf", "otf", "ttc", "TTF", "OTF", "TTC"];
    for dir in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if font_exts.contains(&ext) {
                        if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                            let name = name.to_string();
                            if !fonts.contains(&name) {
                                fonts.push(name);
                            }
                        }
                    }
                }
            }
        }
    }

    fonts.sort();
    Ok(fonts)
}

/// 桌面歌词选项（暂未实现，返回 null 让前端使用默认值）
#[tauri::command]
pub async fn get_desktop_lyric_option() -> Result<Option<Value>, String> {
    Ok(None)
}

/// Audio proxy — fetches a remote audio URL via Rust backend (bypassing CORS)
/// and returns a `data:` URI that the WebView `<audio>` element can play.
/// Follows up to 10 redirects.
#[tauri::command]
pub async fn audio_proxy(url: String) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("创建音频代理客户端失败: {}", e))?;

    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")
        .send()
        .await
        .map_err(|e| format!("音频请求失败: {}", e))?;

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/mpeg")
        .to_string();

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取音频数据失败: {}", e))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", content_type, b64))
}

/// HTTP proxy command — bypasses CORS by making requests from the Rust backend.
/// Used by plugins in the WebView that need to call cross-origin APIs.
///
/// Supports `raw` mode: when `raw: true`, returns the response body as a base64 string
/// (useful for binary data like audio files). The response format changes:
///   { "statusCode": N, "headers": {}, "body": "<base64>", "isBase64": true }
///
/// By default (raw: false), follows redirects and returns JSON-wrapped response:
///   { "statusCode": N, "headers": {}, "body": <parsed JSON or string> }
#[tauri::command]
pub async fn http_proxy(args: Value) -> Result<Value, String> {
    let url = args.get("url").and_then(|v| v.as_str()).ok_or("缺少 url 参数")?;
    let method = args.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
    let headers: Option<std::collections::HashMap<String, String>> =
        args.get("headers").and_then(|v| serde_json::from_value(v.clone()).ok());
    let body = args.get("body").and_then(|v| v.as_str());
    let timeout_ms = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(15000);
    let raw = args.get("raw").and_then(|v| v.as_bool()).unwrap_or(false);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let mut req_builder = match method.to_uppercase().as_str() {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        _ => client.get(url),
    };

    if let Some(hdrs) = headers {
        for (k, v) in &hdrs {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }
    }
    if let Some(b) = body {
        req_builder = req_builder.body(b.to_string());
    }

    let resp = req_builder.send().await.map_err(|e| format!("请求失败: {}", e))?;
    let status = resp.status().as_u16();
    let resp_headers: std::collections::HashMap<String, String> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    if raw {
        // Raw mode: return binary body as base64 (for audio/video/image data)
        let bytes = resp.bytes().await.map_err(|e| format!("读取响应失败: {}", e))?;
        let body_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Ok(serde_json::json!({
            "statusCode": status,
            "headers": resp_headers,
            "body": body_base64,
            "isBase64": true
        }));
    }

    let text = resp.text().await.map_err(|e| format!("读取响应失败: {}", e))?;

    let body_value: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));

    Ok(serde_json::json!({
        "statusCode": status,
        "headers": resp_headers,
        "body": body_value
    }))
}
