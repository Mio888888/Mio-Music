//! Persistent image bytes, shared by list images and the restored player cover.
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, sync::Mutex};

static DIRECTORY: Mutex<Option<PathBuf>> = Mutex::new(None);
const MAX_IMAGE: usize = 16 * 1024 * 1024;

pub fn configure(root: PathBuf) {
    if let Ok(mut directory) = DIRECTORY.lock() {
        *directory = Some(root.join("images"));
    }
}

fn path(key: &str) -> Option<PathBuf> {
    let directory = DIRECTORY.lock().ok()?.clone()?;
    Some(directory.join(hex::encode(Sha256::digest(key.as_bytes()))))
}

pub fn read(key: &str) -> Option<Vec<u8>> {
    let path = path(key)?;
    let metadata = fs::metadata(&path).ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_IMAGE as u64 {
        return None;
    }
    let bytes = fs::read(&path).ok()?;
    if let Ok(file) = fs::File::options().write(true).open(path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
    Some(bytes)
}

pub fn remove(key: &str) {
    if let Some(path) = path(key) {
        let _ = fs::remove_file(path);
    }
}

pub fn write(key: &str, bytes: &[u8]) {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE || image::guess_format(bytes).is_err() {
        return;
    }
    let Some(path) = path(key) else { return };
    let directory = path.parent().unwrap();
    let save = || -> std::io::Result<()> {
        fs::create_dir_all(directory)?;
        let temporary = tempfile::Builder::new()
            .prefix(".part-")
            .tempfile_in(directory)?;
        fs::write(temporary.path(), bytes)?;
        temporary.persist(&path).map_err(|e| e.error)?;
        Ok(())
    };
    if save().is_ok() {
        crate::audio_cache::evict(directory, 128 * 1024 * 1024);
    }
}

pub fn song_key(id: &str) -> String {
    format!("song-cover:{id}")
}
