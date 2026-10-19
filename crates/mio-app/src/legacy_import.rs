//! One-time import of the original (Tauri) application's data, PLAN §6.4.
//!
//! The original keeps its data in two places: the app-data folder (`playlists.db` with a
//! `kv_store` holding the hotkeys and the folders, the `plugins` folder, `downloads.json`,
//! `desktop_lyric_window.json`) and the WebView's `localStorage` (`appSettings`, `playSetting`,
//! `userInfo`). Everything is read from copies in a temporary folder — the original's files are
//! never opened for writing — and added to this application's own data: playlists are merged, the
//! plugin registry keeps existing entries and merges valid new plugins, and single files are only taken when this
//! application has none of its own. Running it again changes nothing.
//!
//! `localStorage` is a SQLite file for WebKit (macOS, Linux). On Windows WebView2 keeps it in a
//! LevelDB folder, which `legacy_leveldb` reads from a copy.
use crate::{
    backup::{self, Mode},
    directories::Directories,
    storage::{Preferences, Storage},
};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const IDENTIFIER: &str = "com.vant.Mio.Music";
const DIR_ENV: &str = "MIO_GPUI_LEGACY_DIR";
const LOCAL_STORAGE_ENV: &str = "MIO_GPUI_LEGACY_LOCAL_STORAGE";
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// Where the original's data was found.
#[derive(Clone, Debug, Default)]
pub struct Source {
    pub data_dir: Option<PathBuf>,
    pub local_storage: Option<PathBuf>,
}
impl Source {
    pub fn found(&self) -> bool {
        self.data_dir.is_some() || self.local_storage.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Playlists in the original's database, and how many of them were new here.
    pub playlists_seen: usize,
    pub playlists_added: usize,
    pub plugin: bool,
    pub settings: bool,
    pub hotkeys: bool,
    pub directories: bool,
    pub downloads: bool,
    pub desktop_lyric: bool,
    pub equalizer: bool,
    pub audio_effects: bool,
    /// Tracks taken over from the original's `local-music.db`.
    pub local_tracks: usize,
}
impl Report {
    pub fn changed(&self) -> bool {
        self.playlists_added > 0
            || self.plugin
            || self.settings
            || self.hotkeys
            || self.directories
            || self.downloads
            || self.desktop_lyric
            || self.equalizer
            || self.audio_effects
            || self.local_tracks > 0
    }
}

fn app_data_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(DIR_ENV) {
        return Some(PathBuf::from(path));
    }
    if cfg!(target_os = "macos") {
        Some(
            PathBuf::from(std::env::var_os("HOME")?)
                .join("Library/Application Support")
                .join(IDENTIFIER),
        )
    } else if cfg!(target_os = "windows") {
        Some(PathBuf::from(std::env::var_os("APPDATA")?).join(IDENTIFIER))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .map(|root| root.join(IDENTIFIER))
    }
}

fn child_dirs(path: &Path) -> Vec<PathBuf> {
    fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// The WebView's `localStorage` files that could belong to the original.
fn local_storage_candidates(data_dir: Option<&Path>) -> Vec<PathBuf> {
    if let Some(path) = std::env::var_os(LOCAL_STORAGE_ENV) {
        return vec![PathBuf::from(path)];
    }
    let mut found = Vec::new();
    if cfg!(target_os = "macos") {
        let Some(home) = std::env::var_os("HOME") else {
            return found;
        };
        let default = PathBuf::from(home)
            .join("Library/WebKit")
            .join(IDENTIFIER)
            .join("WebsiteData/Default");
        for first in child_dirs(&default) {
            for second in child_dirs(&first) {
                let file = second.join("LocalStorage/localstorage.sqlite3");
                if file.is_file() {
                    found.push(file);
                }
            }
        }
    } else if cfg!(target_os = "windows") {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let folder = PathBuf::from(local)
                .join(IDENTIFIER)
                .join("EBWebView/Default/Local Storage/leveldb");
            if folder.is_dir() {
                found.push(folder);
            }
        }
    } else if cfg!(target_os = "linux")
        && let Some(data_dir) = data_dir
        && let Ok(entries) = fs::read_dir(data_dir.join("localstorage"))
    {
        found.extend(
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "localstorage")),
        );
    }
    found
}

/// Keys of the original's stores that this import reads.
const KEYS: [&str; 5] = [
    "appSettings",
    "playSetting",
    "userInfo",
    "equalizer",
    "audioEffects",
];

fn temp_copy(file: &Path, dir: &Path) -> Option<PathBuf> {
    let name = file.file_name()?;
    let target = dir.join(name);
    fs::copy(file, &target).ok()?;
    for suffix in ["-wal", "-shm"] {
        let mut companion = file.as_os_str().to_owned();
        companion.push(suffix);
        let companion = PathBuf::from(companion);
        if companion.is_file() {
            let mut destination = target.as_os_str().to_owned();
            destination.push(suffix);
            let _ = fs::copy(companion, PathBuf::from(destination));
        }
    }
    Some(target)
}

fn decode_value(bytes: &[u8]) -> Option<String> {
    // WebKit stores the value as UTF-16LE; fall back to UTF-8 when that is not JSON.
    if bytes.len().is_multiple_of(2) {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        if let Ok(text) = String::from_utf16(&units)
            && serde_json::from_str::<Value>(&text).is_ok()
        {
            return Some(text);
        }
    }
    String::from_utf8(bytes.to_vec()).ok()
}

/// The items of a WebView2 `localStorage` LevelDB folder, read from a copy: the origin holding
/// most of [`KEYS`] (the original runs from `tauri.localhost`).
fn read_leveldb_items(folder: &Path) -> HashMap<String, String> {
    let Ok(temp) = tempfile::tempdir() else {
        return HashMap::new();
    };
    let Ok(entries) = fs::read_dir(folder) else {
        return HashMap::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let wanted = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e, "ldb" | "sst" | "log"));
        if wanted
            && path.is_file()
            && let Some(name) = path.file_name()
        {
            let _ = fs::copy(&path, temp.path().join(name));
        }
    }
    crate::legacy_leveldb::read_local_storage(temp.path())
        .into_iter()
        .max_by_key(|(origin, items)| {
            (
                KEYS.iter().filter(|key| items.contains_key(**key)).count(),
                origin.contains("tauri.localhost"),
            )
        })
        .map(|(_, items)| items)
        .unwrap_or_default()
}

/// Every item of a `localStorage` store: a WebKit database (read from a copy) or a WebView2
/// LevelDB folder.
fn read_items(file: &Path) -> HashMap<String, String> {
    if file.is_dir() {
        return read_leveldb_items(file);
    }
    let Ok(temp) = tempfile::tempdir() else {
        return HashMap::new();
    };
    let Some(copy) = temp_copy(file, temp.path()) else {
        return HashMap::new();
    };
    let Ok(connection) = rusqlite::Connection::open_with_flags(
        &copy,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return HashMap::new();
    };
    let Ok(mut statement) = connection.prepare("SELECT key, value FROM ItemTable") else {
        return HashMap::new();
    };
    let rows = statement.query_map([], |row| {
        let key: String = row.get(0)?;
        let value = match row.get_ref(1)? {
            rusqlite::types::ValueRef::Blob(bytes) => decode_value(bytes),
            rusqlite::types::ValueRef::Text(bytes) => String::from_utf8(bytes.to_vec()).ok(),
            _ => None,
        };
        Ok((key, value))
    });
    let mut items = HashMap::new();
    if let Ok(rows) = rows {
        for (key, value) in rows.flatten() {
            if let Some(value) = value {
                items.insert(key, value);
            }
        }
    }
    items
}

/// The `localStorage` file that holds the original's stores: the one with most of [`KEYS`].
fn best_local_storage(candidates: Vec<PathBuf>) -> Option<(PathBuf, HashMap<String, String>)> {
    candidates
        .into_iter()
        .map(|path| {
            let items = read_items(&path);
            let score = KEYS.iter().filter(|key| items.contains_key(**key)).count();
            let modified = fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (score, modified, path, items)
        })
        .filter(|(score, ..)| *score > 0)
        .max_by_key(|(score, modified, ..)| (*score, *modified))
        .map(|(_, _, path, items)| (path, items))
}

/// Looks for the original's data on this machine.
pub fn find() -> Source {
    let data_dir = app_data_dir().filter(|path| {
        path.join("playlists.db").is_file()
            || path.join("plugins").is_dir()
            || path.join("downloads.json").is_file()
    });
    let local_storage =
        best_local_storage(local_storage_candidates(data_dir.as_deref())).map(|(path, _)| path);
    Source {
        data_dir,
        local_storage,
    }
}

fn kv_value(connection: &rusqlite::Connection, key: &str) -> Option<String> {
    connection
        .query_row("SELECT value FROM kv_store WHERE key=?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .ok()
}

fn copy_json_if_missing(from: &Path, store: &Storage, name: &str) -> bool {
    if store.root().join(name).exists() {
        return false;
    }
    let Ok(metadata) = fs::metadata(from) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return false;
    }
    let Ok(bytes) = fs::read(from) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    store.save(name, &value).is_ok()
}

fn flag(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

/// `playSetting` and `userInfo` of the original in this application's preferences.
fn apply_stores(preferences: &mut Preferences, play: Option<&Value>, user: Option<&Value>) -> bool {
    let mut changed = false;
    if let Some(play) = play.filter(|v| v.is_object()) {
        if let Some(v) = flag(play, "isJumpLyric") {
            preferences.app.jump_lyric = v;
        }
        if let Some(v) = flag(play, "bgPlaying") {
            preferences.app.bg_playing = v;
        }
        if let Some(v) = flag(play, "isAudioVisualizer") {
            preferences.app.audio_visualizer = v;
        }
        if let Some(v) = flag(play, "showLeftPanel") {
            preferences.show_left_panel = v;
        }
        if let Some(v) = flag(play, "autoHideBottom") {
            preferences.auto_hide_bottom = v;
        }
        if let Some(v) = flag(play, "isPauseTransition") {
            preferences.pause_transition = v;
        }
        if let Some(mode) = play.get("layoutMode").and_then(Value::as_str) {
            preferences.cover_layout = mode == "cover";
        }
        changed = true;
    }
    if let Some(v) = user.and_then(|user| flag(user, "topBarStyle")) {
        preferences.app.traffic_light = v;
        changed = true;
    }
    changed
}

/// The original's `equalizer` store as this application's `equalizer.json`, when it has none.
fn import_equalizer(value: Option<&Value>, store: &Storage) -> bool {
    if store.root().join("equalizer.json").exists() {
        return false;
    }
    let Some(state) = value
        .filter(|v| v.is_object())
        .and_then(|v| crate::equalizer::hydrate(v, false).ok())
    else {
        return false;
    };
    store.save("equalizer.json", &state).is_ok()
}

/// The original's `audioEffects` store (`surround`, `balance`) as `audio-effects.json`.
fn import_audio_effects(value: Option<&Value>, store: &Storage) -> bool {
    if store.root().join("audio-effects.json").exists() {
        return false;
    }
    let Some(effects) = value.filter(|v| v.is_object()) else {
        return false;
    };
    let mut out = serde_json::Map::new();
    if let Some(surround) = effects.get("surround") {
        if let Some(enabled) = surround.get("enabled").and_then(Value::as_bool) {
            out.insert("surround_enabled".into(), enabled.into());
        }
        if let Some(mode) = surround
            .get("mode")
            .and_then(Value::as_str)
            .filter(|m| matches!(*m, "off" | "small" | "medium" | "large"))
        {
            out.insert("surround_mode".into(), mode.into());
        }
    }
    if let Some(balance) = effects.get("balance") {
        if let Some(enabled) = balance.get("enabled").and_then(Value::as_bool) {
            out.insert("balance_enabled".into(), enabled.into());
        }
        if let Some(value) = balance
            .get("value")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
        {
            out.insert("balance".into(), value.clamp(-1., 1.).into());
        }
    }
    if out.is_empty() {
        return false;
    }
    store
        .save("audio-effects.json", &Value::Object(out))
        .is_ok()
}

/// The original's `local-music.db` (its scanned directories and tracks, with their tags) as this
/// application's local music index, only when it has none yet. Returns the tracks taken over;
/// files that are gone are left out and a rescan finds anything else.
fn import_local_music(database: &Path, store: &Storage, staging: &Path) -> usize {
    use crate::local_music::{Library, legacy_track};
    const MAX_TRACKS: usize = 100_000;
    if !database.is_file() || store.root().join("local-music.json").exists() {
        return 0;
    }
    let Some(copy) = temp_copy(database, staging) else {
        return 0;
    };
    let Ok(connection) =
        rusqlite::Connection::open_with_flags(&copy, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return 0;
    };
    let directories: Vec<PathBuf> = connection
        .prepare("SELECT path FROM dirs")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .collect();
    let mut tracks = Vec::new();
    if let Ok(mut statement) = connection.prepare(
        "SELECT path, COALESCE(name,''), COALESCE(singer,''), COALESCE(albumName,''), \
         COALESCE(duration,0), COALESCE(year,0) FROM tracks",
    ) && let Ok(rows) = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    }) {
        for (path, title, artist, album, duration, year) in rows.flatten() {
            if tracks.len() >= MAX_TRACKS {
                break;
            }
            if let Some(track) = legacy_track(
                Path::new(&path),
                &title,
                &artist,
                &album,
                duration.max(0.).round() as u64,
                u32::try_from(year).unwrap_or(0),
            ) {
                tracks.push(track);
            }
        }
    }
    if tracks.is_empty() && directories.is_empty() {
        return 0;
    }
    let count = tracks.len();
    match Library::from_legacy(directories, tracks).save(store) {
        Ok(()) => count,
        Err(_) => 0,
    }
}

/// Adds what `source` holds to this application's data in `store`.
pub fn import(store: &Storage, source: &Source) -> Result<Report, String> {
    if !source.found() {
        return Err("未找到旧版数据".into());
    }
    let mut report = Report::default();
    let staging = tempfile::tempdir().map_err(|_| "无法创建临时目录".to_string())?;

    if let Some(dir) = &source.data_dir {
        // Playlists: read from a copy of the database, merged without touching what exists.
        let database = dir.join("playlists.db");
        let mut kv = None;
        if database.is_file()
            && let Some(copy) = temp_copy(&database, staging.path())
        {
            let backup =
                crate::library::export_backup(&copy).map_err(|_| "读取旧版歌单失败".to_string())?;
            let (seen, _) = crate::library::check_backup(&backup).unwrap_or((0, 0));
            let library = store.library_path();
            let before = crate::library::list_playlists(&library)
                .map(|list| list.len())
                .unwrap_or(0);
            if seen > 0 {
                crate::library::restore_backup(&library, &backup, false)
                    .map_err(|_| "导入歌单失败".to_string())?;
            }
            let after = crate::library::list_playlists(&library)
                .map(|list| list.len())
                .unwrap_or(before);
            report.playlists_seen = seen;
            report.playlists_added = after.saturating_sub(before);
            kv = rusqlite::Connection::open_with_flags(
                &copy,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .ok();
        }
        // Hotkeys and folders live in the database's `kv_store`.
        if let Some(kv) = &kv {
            if !store.root().join("hotkeys.json").exists()
                && let Some(config) = kv_value(kv, "hotkeys")
                    .and_then(|json| serde_json::from_str::<crate::hotkeys::Config>(&json).ok())
            {
                report.hotkeys = store.save("hotkeys.json", &config).is_ok();
            }
            if !store.root().join("directories.json").exists()
                && let Some(folders) = kv_value(kv, "directories")
                    .and_then(|json| serde_json::from_str::<Directories>(&json).ok())
                    .filter(|d| !d.cache_dir.is_empty() && !d.download_dir.is_empty())
            {
                report.directories = store.save("directories.json", &folders).is_ok();
            }
        }
        // Merge the original plugin folder, newest first; existing registry entries stay intact.
        {
            let mut files: Vec<(std::time::SystemTime, PathBuf)> =
                fs::read_dir(dir.join("plugins"))
                    .map(|entries| {
                        entries
                            .flatten()
                            .filter(|entry| {
                                let name = entry.file_name().to_string_lossy().into_owned();
                                entry.path().is_file()
                                    && !name.ends_with(".config.json")
                                    && name.contains('-')
                            })
                            .map(|entry| {
                                let modified = entry
                                    .metadata()
                                    .and_then(|m| m.modified())
                                    .unwrap_or(std::time::UNIX_EPOCH);
                                (modified, entry.path())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            files.sort_by(|a, b| b.0.cmp(&a.0));
            let codes = files.iter().filter_map(|(_, path)| {
                let mut code = String::new();
                fs::File::open(path)
                    .ok()?
                    .take(1024 * 1024 + 1)
                    .read_to_string(&mut code)
                    .ok()?;
                (code.len() <= 1024 * 1024).then_some(code)
            });
            report.plugin = backup::install_usable(store, codes)? > 0;
        }
        report.local_tracks =
            import_local_music(&dir.join("local-music.db"), store, staging.path());
        report.downloads =
            copy_json_if_missing(&dir.join("downloads.json"), store, "downloads.json");
        report.desktop_lyric = copy_json_if_missing(
            &dir.join("desktop_lyric_window.json"),
            store,
            "desktop_lyric_window.json",
        );
    }

    if let Some(file) = &source.local_storage {
        let items = read_items(file);
        let parse = |key: &str| {
            items
                .get(key)
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
        };
        report.equalizer = import_equalizer(parse("equalizer").as_ref(), store);
        report.audio_effects = import_audio_effects(parse("audioEffects").as_ref(), store);
        let app_settings = parse("appSettings").filter(Value::is_object);
        let play = parse("playSetting");
        let user = parse("userInfo");
        if app_settings.is_some() || play.is_some() || user.is_some() {
            let directories = Directories::load(Some(store));
            let mut preferences = store
                .load::<Preferences>("preferences.json")
                .ok()
                .flatten()
                .unwrap_or_default();
            let mut changed = false;
            if let Some(settings) = &app_settings {
                backup::apply_settings(&mut preferences, &directories, settings, Mode::Import);
                changed = true;
            }
            changed |= apply_stores(&mut preferences, play.as_ref(), user.as_ref());
            if changed {
                store
                    .save("preferences.json", &preferences)
                    .map_err(|_| "保存设置失败".to_string())?;
                report.settings = true;
            }
        }
    }
    Ok(report)
}
