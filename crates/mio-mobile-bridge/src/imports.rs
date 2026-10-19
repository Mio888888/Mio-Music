//! Private native staging -> validated, owned copies -> original owner commit.
use crate::runtime;
use mio_core::local_music::PreparedImport;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

const OWNER: &str = "mio-audio-staging-v1";
const EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "m4a", "aac", "wma", "opus", "ape", "alac",
];
static HANDLES: AtomicUsize = AtomicUsize::new(0);
static PREPARING: AtomicBool = AtomicBool::new(false);
pub(crate) struct Worker;
impl Worker {
    pub(crate) fn acquire() -> Result<Self, String> {
        PREPARING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "busy: audio staging in use".into())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        PREPARING.store(false, Ordering::Release);
    }
}

pub(crate) struct Session {
    data: PathBuf,
    cache: PathBuf,
    lease: runtime::RuntimeLease,
    cancelled: Arc<AtomicBool>,
    outcome: Mutex<Option<(String, Result<u32, String>)>>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.cancel();
        HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
}
/// Startup only. A replacement owner must not delete copies an old worker is
/// still preparing; failed/busy recovery preserves them for the next startup.
pub(crate) fn recover(core: &mio_core::app_core::AppCore) {
    if let Ok(_worker) = Worker::acquire() {
        let _ = core.recover_local_imports();
    }
}

pub(crate) fn begin() -> Result<Session, String> {
    HANDLES
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < 4).then_some(n + 1)
        })
        .map_err(|_| "busy: too many audio import sessions")?;
    match runtime::inspect_with_lease(|core| (core.paths.data.clone(), core.paths.cache.clone())) {
        Ok(((data, cache), lease)) => Ok(Session {
            data,
            cache,
            lease,
            cancelled: Arc::new(AtomicBool::new(false)),
            outcome: Mutex::new(None),
        }),
        Err(error) => {
            HANDLES.fetch_sub(1, Ordering::AcqRel);
            Err(error)
        }
    }
}
impl Session {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()
    }
    pub(crate) fn commit(&self, ticket: String) -> Result<u32, String> {
        let mut outcome = self
            .outcome
            .try_lock()
            .map_err(|_| "busy: import in progress")?;
        if let Some((previous, result)) = outcome.as_ref() {
            return if previous == &ticket {
                result.clone()
            } else {
                Err("invalid_input: reopen audio import".into())
            };
        }
        self.check()?;
        let _worker = Worker::acquire()?;
        let result = (|| {
            let stage = Stage::open(&self.cache, &ticket)?;
            let paths = stage.files()?;
            let count = paths.len() as u32;
            self.check()?;
            let prepared =
                PreparedImport::prepare_named(&paths, &self.data, || self.check().is_err())
                    .map_err(|_| {
                        if self.check().is_err() {
                            "cancelled"
                        } else {
                            "audio_import_failed: cannot prepare playable audio"
                        }
                        .to_owned()
                    })?;
            self.check()?;
            // Once admitted, execute returns the actual durable outcome even if
            // Flutter leaves or the runtime is closed while saving the index.
            self.lease.execute(runtime::Action::ImportLocal(
                prepared,
                self.cancelled.clone(),
            ))?;
            Ok(count)
        })();
        *outcome = Some((ticket, result.clone()));
        result
    }
}
fn regular(path: &Path, limit: u64) -> Result<fs::Metadata, String> {
    let info = fs::symlink_metadata(path).map_err(|_| "audio_staging_unavailable")?;
    if !info.is_file() || info.len() > limit {
        return Err("audio_staging_invalid".into());
    }
    Ok(info)
}
fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    regular(path, limit)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| "audio_staging_unavailable")?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "audio_staging_unavailable")?;
    if bytes.len() as u64 > limit {
        return Err("audio_staging_invalid".into());
    }
    Ok(bytes)
}
fn staged_name(name: &str) -> bool {
    let Some((index, extension)) = name.split_once('.') else {
        return false;
    };
    !index.is_empty()
        && index.len() <= 2
        && index.bytes().all(|b| b.is_ascii_digit())
        && EXTENSIONS.contains(&extension)
}
pub(crate) struct Stage(PathBuf);
impl Stage {
    pub(crate) fn open(cache: &Path, ticket: &str) -> Result<Self, String> {
        if ticket.len() != 65
            || !ticket.bytes().enumerate().all(|(i, b)| {
                if i == 32 {
                    b == b'-'
                } else {
                    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
                }
            })
        {
            return Err("invalid_input: invalid audio selection".into());
        }
        let root = cache.join("mio-audio-staging");
        let path = root.join(ticket);
        for directory in [&root, &path] {
            if !fs::symlink_metadata(directory)
                .map_err(|_| "audio_staging_unavailable")?
                .is_dir()
            {
                return Err("audio_staging_invalid".into());
            }
        }
        if bounded(&path.join(".owner"), 64)? != OWNER.as_bytes() {
            return Err("audio_staging_invalid".into());
        }
        Ok(Self(path))
    }
    pub(crate) fn files(&self) -> Result<Vec<(PathBuf, String)>, String> {
        let value: serde_json::Value =
            serde_json::from_slice(&bounded(&self.0.join("manifest.json"), 64 * 1024)?)
                .map_err(|_| "audio_staging_invalid")?;
        if value["version"].as_u64() != Some(1) {
            return Err("audio_staging_invalid".into());
        }
        let rows = value["files"].as_array().ok_or("audio_staging_invalid")?;
        if rows.is_empty() || rows.len() > 100 {
            return Err("audio_staging_invalid".into());
        }
        let mut paths = Vec::with_capacity(rows.len());
        let mut total = 0u64;
        for (index, row) in rows.iter().enumerate() {
            let relative = row["path"].as_str().ok_or("audio_staging_invalid")?;
            let name = row["name"].as_str().ok_or("audio_staging_invalid")?;
            let extension = relative
                .split_once('.')
                .map(|v| v.1)
                .ok_or("audio_staging_invalid")?;
            if !staged_name(relative)
                || relative != format!("{index}.{extension}")
                || name.is_empty()
                || name.len() > 240
                || name.contains(['\0', '/', '\\'])
                || Path::new(name)
                    .extension()
                    .and_then(|v| v.to_str())
                    .map(|v| v.to_ascii_lowercase())
                    .as_deref()
                    != Some(extension)
            {
                return Err("audio_staging_invalid".into());
            }
            let path = self.0.join(relative);
            let size = regular(&path, 256 * 1024 * 1024)?.len();
            total += size;
            if size == 0 || total > 512 * 1024 * 1024 {
                return Err("audio_staging_invalid".into());
            }
            paths.push((path, name.to_owned()));
        }
        Ok(paths)
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        // Preserve any unrecognized/non-regular entry. Never recurse through
        // paths supplied by Dart or the manifest, and delete the marker last.
        let cleanup = || -> Result<(), String> {
            if !fs::symlink_metadata(&self.0)
                .map_err(|_| "retained")?
                .is_dir()
                || bounded(&self.0.join(".owner"), 64)? != OWNER.as_bytes()
            {
                return Err("retained".into());
            }
            let mut files = Vec::new();
            for entry in fs::read_dir(&self.0).map_err(|_| "retained")? {
                let entry = entry.map_err(|_| "retained")?;
                if files.len() >= 102 {
                    return Err("retained".into());
                }
                let name = entry.file_name().into_string().map_err(|_| "retained")?;
                if name != ".owner" && name != "manifest.json" && !staged_name(&name) {
                    return Err("retained".into());
                }
                regular(&entry.path(), u64::MAX - 1)?;
                if name != ".owner" {
                    files.push(entry.path());
                }
            }
            for file in files {
                fs::remove_file(file).map_err(|_| "retained")?;
            }
            fs::remove_file(self.0.join(".owner")).map_err(|_| "retained")?;
            fs::remove_dir(&self.0).map_err(|_| "retained".to_owned())
        };
        let _ = cleanup();
    }
}
