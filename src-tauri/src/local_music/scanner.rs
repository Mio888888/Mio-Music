use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::{Accessor, ItemKey, ItemValue};
use std::path::{Path, PathBuf};
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

fn read_sidecar_lrc(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    let dir = path.parent()?;
    let lrc_path = dir.join(format!("{}.lrc", stem));
    let bytes = std::fs::read(&lrc_path).ok().filter(|b| !b.is_empty())?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn read_lyrics_from_file(path: &Path) -> Option<String> {
    if let Ok(tagged_file) = lofty::read_from_path(path) {
        if let Some(lrc) = tagged_file.primary_tag()
            .and_then(|t| t.get_string(&ItemKey::Lyrics).map(|s| s.to_owned()))
            .filter(|s: &String| !s.trim().is_empty())
        {
            return Some(lrc);
        }
    }
    read_sidecar_lrc(path)
}

fn read_tags(path: &Path) -> Option<TrackRow> {
    let tagged_file = lofty::read_from_path(path).ok()?;
    let properties = tagged_file.properties();
    let tag = tagged_file.primary_tag();

    let file_name = path.file_stem()?.to_string_lossy().to_string();
    let (singer, name, album_name, year, has_cover) = match tag {
        Some(t) => (
            t.artist().map(|s| s.to_string()).unwrap_or_default(),
            t.title().map(|s| s.to_string()).unwrap_or_else(|| file_name.clone()),
            t.album().map(|s| s.to_string()).unwrap_or_default(),
            t.year().unwrap_or(0) as i64,
            t.pictures().first().is_some(),
        ),
        None => (String::new(), file_name.clone(), String::new(), 0, false),
    };
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

    let songmid = format!("local_{}", md5_hash(path.to_string_lossy().as_bytes()));

    // Read lyrics from tag, fall back to sidecar .lrc file
    let lrc = tag
        .and_then(|t| t.get_string(&ItemKey::Lyrics).map(|s| s.to_owned()))
        .filter(|s: &String| !s.trim().is_empty())
        .or_else(|| read_sidecar_lrc(path));

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
        lrc,
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

/// Write tags to a downloaded file with full options
pub fn write_download_tags(
    path: &Path,
    song_info: &serde_json::Value,
    tag_options: &serde_json::Value,
    cover_data: Option<&[u8]>,
    lyrics: Option<&str>,
) -> Result<(), String> {
    let mut tagged_file = lofty::read_from_path(path).map_err(|e| e.to_string())?;
    let tag = tagged_file.primary_tag_mut().ok_or("No tag found")?;

    let basic = tag_options.get("basicInfo").and_then(|v| v.as_bool()).unwrap_or(true);
    if basic {
        if let Some(name) = song_info.get("name").and_then(|v| v.as_str()) {
            tag.set_title(name.to_string().into());
        }
        if let Some(singer) = song_info.get("singer").and_then(|v| v.as_str()) {
            tag.set_artist(singer.to_string().into());
        }
        if let Some(album) = song_info.get("albumName").and_then(|v| v.as_str()) {
            tag.set_album(album.to_string().into());
        }
    }

    let write_cover = tag_options.get("cover").and_then(|v| v.as_bool()).unwrap_or(false);
    if write_cover {
        if let Some(data) = cover_data {
            let pic = lofty::picture::Picture::new_unchecked(
                lofty::picture::PictureType::CoverFront,
                Some(lofty::picture::MimeType::Jpeg),
                None,
                data.to_vec(),
            );
            tag.push_picture(pic);
        }
    }

    let write_lyrics = tag_options.get("lyrics").and_then(|v| v.as_bool()).unwrap_or(false);
    if write_lyrics {
        if let Some(lrc) = lyrics {
            if !lrc.is_empty() {
                tag.push(lofty::tag::TagItem::new(
                    ItemKey::Lyrics,
                    ItemValue::Text(lrc.to_string()),
                ));
            }
        }
    }

    tagged_file.save_to_path(path, WriteOptions::default()).map_err(|e| e.to_string())?;

    // Save separate LRC file if option enabled
    let download_lrc = tag_options.get("downloadLyrics").and_then(|v| v.as_bool()).unwrap_or(false);
    if download_lrc {
        if let Some(lrc) = lyrics {
            if !lrc.is_empty() {
                let lrc_path = path.with_extension("lrc");
                std::fs::write(&lrc_path, lrc).map_err(|e| format!("保存歌词文件失败: {}", e))?;
            }
        }
    }

    Ok(())
}

pub fn convert_to_flac(path: &Path) -> Result<PathBuf, String> {
    // Check if already FLAC by magic bytes ("fLaC" = 0x664C6143)
    let mut header = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open(path) {
        use std::io::Read;
        let _ = f.read(&mut header);
    }
    if &header[0..4] == b"fLaC" {
        return Ok(path.to_path_buf());
    }

    // Decode to PCM using symphonia
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| format!("打开文件失败: {}", e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    // Detect format from magic bytes, not extension
    if &header[0..3] == b"ID3" || (header[0] == 0xFF && (header[1] & 0xE0) == 0xE0) {
        hint.with_extension("mp3");
    } else if &header[0..4] == b"OggS" {
        hint.with_extension("ogg");
    } else if &header[4..8] == b"ftyp" {
        hint.with_extension("m4a");
    } else if &header[0..4] == b"RIFF" {
        hint.with_extension("wav");
    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("探测格式失败: {}", e))?;
    let mut format_reader = probed.format;

    let track = format_reader.default_track()
        .ok_or("未找到音频轨道")?
        .to_owned();
    let track_id = track.id;

    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100) as usize;
    let bits_per_sample = track.codec_params.bits_per_sample.unwrap_or(16) as usize;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("创建解码器失败: {}", e))?;

    let mut all_samples: Vec<i32> = Vec::new();

    loop {
        let packet = match format_reader.next_packet() {
            Ok(p) if p.track_id() == track_id => p,
            Ok(_) => continue,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => break,
        };

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let mut buf = SampleBuffer::<i32>::new(decoded.capacity() as u64, spec);
                buf.copy_interleaved_ref(decoded);
                all_samples.extend_from_slice(buf.samples());
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(_) => break,
        }
    }

    if all_samples.is_empty() {
        return Err("解码后无音频数据".to_string());
    }

    // Encode to FLAC using flacenc
    use flacenc::encode_with_fixed_block_size;
    use flacenc::config::Encoder;
    use flacenc::error::Verify;
    use flacenc::source::MemSource;
    use flacenc::component::BitRepr;
    use flacenc::bitsink::ByteSink;

    // Clamp samples to valid range for bits_per_sample
    let max_val = (1i32 << (bits_per_sample - 1)) - 1;
    let min_val = -(1i32 << (bits_per_sample - 1));
    let clamped_samples: Vec<i32> = all_samples.iter()
        .map(|&s| s.clamp(min_val, max_val))
        .collect();

    let source = MemSource::from_samples(&clamped_samples, channels, bits_per_sample, sample_rate);
    let mut config = Encoder::default();
    config.multithread = false;
    let verified_config = config.into_verified().map_err(|e| format!("编码器配置错误: {:?}", e))?;
    let stream = encode_with_fixed_block_size(&verified_config, source, 4096)
        .map_err(|e| format!("FLAC编码失败: {:?}", e))?;

    let mut sink = ByteSink::new();
    stream.write(&mut sink).map_err(|e| format!("写入FLAC数据失败: {}", e))?;

    let flac_path = path.with_extension("flac");
    std::fs::write(&flac_path, sink.as_slice())
        .map_err(|e| format!("写入FLAC文件失败: {}", e))?;

    // Remove original file
    if flac_path != path {
        let _ = std::fs::remove_file(path);
    }

    Ok(flac_path)
}
