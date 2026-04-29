#![allow(non_snake_case)]

use crate::plugin::manager::PluginManager;
use serde_json::Value;
use tauri::State;

type ApiResult = Result<serde_json::Value, String>;

fn ok(data: Value) -> ApiResult {
    Ok(serde_json::json!({ "success": true, "data": data }))
}

fn ok_null() -> ApiResult {
    Ok(serde_json::json!({ "success": true, "data": null }))
}

#[tauri::command]
pub async fn plugin__initialize(pm: State<'_, PluginManager>) -> ApiResult {
    let plugins = pm.initialize().await?;
    ok(serde_json::to_value(plugins).unwrap_or_default())
}

#[tauri::command]
pub async fn plugin__get_list(pm: State<'_, PluginManager>) -> ApiResult {
    let plugins = pm.get_plugins_list().await;
    ok(serde_json::to_value(plugins).unwrap_or_default())
}

#[tauri::command]
pub async fn plugin__add(
    pm: State<'_, PluginManager>,
    plugin_code: String,
    plugin_name: String,
    target_plugin_id: Option<String>,
) -> ApiResult {
    let result = pm.add_plugin(&plugin_code, &plugin_name, target_plugin_id).await?;
    ok(serde_json::to_value(result).unwrap_or_default())
}

#[tauri::command]
pub async fn plugin__uninstall(pm: State<'_, PluginManager>, plugin_id: String) -> ApiResult {
    pm.uninstall_plugin(&plugin_id).await?;
    ok_null()
}

#[tauri::command]
pub async fn plugin__get_info(pm: State<'_, PluginManager>, plugin_id: String) -> ApiResult {
    let plugins = pm.get_plugins_list().await;
    let plugin = plugins.iter().find(|p| p.plugin_id == plugin_id)
        .ok_or_else(|| format!("插件 {} 未找到", plugin_id))?;
    ok(serde_json::to_value(plugin).unwrap_or_default())
}

#[tauri::command]
pub async fn plugin__call_method(
    pm: State<'_, PluginManager>,
    plugin_id: String,
    method: String,
    args_json: String,
) -> ApiResult {
    let result = pm.call_plugin_method(&plugin_id, &method, &args_json)?;
    let parsed: Value = serde_json::from_str(&result).unwrap_or(Value::String(result));
    ok(parsed)
}

#[tauri::command]
pub async fn plugin__download_and_add(
    pm: State<'_, PluginManager>,
    url: String,
    plugin_type: String,
    target_plugin_id: Option<String>,
) -> ApiResult {
    // Download plugin code from URL
    let response = reqwest::get(&url).await.map_err(|e| format!("下载失败: {}", e))?;
    let plugin_code = response.text().await.map_err(|e| format!("读取响应失败: {}", e))?;

    // Validate plugin type
    if plugin_type == "cr" && !plugin_code.to_lowercase().contains("cerumusic") {
        return Err("澜音插件格式校验失败".into());
    }
    if plugin_type == "lx" && !plugin_code.to_lowercase().contains("lx") {
        return Err("洛雪插件格式校验失败".into());
    }

    let file_name = format!("downloaded_{}", chrono::Utc::now().timestamp_millis());
    let result = pm.add_plugin(&plugin_code, &file_name, target_plugin_id).await?;
    ok(serde_json::to_value(result).unwrap_or_default())
}
