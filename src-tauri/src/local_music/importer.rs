use std::io::Read;
use std::path::{Path, PathBuf};
use lofty::file::{FileType, TaggedFileExt};

/// Copy an authorized picker stream into persistent storage before indexing it.
pub fn import_audio(mut input: impl Read, display_name: &str, root: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    // Each selection owns a directory; equal names never overwrite an existing song.
    let directory = root.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let result = (|| {
        let staging = directory.join("audio.part");
        let mut output = std::fs::OpenOptions::new().write(true).create_new(true)
            .open(&staging).map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        // Providers may return an opaque URI. Detect actual bytes, not its extension.
        let tagged = lofty::probe::Probe::open(&staging)
            .map_err(|e| e.to_string())?.guess_file_type()
            .map_err(|e| e.to_string())?
            .read().map_err(|_| "无法识别音频内容或文件已损坏".to_string())?;
        let extension = match tagged.file_type() {
            FileType::Aac => "aac", FileType::Ape => "ape", FileType::Flac => "flac",
            FileType::Mpeg => "mp3", FileType::Mp4 => "m4a", FileType::Opus => "opus",
            FileType::Vorbis => "ogg", FileType::Wav => "wav",
            _ => return Err("暂不支持此音频格式".to_string()),
        };
        let basename = display_name.rsplit(['/', '\\']).next().unwrap_or("");
        let stem = Path::new(basename).file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let safe_name: String = stem.chars().filter(|c| !c.is_control() && !"<>:\"|?*".contains(*c)).take(60).collect();
        let safe_name = safe_name.trim_matches(['.', ' ']);
        let filename = if safe_name.is_empty() { "音频" } else { safe_name };
        let destination = directory.join(format!("{filename}.{extension}"));
        std::fs::rename(staging, &destination).map_err(|e| e.to_string())?;
        Ok(destination)
    })();
    if result.is_err() {
        // Only remove this attempt's freshly created directory.
        let _ = std::fs::remove_dir_all(directory);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("mio-import-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }
    fn wav() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(40u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes()); // PCM
        bytes.extend(1u16.to_le_bytes()); // mono
        bytes.extend(8000u32.to_le_bytes());
        bytes.extend(16000u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(4u32.to_le_bytes());
        bytes.extend([0; 4]);
        bytes
    }

    #[test]
    fn imports_stream_without_filename_and_preserves_same_name_files() {
        let root = TestDir::new();
        let data = wav();
        let a = import_audio(Cursor::new(&data), "../../song.mp3", &root.0).unwrap();
        let b = import_audio(Cursor::new(&data), "../../song.mp3", &root.0).unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with(&root.0));
        assert_eq!(a.file_name().unwrap(), "song.wav");
        assert_eq!(std::fs::read(a).unwrap(), data);
        let unnamed = import_audio(Cursor::new(&data), "", &root.0).unwrap();
        assert_eq!(unnamed.extension().unwrap(), "wav");
        assert_eq!(std::fs::read(b).unwrap(), data);
    }

    #[test]
    fn rejects_non_audio_and_removes_partial_files() {
        let root = TestDir::new();
        assert!(import_audio(Cursor::new(b"not audio"), "fake.mp3", &root.0).is_err());
        struct Broken(bool);
        impl Read for Broken {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if self.0 { return Err(io::Error::other("read failed")); }
                self.0 = true;
                buffer[..4].copy_from_slice(b"RIFF");
                Ok(4)
            }
        }
        assert!(import_audio(Broken(false), "broken.mp3", &root.0).is_err());
        assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
    }

    #[test]
    fn copied_music_can_be_reindexed_with_a_stable_playable_path() {
        use crate::db::music_db;
        use crate::local_music::scanner;
        let root = TestDir::new();
        let path = import_audio(Cursor::new(wav()), "song.wav", &root.0).unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        music_db::init_tables(&conn).unwrap();
        let dirs = vec![root.0.to_string_lossy().into_owned()];
        let first = scanner::scan_directories(&conn, &dirs, false);
        assert_eq!((first.added, first.errors), (1, 0));
        let id = scanner::read_file_tags(&path).unwrap().songmid;
        let track = music_db::get_track_by_id(&conn, &id).unwrap().unwrap();
        assert_eq!(track.name, "song");
        assert_eq!(std::fs::read(&track.path).unwrap(), wav());
        music_db::clear_tracks(&conn).unwrap();
        let restored = scanner::scan_directories(&conn, &dirs, false);
        assert_eq!((restored.added, restored.errors), (1, 0));
        assert_eq!(music_db::get_track_by_id(&conn, &id).unwrap().unwrap().path, track.path);
    }
}
