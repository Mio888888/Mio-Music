//! The played-song cache of the original (`player/cache.rs`): online audio is copied into
//! `<cache dir>/audio/<key>` once it has been fully downloaded, and found there the next time.
//! The least recently used files go first when the folder grows past the limit.
//!
//! The setting is process-wide: the settings page writes it, the transfer task reads it.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `None` while "自动缓存音乐" is off.
static CONFIG: Mutex<Option<(PathBuf, u64)>> = Mutex::new(None);

/// Cache directory and size limit in bytes, or `None` to turn caching off
/// (`player__set_cache_config`).
pub fn configure(config: Option<(PathBuf, u64)>) {
    if let Ok(mut current) = CONFIG.lock() {
        *current = config;
    }
}

/// Acquire the configuration lock before persistence, then publish only on success.
/// No cache scan, deletion or eviction is performed by this operation.
pub(crate) fn configure_when(
    config: Option<(PathBuf, u64)>,
    commit: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let mut current = CONFIG
        .lock()
        .map_err(|_| "cache_configuration_unavailable")?;
    commit()?;
    *current = config;
    Ok(())
}

fn config() -> Option<(PathBuf, u64)> {
    CONFIG.lock().ok().and_then(|c| c.clone())
}

fn safe_path(cache_dir: &Path, key: &str) -> PathBuf {
    let safe: String = key
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    cache_dir.join("audio").join(safe)
}

/// The cached file of `key`, if caching is on and it exists; touches its modification time so
/// recently played songs stay.
pub fn lookup(key: &str) -> Option<PathBuf> {
    let (dir, _) = config()?;
    let path = safe_path(&dir, key);
    let meta = fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() == 0 {
        return None;
    }
    if let Ok(file) = fs::File::options().write(true).open(&path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
    Some(path)
}

/// Copies a finished download into the cache and evicts down to the limit.
pub fn insert(key: &str, source: &Path) -> Result<(), String> {
    let Some((dir, max)) = config() else {
        return Ok(());
    };
    let audio = dir.join("audio");
    fs::create_dir_all(&audio).map_err(|e| format!("创建缓存目录失败: {e}"))?;
    let destination = safe_path(&dir, key);
    // Written next to the destination and renamed, so a reader never sees half a file.
    let partial = tempfile::Builder::new()
        .prefix(".part-")
        .tempfile_in(&audio)
        .map_err(|e| format!("缓存写入失败: {e}"))?;
    fs::copy(source, partial.path()).map_err(|e| format!("缓存写入失败: {e}"))?;
    partial
        .persist(&destination)
        .map_err(|e| format!("缓存写入失败: {e}"))?;
    if max > 0 {
        evict(&audio, max);
    }
    Ok(())
}

pub(crate) fn evict(audio: &Path, max: u64) {
    let Ok(read) = fs::read_dir(audio) else {
        return;
    };
    let checkpoints = audio
        .file_name()
        .is_some_and(|name| name == "partial-audio");
    let mut entries = Vec::new();
    let mut total = 0u64;
    for entry in read.flatten() {
        // Another transfer's private temporary file must survive until its rename.
        if entry.file_name().to_string_lossy().starts_with('.')
            || (checkpoints
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json"))
        {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total += meta.len();
                entries.push((
                    entry.path(),
                    meta.len(),
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                ));
            }
        }
    }
    if total <= max {
        return;
    }
    entries.sort_by_key(|(_, _, modified)| *modified);
    for (path, size, _) in entries {
        if total <= max {
            break;
        }
        // Active checkpoints stay locked across writes and survive eviction.
        let lock_path = if checkpoints {
            audio.join(format!(
                ".lock-{}",
                path.file_name().unwrap().to_string_lossy()
            ))
        } else {
            path.clone()
        };
        let Ok(file) = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(checkpoints)
            .truncate(false)
            .open(&lock_path)
        else {
            continue;
        };
        if file.try_lock().is_err() {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            if checkpoints {
                let _ = fs::remove_file(path.with_extension("json"));
            }
            total -= size;
        }
    }
}

/// Number of files and their size below `dir` (`get_cache_info`).
pub fn info(dir: &Path) -> (u64, u64) {
    fn walk(path: &Path, count: &mut u64, size: &mut u64) {
        let Ok(read) = fs::read_dir(path) else {
            return;
        };
        for entry in read.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                walk(&entry.path(), count, size);
            } else if meta.is_file() {
                *count += 1;
                *size += meta.len();
            }
        }
    }
    let (mut count, mut size) = (0, 0);
    walk(dir, &mut count, &mut size);
    (count, size)
}

/// Size of everything below `path` (`get_directory_size`).
pub fn directory_size(path: &Path) -> u64 {
    info(path).1
}

/// Removes everything inside `dir` (`clear_cache`).
pub fn clear(dir: &Path) -> Result<(), String> {
    let Ok(read) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in read {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() && path.file_name().is_some_and(|name| name == "partial-audio") {
            // Keep stable lock inodes; active checkpoints cannot be deleted underneath a writer.
            evict(&path, 0);
        } else if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// A locked checkpoint is never a complete cache hit. If another transfer owns it,
/// playback still works but does not write a competing checkpoint.
pub struct Checkpoint {
    file: fs::File,
    lock: Option<fs::File>,
    budget: u64,
    pub path: PathBuf,
    metadata: PathBuf,
    pub validator: Option<String>,
}
impl Checkpoint {
    pub fn file(&self) -> &fs::File {
        &self.file
    }
    pub fn reset(&mut self, validator: Option<String>) -> std::io::Result<()> {
        self.file().set_len(0)?;
        self.validator = validator;
        let temporary = tempfile::NamedTempFile::new_in(self.path.parent().unwrap())?;
        fs::write(temporary.path(), serde_json::to_vec(&self.validator)?)?;
        temporary.persist(&self.metadata).map_err(|e| e.error)?;
        Ok(())
    }
    pub fn remove(self) {
        let _ = fs::remove_file(&self.metadata);
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for Checkpoint {
    fn drop(&mut self) {
        drop(self.lock.take());
        evict(self.path.parent().unwrap(), self.budget);
    }
}

pub fn checkpoint(key: &str) -> Option<Checkpoint> {
    let (dir, max) = config()?;
    let directory = dir.join("partial-audio");
    fs::create_dir_all(&directory).ok()?;
    // Incomplete transfers have their own bounded budget and cannot evict playable files.
    evict(&directory, if max == 0 { 256 * 1024 * 1024 } else { max });
    let filename = safe_path(&dir, key).file_name()?.to_owned();
    let path = directory.join(&filename);
    // Stable lock inode: deleting data must not let another writer bypass its owner.
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join(format!(".lock-{}", filename.to_string_lossy())))
        .ok()?;
    lock.try_lock().ok()?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .ok()?;
    let metadata = path.with_extension("json");
    let validator: Option<String> = fs::read(&metadata)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .flatten();
    let mut checkpoint = Checkpoint {
        file,
        lock: Some(lock),
        budget: if max == 0 { 256 * 1024 * 1024 } else { max },
        path,
        metadata,
        validator,
    };
    if checkpoint.validator.is_none()
        || checkpoint.file().metadata().ok()?.len() > 256 * 1024 * 1024
    {
        checkpoint.reset(None).ok()?;
    }
    Some(checkpoint)
}
