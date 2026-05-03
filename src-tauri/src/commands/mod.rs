pub mod music_commands;
pub mod playlist_commands;
pub mod config_commands;
pub mod hotkey_commands;
pub mod directory_commands;
pub mod power_save;

use crate::db::AppDb;
use base64::Engine;
use serde_json::Value;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

pub type DbState<'a> = State<'a, AppDb>;

/// Shared state for the desktop lyric window
pub struct DesktopLyricState {
    pub is_open: Mutex<bool>,
    pub is_locked: Mutex<bool>,
}

/// Open or close the desktop lyric window.
///
/// Called via `ipcSend('change-desktop-lyric', bool)` which maps to
/// `invoke('change_desktop_lyric', { args: [bool] })`.
#[tauri::command]
pub async fn change_desktop_lyric(
    app: AppHandle,
    state: State<'_, DesktopLyricState>,
    args: Vec<bool>,
) -> Result<(), String> {
    let open = args.first().copied().unwrap_or(false);

    if open {
        // If window already exists, just show/focus it
        if let Some(existing) = app.get_webview_window("desktop-lyric") {
            let _ = existing.show();
            let _ = existing.set_focus();
            *state.is_open.lock().map_err(|e| e.to_string())? = true;
            return Ok(());
        }

        // Create a new desktop lyric window
        let label = "desktop-lyric";
        let win_width = 800.0_f64;
        let win_height = 180.0_f64;

        let mut builder = WebviewWindowBuilder::new(
            &app,
            label,
            WebviewUrl::App("#/desktop-lyric".into()),
        )
        .title("Desktop Lyric")
        .inner_size(win_width, win_height)
        .always_on_top(true)
        .decorations(false)
        .transparent(true)
        .skip_taskbar(true)
        .resizable(true);

        // Position the window at bottom-center of the primary monitor
        if let Ok(monitor) = app.primary_monitor() {
            if let Some(m) = monitor {
                let scale = m.scale_factor();
                let screen_w = m.size().width as f64 / scale;
                let screen_h = m.size().height as f64 / scale;
                let x = (screen_w - win_width) / 2.0;
                let y = screen_h - win_height - 100.0;
                builder = builder.position(x, y);
            }
        }

        builder
            .build()
            .map_err(|e| format!("创建桌面歌词窗口失败: {}", e))?;

        *state.is_open.lock().map_err(|e| e.to_string())? = true;
    } else {
        // Close the desktop lyric window
        if let Some(window) = app.get_webview_window("desktop-lyric") {
            window.close().map_err(|e| format!("关闭桌面歌词窗口失败: {}", e))?;
        }
        *state.is_open.lock().map_err(|e| e.to_string())? = false;
    }

    Ok(())
}

/// Toggle the desktop lyric window's cursor-events-ignore (lock) state.
///
/// When locked the window ignores mouse events (click-through);
/// when unlocked it accepts pointer events normally.
///
/// Called via `ipcSend('toogleDesktopLyricLock', bool)`.
#[tauri::command]
pub async fn toogle_desktop_lyric_lock(
    app: AppHandle,
    state: State<'_, DesktopLyricState>,
    args: Vec<bool>,
) -> Result<(), String> {
    let locked = args.first().copied().unwrap_or(false);

    if let Some(window) = app.get_webview_window("desktop-lyric") {
        window
            .set_ignore_cursor_events(locked)
            .map_err(|e| format!("设置桌面歌词锁定状态失败: {}", e))?;
    }

    *state.is_locked.lock().map_err(|e| e.to_string())? = locked;
    Ok(())
}

/// Return whether the desktop lyric window is currently open.
#[tauri::command]
pub async fn get_lyric_open_state(state: State<'_, DesktopLyricState>) -> Result<bool, String> {
    let is_open = state.is_open.lock().map_err(|e| e.to_string())?;
    Ok(*is_open)
}

/// Return whether the desktop lyric window is currently locked (ignoring cursor events).
#[tauri::command]
pub async fn get_lyric_lock_state(state: State<'_, DesktopLyricState>) -> Result<bool, String> {
    let is_locked = state.is_locked.lock().map_err(|e| e.to_string())?;
    Ok(*is_locked)
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

/// 桌面歌词选项 — 从 app_data_dir/desktop_lyric_option.json 读取
#[tauri::command]
pub async fn get_desktop_lyric_option() -> Result<Option<Value>, String> {
    let dir = crate::db::get_app_data_dir();
    let path = dir.join("desktop_lyric_option.json");
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&path).map_err(|e| format!("读取桌面歌词配置失败: {}", e))?;
    let val: Value = serde_json::from_str(&data).map_err(|e| format!("解析桌面歌词配置失败: {}", e))?;
    Ok(Some(val))
}

/// 桌面歌词选项 — 保存到 app_data_dir/desktop_lyric_option.json 并广播事件到桌面歌词窗口
#[tauri::command]
pub async fn set_desktop_lyric_option(
    app: AppHandle,
    args: Vec<Value>,
) -> Result<(), String> {
    let options = args.first().ok_or("缺少配置参数")?.clone();
    let dir = crate::db::get_app_data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let path = dir.join("desktop_lyric_option.json");
    let data = serde_json::to_string_pretty(&options).map_err(|e| format!("序列化配置失败: {}", e))?;
    std::fs::write(&path, data).map_err(|e| format!("写入桌面歌词配置失败: {}", e))?;
    // 广播到桌面歌词窗口使其实时更新
    app.emit("desktop-lyric-style-change", options).map_err(|e| format!("广播样式事件失败: {}", e))?;
    Ok(())
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

    // 从 URL 推导 Referer 和 Origin
    let referer_origin = url.split("://").nth(1)
        .and_then(|host_port| host_port.split('/').next())
        .map(|host| format!("https://{}", host))
        .unwrap_or_default();

    // 构建请求（带重试）
    let max_attempts = 3u32;
    let mut last_status: u16 = 0;
    let mut resp: Option<reqwest::Response> = None;

    for attempt in 0..max_attempts {
        let mut req_builder: reqwest::RequestBuilder = match method.to_uppercase().as_str() {
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            _ => client.get(url),
        };

        // 默认浏览器请求头（插件可覆盖）
        req_builder = req_builder
            .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .header("Accept", "*/*")
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Referer", &referer_origin)
            .header("Origin", &referer_origin)
            .header("Sec-Fetch-Dest", "empty")
            .header("Sec-Fetch-Mode", "cors")
            .header("Sec-Fetch-Site", "cross-site");

        if let Some(ref hdrs) = headers {
            for (k, v) in hdrs {
                req_builder = req_builder.header(k.as_str(), v.as_str());
            }
        }
        if let Some(b) = body {
            req_builder = req_builder.body(b.to_string());
        }

        let r = req_builder.send().await.map_err(|e| format!("请求失败: {}", e))?;
        last_status = r.status().as_u16();

        // 只有 5xx 才重试
        if r.status().is_server_error() && attempt + 1 < max_attempts {
            tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1))).await;
            continue;
        }
        resp = Some(r);
        break;
    }

    let resp = resp.ok_or_else(|| format!("请求失败: HTTP {}", last_status))?;
    let status = resp.status().as_u16();
    let resp_headers: std::collections::HashMap<String, String> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    if raw {
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
