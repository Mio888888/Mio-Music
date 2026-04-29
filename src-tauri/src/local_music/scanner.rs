use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::Accessor;
use std::path::Path;
use std::time::SystemTime;
use walkdir::WalkDir;
use crate::db::music_db::{self, TrackRow, TrackStat};

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "ogg", "m4a", "aac", "wma", "opus", "ape", "alac"];

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn read_tags(path: &Path) -> Option<TrackRow> {
    let tagged_file = lofty::read_from_path(path).ok()?;
    let properties = tagged_file.properties();
    let tag = tagged_file.primary_tag()?;

    let file_name = path.file_stem()?.to_string_lossy().to_string();
    let singer = tag.artist().map(|s| s.to_string()).unwrap_or_default();
    let name = tag.title().map(|s| s.to_string()).unwrap_or_else(|| file_name.clone());
    let album_name = tag.album().map(|s| s.to_string()).unwrap_or_default();
    let year = tag.year().unwrap_or(0) as i64;
    let duration = properties.duration().as_secs_f64();
    let bitrate = properties.audio_bitrate().unwrap_or(0) as i64;
    let sample_rate = properties.sample_rate().unwrap_or(0) as i64;
    let channels = properties.channels().unwrap_or(0) as i64;

    let mtime_ms = path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let file_size = path.metadata().map(|m| m.len() as i64).unwrap_or(0);
    let has_cover = tag.pictures().first().is_some();

    // Generate a unique songmid from path hash
    let songmid = format!("local_{}", md5_hash(path.to_string_lossy().as_bytes()));

    Some(TrackRow {
        songmid,
        path: path.to_string_lossy().to_string(),
        url: None,
        singer,
        name,
        album_name,
        album_id: 0,
        source: "local".to_string(),
        interval: format_duration(duration),
        has_cover: if has_cover { 1 } else { 0 },
        cover_key: None,
        year,
        lrc: None,
        types: "[]".to_string(),
        _types: "{}".to_string(),
        type_url: "{}".to_string(),
        bitrate,
        sample_rate,
        channels,
        duration,
        size: file_size,
        mtime_ms,
        hash: None,
        updated_at: chrono::Utc::now().timestamp(),
    })
}

fn md5_hash(data: &[u8]) -> String {
    // Simple hash for generating unique IDs (not cryptographic)
    let mut hash: u64 = 5381;
    for &byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    format!("{:016x}", hash)
}

fn format_duration(secs: f64) -> String {
    let total_secs = secs as u64;
    let min = total_secs / 60;
    let sec = total_secs % 60;
    format!("{:02}:{:02}", min, sec)
}

pub struct ScanResult {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub errors: usize,
}

pub fn scan_directories(conn: &rusqlite::Connection, dirs: &[String]) -> ScanResult {
    let mut result = ScanResult { scanned: 0, added: 0, updated: 0, errors: 0 };

    // Get existing stats for incremental scan
    let existing_stats: Vec<TrackStat> = music_db::get_all_stats(conn).unwrap_or_default();
    let stat_map: std::collections::HashMap<String, TrackStat> = existing_stats
        .into_iter()
        .map(|s| (s.path.clone(), s))
        .collect();

    let mut new_tracks: Vec<TrackRow> = Vec::new();
    let mut found_paths: Vec<String> = Vec::new();

    for dir in dirs {
        for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || !is_audio_file(path) { continue; }

            result.scanned += 1;
            let path_str = path.to_string_lossy().to_string();
            found_paths.push(path_str.clone());

            // Check mtime for incremental update
            let mtime_ms = path.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            if let Some(existing) = stat_map.get(&path_str) {
                if existing.mtime_ms == mtime_ms { continue; } // Unchanged
            }

            match read_tags(path) {
                Some(track) => new_tracks.push(track),
                None => result.errors += 1,
            }
        }
    }

    // Batch upsert new/changed tracks
    if !new_tracks.is_empty() {
        result.added = new_tracks.len();
        let _ = music_db::upsert_tracks(conn, &new_tracks);
    }

    // Prune tracks that are no longer in scan directories
    let _ = music_db::prune_outside_keep(conn, &found_paths);

    result
}

pub fn get_cover_data(path: &Path) -> Option<Vec<u8>> {
    let tagged_file = lofty::read_from_path(path).ok()?;
    let tag = tagged_file.primary_tag()?;
    let picture = tag.pictures().first()?;
    Some(picture.data().to_vec())
}

pub fn write_tags(path: &Path, info: &TrackRow) -> Result<(), String> {
    let mut tagged_file = lofty::read_from_path(path).map_err(|e| e.to_string())?;
    let tag = tagged_file.primary_tag_mut().ok_or("No tag found")?;

    tag.set_title(info.name.clone().into());
    tag.set_artist(info.singer.clone().into());
    tag.set_album(info.album_name.clone().into());
    if info.year > 0 {
        tag.set_year(info.year as u32);
    }

    tagged_file.save_to_path(path, WriteOptions::default()).map_err(|e| e.to_string())
}
