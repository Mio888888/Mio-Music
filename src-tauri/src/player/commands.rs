use crate::player::media_control::MEDIA_CONTROL;
use crate::player::{AudioSlot, SharedPlayer};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct CommandResult<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> CommandResult<T> {
    pub fn ok(data: T) -> Self {
        Self { success: true, data: Some(data), error: None }
    }
    pub fn err(msg: impl Into<String>) -> Self {
        Self { success: false, data: None, error: Some(msg.into()) }
    }
}

type CmdResult<T> = Result<CommandResult<T>, String>;

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__play(
    player: State<'_, SharedPlayer>,
    url: String,
    slot: Option<String>,
) -> CmdResult<()> {
    let audio_slot = slot.and_then(|s| match s.to_uppercase().as_str() {
        "A" => Some(AudioSlot::A),
        "B" => Some(AudioSlot::B),
        _ => None,
    });
    let mut engine = player.lock();
    match engine.play_async(&url, audio_slot) {
        Ok(()) => Ok(CommandResult::ok(())),
        Err(e) => Ok(CommandResult::err(e)),
    }
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__pause(player: State<'_, SharedPlayer>) -> CmdResult<()> {
    player.lock().pause();
    MEDIA_CONTROL.lock().set_playback_state(false);
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__resume(player: State<'_, SharedPlayer>) -> CmdResult<()> {
    player.lock().resume();
    MEDIA_CONTROL.lock().set_playback_state(true);
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__stop(
    player: State<'_, SharedPlayer>,
    slot: Option<String>,
) -> CmdResult<()> {
    let mut engine = player.lock();
    match slot {
        Some(s) => {
            let audio_slot = match s.to_uppercase().as_str() {
                "A" => AudioSlot::A,
                _ => AudioSlot::B,
            };
            engine.stop_slot(audio_slot);
        }
        None => engine.stop_all(),
    }
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__seek(player: State<'_, SharedPlayer>, position: f64) -> CmdResult<()> {
    player.lock().seek(position);
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__set_volume(player: State<'_, SharedPlayer>, volume: f64) -> CmdResult<()> {
    player.lock().set_volume(volume);
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__get_state(player: State<'_, SharedPlayer>) -> CmdResult<serde_json::Value> {
    let engine = player.lock();
    let snap = engine.snapshot();
    Ok(CommandResult::ok(serde_json::to_value(snap).unwrap_or_default()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__crossfade(
    player: State<'_, SharedPlayer>,
    url: String,
    duration_ms: Option<u64>,
) -> CmdResult<()> {
    let mut engine = player.lock();
    match engine.crossfade_to(&url, duration_ms.unwrap_or(2000)) {
        Ok(()) => Ok(CommandResult::ok(())),
        Err(e) => Ok(CommandResult::err(e)),
    }
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__swap_slot(player: State<'_, SharedPlayer>) -> CmdResult<()> {
    player.lock().swap_primary();
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__set_eq_band(
    player: State<'_, SharedPlayer>,
    index: usize,
    gain: f64,
) -> CmdResult<()> {
    player.lock().set_eq_band(index, gain);
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__get_eq_bands(player: State<'_, SharedPlayer>) -> CmdResult<Vec<f64>> {
    Ok(CommandResult::ok(player.lock().get_eq_bands()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__set_bass_boost(player: State<'_, SharedPlayer>, gain: f64) -> CmdResult<()> {
    player.lock().set_bass_boost(gain);
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__set_balance(player: State<'_, SharedPlayer>, value: f64) -> CmdResult<()> {
    player.lock().set_balance(value);
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__update_now_playing(
    title: String,
    artist: String,
    album: String,
    duration: f64,
    cover_url: Option<String>,
) -> CmdResult<()> {
    MEDIA_CONTROL.lock().update_now_playing(
        &title, &artist, &album, duration, cover_url.as_deref(),
    );
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__shutdown(player: State<'_, SharedPlayer>) -> CmdResult<()> {
    player.lock().shutdown();
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__preload(
    player: State<'_, SharedPlayer>,
    url: String,
) -> CmdResult<()> {
    player.lock().preload(url);
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__gapless_swap(player: State<'_, SharedPlayer>) -> CmdResult<bool> {
    let swapped = player.lock().gapless_swap();
    Ok(CommandResult::ok(swapped))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__clear_secondary(player: State<'_, SharedPlayer>) -> CmdResult<()> {
    player.lock().clear_secondary();
    Ok(CommandResult::ok(()))
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn player__set_seamless_config(
    player: State<'_, SharedPlayer>,
    mode: String,
    crossfade_duration_ms: Option<u64>,
) -> CmdResult<()> {
    let auto_crossfade = mode == "crossfade";
    let duration = crossfade_duration_ms.unwrap_or(3000);
    player.lock().set_seamless_config(auto_crossfade, duration);
    Ok(CommandResult::ok(()))
}
