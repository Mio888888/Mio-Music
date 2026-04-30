mod macos;

use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceInfo {
    pub id: u32,
    pub name: String,
    pub is_default: bool,
    pub sample_rate: f64,
    pub channels: u32,
    pub volume: f32,
    pub volume_supported: bool,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[allow(non_snake_case)]
#[tauri::command]
pub fn audio__enumerate_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    macos::enumerate_devices()
        .map(|devices| {
            devices
                .into_iter()
                .map(|d| AudioDeviceInfo {
                    id: d.id,
                    name: d.name,
                    is_default: d.is_default,
                    sample_rate: d.sample_rate,
                    channels: d.channels,
                    volume: d.volume,
                    volume_supported: d.volume_supported,
                })
                .collect()
        })
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn audio__set_output_device(device_id: u32) -> Result<(), String> {
    macos::set_output_device(device_id)
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn audio__get_device_volume(device_id: u32) -> Result<f32, String> {
    macos::get_device_volume_pub(device_id)
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn audio__set_device_volume(device_id: u32, volume: f32) -> Result<(), String> {
    macos::set_device_volume(device_id, volume)
}

pub fn start_device_listener(app_handle: AppHandle) {
    macos::start_listening(app_handle);
}
