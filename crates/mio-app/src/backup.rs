//! Desktop backup data collection and application; protocol shared with mobile.
use crate::{
    directories::Directories,
    storage::{Preferences, Storage},
};
pub use mio_core::backup::{
    Mode, Payload, RestoreDone, S3Config, Saved, backup, fetch_latest, test_connection, validate,
};
use serde_json::Value;

// ---------------------------------------------------------------- local data

/// Desktop host projection; default directory selection stays in the desktop host.
fn projected_directories(d: &Directories) -> mio_core::backup::Directories<'_> {
    mio_core::backup::Directories {
        cache_dir: &d.cache_dir,
        download_dir: &d.download_dir,
    }
}
pub fn collect(
    store: &Storage,
    preferences: &Preferences,
    directories: &Directories,
) -> Result<Payload, String> {
    mio_core::backup::collect(store, preferences, &projected_directories(directories))
}
pub fn apply_settings(
    preferences: &mut Preferences,
    directories: &Directories,
    backup: &Value,
    mode: Mode,
) {
    mio_core::backup::apply_settings(
        preferences,
        &projected_directories(directories),
        backup,
        mode,
    )
}

fn valid_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Existing plugins and the active selection are retained; new valid entries are installed inactive.
fn restore_plugins(store: &Storage, plugins: &[Value]) -> Result<u32, String> {
    for plugin in plugins {
        let id = plugin.get("id").and_then(Value::as_str).unwrap_or("");
        if !valid_plugin_id(id) {
            return Err("插件 ID 格式无效".into());
        }
    }
    install_usable(
        store,
        plugins
            .iter()
            .filter_map(|plugin| plugin.get("code").and_then(Value::as_str)),
    )
}

/// Used by backup restoration and the one-time legacy folder import; the registry handles bounds and deduplication.
pub fn install_usable<C: AsRef<str>>(
    store: &Storage,
    codes: impl Iterator<Item = C>,
) -> Result<u32, String> {
    crate::online::Online::merge_plugins(store, codes)
}

/// Applies a checked backup: playlists in one transaction, then the plugin, then the settings.
pub fn apply(store: &Storage, data: &Value, overwrite: bool) -> Result<RestoreDone, String> {
    validate(data)?;
    let playlists = data.get("playlists").ok_or("备份缺少歌单数据")?;
    let (playlist_count, _) = crate::library::check_backup(playlists).ok_or("歌单备份格式错误")?;
    let plugins: Vec<Value> = data
        .get("plugins")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // Plugin ids are checked before anything is written.
    for plugin in &plugins {
        let id = plugin.get("id").and_then(Value::as_str).unwrap_or("");
        if !valid_plugin_id(id) {
            return Err("插件 ID 格式无效".into());
        }
    }
    crate::library::restore_backup(&store.library_path(), playlists, overwrite)
        .map_err(|_| "恢复歌单失败".to_string())?;
    let restored = restore_plugins(store, &plugins)?;
    let directories = Directories::load(Some(store));
    let mut preferences = store
        .load::<Preferences>("preferences.json")
        .ok()
        .flatten()
        .unwrap_or_default();
    apply_settings(
        &mut preferences,
        &directories,
        data.get("settings").unwrap_or(&Value::Null),
        if overwrite {
            Mode::Overwrite
        } else {
            Mode::Merge
        },
    );
    store
        .save("preferences.json", &preferences)
        .map_err(|_| "保存设置失败".to_string())?;
    Ok(RestoreDone {
        playlists: playlist_count,
        plugins: restored,
    })
}
