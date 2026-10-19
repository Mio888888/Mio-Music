//! Download queue for online songs: resolve a playback address through the plugin, stream it to
//! a file and report progress. Task records are kept in `downloads.json` in the original app's
//! layout (an array of `DownloadTask`), so they survive restarts.
//!
//! A transfer writes to `<name>.<ext>.<task id>.part`. Pausing, an interrupted transfer and an
//! app restart keep that file, and the next run asks the server for the rest with a `Range`
//! request (the original's `execute_download`); only cancelling deletes it.
use crate::{online::Online, tags::TagOptions};
use mio_music_sdk::MusicItem;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashSet, VecDeque},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

const MAX_BYTES: u64 = 1024 * 1024 * 1024;
/// A failed transfer is queued again this many times before it is reported (the original's
/// `task.retries < 3`).
pub const AUTO_RETRIES: u32 = 3;
/// Worker threads; how many may transfer at once is `Shared::limit`, between 1 and this.
pub const MAX_CONCURRENT: usize = 5;
/// The original's initial "同时下载数" (it is not persisted there either).
const DEFAULT_CONCURRENT: usize = 3;
/// Records kept on disk; the oldest finished ones go first.
const MAX_RECORDS: usize = 2000;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Queued,
    Resolving,
    Downloading,
    Paused,
    Done,
    Failed(String),
    Cancelled,
}
impl Status {
    /// Waiting for a transfer slot or transferring.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Queued | Self::Resolving | Self::Downloading)
    }
    /// Not finished: active, or paused and waiting to be continued.
    pub fn in_progress(&self) -> bool {
        self.is_active() || *self == Self::Paused
    }
}

#[derive(Clone, Debug)]
pub struct TaskView {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub quality: String,
    pub status: Status,
    pub received: u64,
    pub total: Option<u64>,
    pub path: Option<PathBuf>,
    /// Set when the file already existed and nothing was written.
    pub skipped: bool,
    /// A finished task whose file is no longer there (checked when records are restored).
    pub missing: bool,
    /// Why the tags could not be written, when the file itself downloaded fine.
    pub tag_error: Option<String>,
    /// Bytes per second over the last report, and seconds left at that speed; only while
    /// transferring.
    pub speed: f64,
    pub remaining: Option<f64>,
    /// Automatic restarts after a failure so far (at most [`AUTO_RETRIES`]); a manual retry
    /// starts counting again.
    pub retries: u32,
}

impl TaskView {
    fn file_exists(&self) -> bool {
        self.path.as_ref().is_some_and(|path| path.exists())
    }
}

#[derive(Debug, PartialEq)]
pub enum Enqueued {
    Added,
    /// The same song is already queued or downloading.
    Duplicate,
    /// The song was downloaded earlier and its file is still there.
    AlreadyDone,
}

struct Task {
    view: TaskView,
    item: MusicItem,
    created_at: i64,
    /// Stops the running worker; with `pausing` set it was a pause, else a cancel.
    cancel: Arc<AtomicBool>,
    pausing: Arc<AtomicBool>,
    /// Bumped whenever the task is started again or cancelled, so a worker that is still
    /// blocked in a read cannot touch the new run's state.
    generation: u64,
    /// A worker is working on this task (possibly still unwinding after a pause).
    running: bool,
}
struct Shared {
    tasks: Mutex<Vec<Task>>,
    queue: Mutex<VecDeque<String>>,
    wake: Condvar,
    /// Tasks that may be worked on at the same time (`download__set_max_concurrent`).
    limit: AtomicUsize,
    /// Tasks a worker has taken (resolving, transferring, finishing).
    active: AtomicUsize,
    closed: AtomicBool,
    online: Mutex<Option<Arc<Online>>>,
    tags: Mutex<TagOptions>,
    /// `filenameTemplate` of the settings: how a new download's file is named.
    template: Mutex<String>,
    records: Option<PathBuf>,
    /// Serializes snapshot writes so an older snapshot never lands after a newer one.
    write_lock: Mutex<()>,
    /// Where downloads are written; the settings page can change it while the app runs.
    dir: RwLock<PathBuf>,
}

impl Shared {
    fn dir(&self) -> PathBuf {
        self.dir
            .read()
            .map(|dir| dir.clone())
            .unwrap_or_else(|_| PathBuf::from("downloads"))
    }
}

pub struct Downloads {
    shared: Arc<Shared>,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
    shutdown_result: Mutex<Option<Result<(), String>>>,
}

impl Drop for Downloads {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

/// One entry of `downloads.json`; field names and status strings match the original app's
/// `DownloadTask`. Fields it does not have are prefixed `_mioGpui` and ignored there.
#[derive(Serialize, Deserialize)]
struct Record {
    id: String,
    #[serde(default)]
    song_info: Value,
    #[serde(default)]
    url: String,
    #[serde(default)]
    plugin_id: Option<String>,
    #[serde(default)]
    quality: Option<String>,
    #[serde(default)]
    file_path: String,
    status: String,
    #[serde(default)]
    progress: f64,
    #[serde(default)]
    speed: f64,
    #[serde(default)]
    total_size: u64,
    #[serde(default)]
    downloaded_size: u64,
    #[serde(default)]
    remaining_time: Option<f64>,
    #[serde(default)]
    retries: u32,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    priority: u32,
    #[serde(default)]
    created_at: i64,
    #[serde(default, rename = "_mioGpuiSkipped")]
    skipped: bool,
    #[serde(default, rename = "_mioGpuiTagError")]
    tag_error: Option<String>,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl Task {
    fn record(&self) -> Record {
        let view = &self.view;
        let (status, error) = match &view.status {
            Status::Queued => ("queued", None),
            Status::Resolving | Status::Downloading => ("downloading", None),
            Status::Paused => ("paused", None),
            Status::Done => ("completed", None),
            Status::Failed(message) => ("error", Some(message.clone())),
            Status::Cancelled => ("cancelled", None),
        };
        let progress = match (&view.status, view.total) {
            (Status::Done, _) => 100.0,
            (_, Some(total)) if total > 0 => {
                (view.received as f64 * 100.0 / total as f64).min(100.0)
            }
            _ => 0.0,
        };
        Record {
            id: view.id.clone(),
            song_info: serde_json::to_value(&self.item).unwrap_or_default(),
            url: String::new(),
            plugin_id: None,
            quality: Some(view.quality.clone()),
            file_path: view
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            status: status.into(),
            progress,
            speed: 0.0,
            total_size: view.total.unwrap_or(0),
            downloaded_size: view.received,
            remaining_time: None,
            retries: view.retries,
            error,
            priority: 0,
            created_at: self.created_at,
            skipped: view.skipped,
            tag_error: view.tag_error.clone(),
        }
    }

    /// Restores a record. A task that was running when the app stopped comes back paused, like
    /// the original's `normalize_restored_tasks`; continuing it picks up its partial file.
    fn from_record(record: Record) -> Option<Self> {
        let item: MusicItem = serde_json::from_value(record.song_info).ok()?;
        if !mio_music_sdk::is_implemented(&item.source) || record.id.trim().is_empty() {
            return None;
        }
        let path = (!record.file_path.is_empty()).then(|| PathBuf::from(&record.file_path));
        let status = match record.status.as_str() {
            "queued" => Status::Queued,
            "completed" => Status::Done,
            "cancelled" => Status::Cancelled,
            "error" => Status::Failed(
                record
                    .error
                    .filter(|e| !e.is_empty())
                    .unwrap_or_else(|| "下载失败".into()),
            ),
            // "downloading" and "paused": interrupted by the app closing.
            _ => Status::Paused,
        };
        let missing = status == Status::Done && path.as_ref().is_none_or(|p| !p.exists());
        Some(Self {
            view: TaskView {
                id: record.id,
                title: item.name.clone(),
                artist: item.singer.clone(),
                quality: record.quality.unwrap_or_default(),
                status,
                received: record.downloaded_size,
                total: (record.total_size > 0).then_some(record.total_size),
                path,
                skipped: record.skipped,
                missing,
                tag_error: record.tag_error,
                speed: 0.,
                remaining: None,
                retries: record.retries.min(AUTO_RETRIES),
            },
            item,
            created_at: record.created_at,
            cancel: Arc::new(AtomicBool::new(false)),
            pausing: Arc::new(AtomicBool::new(false)),
            generation: 0,
            running: false,
        })
    }
}

/// The original's default `filenameTemplate`.
pub const DEFAULT_TEMPLATE: &str = "%t - %s";

/// `formatMusicInfo` of the original: `%t` title, `%s` artist, `%a` album, `%u` source, `%q`
/// quality, `%d` date; anything else stays as written.
pub fn format_template(
    template: &str,
    title: &str,
    artist: &str,
    album: &str,
    source: &str,
    quality: &str,
    date: &str,
) -> String {
    let template = if template.is_empty() {
        DEFAULT_TEMPLATE
    } else {
        template
    };
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                Some('t') => out.push_str(title),
                Some('s') => out.push_str(artist),
                Some('a') => out.push_str(album),
                Some('u') => out.push_str(source),
                Some('q') => out.push_str(quality),
                Some('d') => out.push_str(date),
                _ => {
                    out.push('%');
                    continue;
                }
            }
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

/// A song's file name (without extension) from the template, made safe for every platform's
/// file names.
fn file_stem(template: &str, item: &MusicItem, quality: &str) -> String {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let raw = format_template(
        template,
        item.name.trim(),
        item.singer.trim(),
        item.album_name.trim(),
        item.source.trim(),
        quality,
        &date,
    );
    sanitize_stem(&raw)
}

pub(crate) fn sanitize_stem(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    // A field the song does not have leaves its separator dangling at an end; drop it.
    let trimmed = cleaned
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '.'))
        .trim();
    let limited: String = trimmed.chars().take(120).collect();
    if limited.is_empty() {
        "unknown".into()
    } else {
        limited
    }
}

/// System Downloads folder like the original's default; `MIO_GPUI_DOWNLOAD_DIR` overrides it.
pub fn default_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MIO_GPUI_DOWNLOAD_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    home.map(|h| PathBuf::from(h).join("Downloads"))
        .unwrap_or_else(|| PathBuf::from("downloads"))
}

fn extension(url: &str, content_type: Option<&str>, quality: &str) -> &'static str {
    const KNOWN: [&str; 9] = [
        "mp3", "flac", "m4a", "aac", "ogg", "wav", "ape", "wma", "opus",
    ];
    let path = url.split(['?', '#']).next().unwrap_or("");
    if let Some(ext) = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()) {
        if let Some(known) = KNOWN.iter().find(|k| **k == ext) {
            return known;
        }
    }
    match content_type.map(|c| c.to_ascii_lowercase()).as_deref() {
        Some(c) if c.contains("flac") => "flac",
        Some(c) if c.contains("mp4") || c.contains("m4a") => "m4a",
        Some(c) if c.contains("aac") => "aac",
        Some(c) if c.contains("ogg") => "ogg",
        Some(c) if c.contains("wav") => "wav",
        Some(c) if c.contains("mpeg") || c.contains("mp3") => "mp3",
        _ if matches!(quality, "flac" | "flac24bit" | "hires" | "master") => "flac",
        _ => "mp3",
    }
}

fn load_records(path: &Path) -> Vec<Task> {
    let mut bytes = Vec::new();
    let read =
        std::fs::File::open(path).and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes));
    if read.is_err() {
        return Vec::new();
    }
    let parsed = if bytes.len() as u64 > MAX_FILE_BYTES {
        None
    } else {
        serde_json::from_slice::<Vec<Record>>(&bytes).ok()
    };
    let Some(records) = parsed else {
        // Keep an unreadable file for inspection instead of overwriting it with a fresh list.
        let _ = std::fs::rename(path, path.with_extension("json.unreadable"));
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut tasks: Vec<Task> = records
        .into_iter()
        .filter_map(Task::from_record)
        .filter(|task| seen.insert(task.view.id.clone()))
        .collect();
    tasks.sort_by_key(|t| t.created_at);
    tasks
}

impl Downloads {
    /// `records` is where task records are kept; `None` keeps them in memory only.
    pub fn new(dir: PathBuf, records: Option<PathBuf>) -> Self {
        let mut tasks = records.as_deref().map(load_records).unwrap_or_default();
        // Records are written at status changes, not while bytes arrive; what a transfer that
        // was cut off had written is on disk, and that is where it continues from.
        for task in tasks
            .iter_mut()
            .filter(|t| matches!(t.view.status, Status::Paused | Status::Queued))
        {
            if let Some((_, size)) = find_partial(&dir, &task.view.id) {
                task.view.received = size;
            }
        }
        let queue: VecDeque<String> = tasks
            .iter()
            .filter(|t| t.view.status == Status::Queued)
            .map(|t| t.view.id.clone())
            .collect();
        let shared = Arc::new(Shared {
            tasks: Mutex::new(tasks),
            queue: Mutex::new(queue),
            wake: Condvar::new(),
            limit: AtomicUsize::new(DEFAULT_CONCURRENT),
            active: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            online: Mutex::new(None),
            tags: Mutex::new(TagOptions::default()),
            template: Mutex::new(DEFAULT_TEMPLATE.into()),
            records,
            write_lock: Mutex::new(()),
            dir: RwLock::new(dir),
        });
        let mut workers = Vec::with_capacity(MAX_CONCURRENT);
        for _ in 0..MAX_CONCURRENT {
            let shared = shared.clone();
            workers.push(std::thread::spawn(move || worker(shared)));
        }
        // Leftovers of interrupted transfers have no owner any more.
        clean_partials(&shared);
        Self {
            shared,
            workers: Mutex::new(workers),
            shutdown_result: Mutex::new(None),
        }
    }

    /// Final host shutdown: keep partial files, persist paused tasks, and wait
    /// until no worker can publish files or records into a replacement core.
    /// Page disposal must never call this process-level operation.
    pub fn shutdown(&self) -> Result<(), String> {
        let mut completed = self
            .shutdown_result
            .lock()
            .map_err(|_| "Download shutdown unavailable")?;
        if let Some(result) = completed.as_ref() {
            return result.clone();
        }
        let mut workers = self
            .workers
            .lock()
            .map_err(|_| "Download shutdown unavailable")?;
        {
            let mut tasks = self
                .shared
                .tasks
                .lock()
                .map_err(|_| "Download queue unavailable")?;
            self.shared.closed.store(true, Ordering::Release);
            for task in tasks
                .iter_mut()
                .filter(|task| task.running || task.view.status.is_active())
            {
                // Set both flags even if the worker has popped a Queued task
                // but has not yet changed its visible status to Resolving.
                task.pausing.store(true, Ordering::Relaxed);
                task.cancel.store(true, Ordering::Relaxed);
                if task.view.status.is_active() {
                    task.view.status = Status::Paused;
                }
            }
        }
        wake_all(&self.shared);
        let mut failed = false;
        for worker in workers.drain(..) {
            failed |= worker.join().is_err();
        }
        let result = persist_checked(&self.shared, true).and_then(|()| {
            if failed {
                Err("Download worker interrupted during shutdown".into())
            } else {
                Ok(())
            }
        });
        *completed = Some(result.clone());
        result
    }

    /// Sets the folder new downloads go to; running transfers keep writing where they began.
    pub fn set_dir(&self, dir: PathBuf) {
        let Ok(_tasks) = self.shared.tasks.lock() else {
            return;
        };
        if self.shared.closed.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut current) = self.shared.dir.write() {
            *current = dir;
        }
    }

    /// How many tasks may be downloaded at once.
    pub fn max_concurrent(&self) -> usize {
        self.shared.limit.load(Ordering::Relaxed)
    }

    /// A stale view cannot replace a newer concurrency setting.
    pub fn compare_max_concurrent(&self, expected: usize, next: usize) -> Result<usize, String> {
        if !(1..=MAX_CONCURRENT).contains(&expected) || !(1..=MAX_CONCURRENT).contains(&next) {
            return Err("invalid_input: download concurrency".into());
        }
        if self.shared.closed.load(Ordering::Acquire) {
            return Err("runtime_closed: download manager".into());
        }
        self.shared
            .limit
            .compare_exchange(expected, next, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "settings_changed: refresh downloads")?;
        wake_all(&self.shared);
        Ok(next)
    }

    /// Only expose a completed direct child of the original host download root.
    /// The native host independently validates the returned basename again.
    pub fn completed_file_name(&self, id: &str, root: &Path) -> Result<String, String> {
        let unavailable = || "file_unavailable: refresh completed downloads".to_owned();
        let path = {
            let tasks = self.shared.tasks.lock().map_err(|_| unavailable())?;
            if self.shared.closed.load(Ordering::Acquire) {
                return Err(unavailable());
            }
            let row = tasks
                .iter()
                .find(|t| t.view.id == id)
                .ok_or_else(unavailable)?;
            if row.view.status != Status::Done || row.view.missing {
                return Err(unavailable());
            }
            let path = row.view.path.as_ref().ok_or_else(unavailable)?;
            if path.as_os_str().len() > 4096 {
                return Err(unavailable());
            }
            path.clone()
        };
        let root_meta = std::fs::symlink_metadata(root).map_err(|_| unavailable())?;
        if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
            return Err(unavailable());
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| unavailable())?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(unavailable());
        }
        let expected = root.canonicalize().map_err(|_| unavailable())?;
        let actual = path.canonicalize().map_err(|_| unavailable())?;
        if actual.parent() != Some(expected.as_path()) {
            return Err(unavailable());
        }
        let name = actual
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(unavailable)?;
        if name.is_empty()
            || name.len() > 1024
            || name == "."
            || name == ".."
            || name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
        {
            return Err(unavailable());
        }
        Ok(name.to_owned())
    }

    /// Sets how many tasks may be downloaded at once (1 to 5). Tasks already running keep
    /// going when the number goes down; waiting ones start when the number goes up.
    pub fn set_max_concurrent(&self, max: usize) {
        self.shared
            .limit
            .store(max.clamp(1, MAX_CONCURRENT), Ordering::Relaxed);
        wake_all(&self.shared);
    }

    /// The template new downloads are named by (`%t %s %a %u %q %d`); empty means the default.
    pub fn set_filename_template(&self, template: &str) {
        if let Ok(mut slot) = self.shared.template.lock() {
            *slot = if template.is_empty() {
                DEFAULT_TEMPLATE.into()
            } else {
                template.to_owned()
            };
        }
    }

    /// Reserve both output locks before saving preferences. Workers only hold one
    /// of these locks briefly while copying configuration, never across network I/O.
    pub(crate) fn configure_output_when(
        &self,
        value: &crate::storage_settings::Config,
        commit: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        self.configure_output_with(value, None, commit)
    }

    pub(crate) fn configure_restore_output_when(
        &self,
        value: &crate::storage_settings::Config,
        tags: &TagOptions,
        commit: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        self.configure_output_with(value, Some(tags), commit)
    }

    fn configure_output_with(
        &self,
        value: &crate::storage_settings::Config,
        replacement: Option<&TagOptions>,
        commit: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let mut template = self
            .shared
            .template
            .lock()
            .map_err(|_| "download_configuration_unavailable")?;
        let mut tags = self
            .shared
            .tags
            .lock()
            .map_err(|_| "download_configuration_unavailable")?;
        commit()?;
        *template = if value.filename_template.is_empty() {
            DEFAULT_TEMPLATE.into()
        } else {
            value.filename_template.clone()
        };
        if let Some(replacement) = replacement {
            *tags = replacement.clone();
        } else {
            value.apply_tags(&mut tags);
        }
        Ok(())
    }

    /// What gets written into a file's tags once it has downloaded.
    pub fn set_tag_options(&self, options: TagOptions) {
        if let Ok(mut slot) = self.shared.tags.lock() {
            *slot = options;
        }
    }

    /// The plugin used to resolve addresses. Restored queued tasks wait until one is attached.
    pub fn clear_online(&self) {
        if let Ok(mut slot) = self.shared.online.lock() {
            *slot = None;
        }
    }
    pub fn set_online(&self, online: Arc<Online>) {
        if let Ok(mut slot) = self.shared.online.lock() {
            *slot = Some(online);
        }
        wake_all(&self.shared);
    }

    pub fn enqueue(
        &self,
        online: &Arc<Online>,
        item: MusicItem,
        quality: String,
    ) -> Result<Enqueued, String> {
        self.set_online(online.clone());
        self.enqueue_item(item, quality)
    }

    /// A source-bound UI choice must not replace a newer provider when it completes.
    /// Future provider changes still govern queued work, as on desktop.
    pub fn enqueue_current(
        &self,
        expected: &Arc<Online>,
        item: MusicItem,
        quality: String,
    ) -> Result<Enqueued, String> {
        let current = self.shared.online.lock().map_err(|_| "下载队列不可用")?;
        if !current
            .as_ref()
            .is_some_and(|online| Arc::ptr_eq(online, expected))
        {
            return Err("Source changed; choose download quality again".into());
        }
        // Do not nest the online lock with tasks/queue: worker takes queue then online.
        drop(current);
        self.enqueue_item(item, quality)
    }

    fn enqueue_item(&self, item: MusicItem, quality: String) -> Result<Enqueued, String> {
        let song_id = song_key(&item);
        let mut tasks = self.shared.tasks.lock().map_err(|_| "下载队列不可用")?;
        if self.shared.closed.load(Ordering::Acquire) {
            return Err("Download runtime is closed".into());
        }
        let same = |t: &Task| song_key(&t.item) == song_id;
        if tasks.iter().any(|t| same(t) && t.view.status.in_progress()) {
            return Ok(Enqueued::Duplicate);
        }
        if tasks.iter().any(|t| {
            same(t)
                && t.view.status == Status::Done
                && !t.view.missing
                && t.view.path.as_ref().is_some_and(|path| path.exists())
        }) {
            return Ok(Enqueued::AlreadyDone);
        }
        let id = new_id();
        tasks.push(Task {
            view: TaskView {
                id: id.clone(),
                title: item.name.clone(),
                artist: item.singer.clone(),
                quality,
                status: Status::Queued,
                received: 0,
                total: None,
                path: None,
                skipped: false,
                missing: false,
                tag_error: None,
                speed: 0.,
                remaining: None,
                retries: 0,
            },
            item,
            created_at: now_millis(),
            cancel: Arc::new(AtomicBool::new(false)),
            pausing: Arc::new(AtomicBool::new(false)),
            generation: 0,
            running: false,
        });
        drop(tasks);
        self.shared
            .queue
            .lock()
            .map_err(|_| "下载队列不可用")?
            .push_back(id);
        persist(&self.shared);
        self.shared.wake.notify_one();
        Ok(Enqueued::Added)
    }

    pub fn snapshot(&self) -> Vec<TaskView> {
        self.try_snapshot().unwrap_or_default()
    }

    /// Readers that must distinguish an empty queue from a failed lock.
    pub fn try_snapshot(&self) -> Result<Vec<TaskView>, String> {
        self.try_snapshot_bounded(usize::MAX, usize::MAX)
    }

    /// Reject oversized projections before allocating their clones. No task is
    /// removed or omitted, including live transfers beyond the persisted cap.
    pub fn try_snapshot_bounded(
        &self,
        max_tasks: usize,
        max_text_bytes: usize,
    ) -> Result<Vec<TaskView>, String> {
        self.shared
            .tasks
            .lock()
            .map_err(|_| "download queue unavailable".to_owned())
            .and_then(|tasks| {
                if tasks.len() > max_tasks {
                    return Err("download snapshot exceeds task capacity".into());
                }
                let mut bytes = 0usize;
                for task in tasks.iter() {
                    let view = &task.view;
                    for size in [
                        view.id.len(),
                        view.title.len(),
                        view.artist.len(),
                        view.quality.len(),
                        view.path.as_ref().map_or(0, |path| path.as_os_str().len()),
                        view.tag_error.as_ref().map_or(0, String::len),
                        match &view.status {
                            Status::Failed(error) => error.len(),
                            _ => 0,
                        },
                    ] {
                        bytes = bytes
                            .checked_add(size)
                            .ok_or("download snapshot exceeds text capacity")?;
                        if bytes > max_text_bytes {
                            return Err("download snapshot exceeds text capacity".into());
                        }
                    }
                }
                Ok(tasks
                    .iter()
                    .map(|t| {
                        let mut view = t.view.clone();
                        if view.status != Status::Downloading {
                            view.speed = 0.;
                            view.remaining = None;
                        }
                        view
                    })
                    .collect())
            })
    }

    pub fn active_count(&self) -> usize {
        self.shared
            .tasks
            .lock()
            .map(|tasks| tasks.iter().filter(|t| t.view.status.is_active()).count())
            .unwrap_or(0)
    }

    /// Cancels a queued, running or paused task and deletes its partial file; a task already
    /// finished is left as it is.
    pub fn cancel(&self, id: &str) {
        let mut drop_partial = false;
        if let Ok(mut tasks) = self.shared.tasks.lock() {
            if self.shared.closed.load(Ordering::Acquire) {
                return;
            }
            if let Some(task) = tasks.iter_mut().find(|t| t.view.id == id) {
                match task.view.status {
                    Status::Queued | Status::Paused | Status::Failed(_) => {
                        drop_partial = !task.running;
                        task.pausing.store(false, Ordering::Relaxed);
                        task.cancel.store(true, Ordering::Relaxed);
                        task.view.status = Status::Cancelled;
                        task.generation += 1;
                    }
                    Status::Resolving | Status::Downloading => {
                        // The worker deletes the partial file when it notices.
                        task.pausing.store(false, Ordering::Relaxed);
                        task.cancel.store(true, Ordering::Relaxed);
                        task.view.status = Status::Cancelled;
                        task.generation += 1;
                    }
                    _ => {}
                }
            }
        }
        if drop_partial {
            remove_partial_if_open(&self.shared, id);
        }
        persist(&self.shared);
    }

    /// Pauses a queued or running task; its partial file is kept.
    pub fn pause(&self, id: &str) {
        if let Ok(mut tasks) = self.shared.tasks.lock() {
            if self.shared.closed.load(Ordering::Acquire) {
                return;
            }
            if let Some(task) = tasks.iter_mut().find(|t| t.view.id == id) {
                match task.view.status {
                    Status::Queued => task.view.status = Status::Paused,
                    Status::Resolving | Status::Downloading => {
                        task.pausing.store(true, Ordering::Relaxed);
                        task.cancel.store(true, Ordering::Relaxed);
                        task.view.status = Status::Paused;
                    }
                    _ => {}
                }
            }
        }
        persist(&self.shared);
    }

    /// Continues a paused task; the transfer picks up its partial file.
    pub fn resume(&self, id: &str) {
        let resumed = self.shared.tasks.lock().ok().and_then(|mut tasks| {
            if self.shared.closed.load(Ordering::Acquire) {
                return None;
            }
            let task = tasks.iter_mut().find(|t| t.view.id == id)?;
            if task.view.status != Status::Paused {
                return None;
            }
            task.cancel = Arc::new(AtomicBool::new(false));
            task.pausing = Arc::new(AtomicBool::new(false));
            task.generation += 1;
            task.view.status = Status::Queued;
            Some(task.view.id.clone())
        });
        if let Some(id) = resumed {
            if let Ok(mut queue) = self.shared.queue.lock() {
                if !queue.contains(&id) {
                    queue.push_back(id);
                }
            }
            persist(&self.shared);
            self.shared.wake.notify_one();
        }
    }

    /// `pauseAllTasks`: every queued or running task.
    pub fn pause_all(&self) {
        let ids: Vec<String> = self
            .shared
            .tasks
            .lock()
            .map(|tasks| {
                tasks
                    .iter()
                    .filter(|t| t.view.status.is_active())
                    .map(|t| t.view.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            self.pause(&id);
        }
    }

    /// `resumeAllTasks`: every paused task.
    pub fn resume_all(&self) {
        let ids: Vec<String> = self
            .shared
            .tasks
            .lock()
            .map(|tasks| {
                tasks
                    .iter()
                    .filter(|t| t.view.status == Status::Paused)
                    .map(|t| t.view.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            self.resume(&id);
        }
    }

    /// Starts a failed or cancelled task again. A partial file left by a failed transfer is
    /// continued rather than downloaded again.
    pub fn retry(&self, id: &str) {
        let restarted = self.shared.tasks.lock().ok().and_then(|mut tasks| {
            if self.shared.closed.load(Ordering::Acquire) {
                return None;
            }
            let task = tasks.iter_mut().find(|t| t.view.id == id)?;
            if !matches!(task.view.status, Status::Failed(_) | Status::Cancelled) {
                return None;
            }
            task.cancel = Arc::new(AtomicBool::new(false));
            task.pausing = Arc::new(AtomicBool::new(false));
            task.generation += 1;
            task.view.status = Status::Queued;
            task.view.received = 0;
            task.view.total = None;
            task.view.path = None;
            task.view.skipped = false;
            task.view.missing = false;
            task.view.tag_error = None;
            task.view.retries = 0;
            task.view.speed = 0.;
            task.view.remaining = None;
            Some(task.view.id.clone())
        });
        if let Some(id) = restarted {
            if let Ok(mut queue) = self.shared.queue.lock() {
                queue.push_back(id);
            }
            persist(&self.shared);
            self.shared.wake.notify_one();
        }
    }

    /// Removes one finished task's record; the downloaded file is untouched.
    pub fn remove(&self, id: &str) {
        let mut dropped = Vec::new();
        if let Ok(mut tasks) = self.shared.tasks.lock() {
            if self.shared.closed.load(Ordering::Acquire) {
                return;
            }
            tasks.retain(|t| {
                let keep = t.view.id != id || t.view.status.in_progress();
                if !keep {
                    dropped.push(t.view.id.clone());
                }
                keep
            });
        }
        for id in dropped {
            remove_partial_if_open(&self.shared, &id);
        }
        persist(&self.shared);
    }

    /// `validate_files`: marks completed tasks whose file is no longer there (the original does
    /// this whenever the completed tab is opened).
    pub fn validate_files(&self) {
        if let Ok(mut tasks) = self.shared.tasks.lock() {
            if self.shared.closed.load(Ordering::Acquire) {
                return;
            }
            for task in tasks.iter_mut().filter(|t| t.view.status == Status::Done) {
                task.view.missing = !task.view.file_exists();
            }
        }
    }

    /// `clear_tasks` of the original, per tab: the queue (queued, running and paused tasks are
    /// stopped, their partial files deleted and their records removed), the completed records,
    /// or the failed and cancelled ones. Downloaded files are untouched.
    pub fn clear(&self, scope: ClearScope) {
        self.clear_matching(scope, |_| true);
    }

    /// Clear only records captured by a caller's snapshot. Tasks added after
    /// that snapshot, or moved to another bucket, are preserved.
    pub fn clear_selected(&self, scope: ClearScope, ids: &HashSet<String>) {
        self.clear_matching(scope, |id| ids.contains(id));
    }

    fn clear_matching(&self, scope: ClearScope, selected_id: impl Fn(&str) -> bool) {
        let mut dropped = Vec::new();
        if let Ok(mut tasks) = self.shared.tasks.lock() {
            if self.shared.closed.load(Ordering::Acquire) {
                return;
            }
            tasks.retain_mut(|t| {
                if !selected_id(&t.view.id) {
                    return true;
                }
                let selected = match scope {
                    ClearScope::Queue => t.view.status.in_progress(),
                    ClearScope::Completed => t.view.status == Status::Done,
                    ClearScope::Failed => {
                        matches!(t.view.status, Status::Failed(_) | Status::Cancelled)
                    }
                };
                if !selected {
                    return true;
                }
                if t.view.status.in_progress() {
                    // A worker that is still unwinding notices this and deletes its file.
                    t.pausing.store(false, Ordering::Relaxed);
                    t.cancel.store(true, Ordering::Relaxed);
                    t.generation += 1;
                }
                dropped.push(t.view.id.clone());
                false
            });
        }
        for id in dropped {
            remove_partial_if_open(&self.shared, &id);
        }
        persist(&self.shared);
    }
}

/// Which tab's records [`Downloads::clear`] removes.
#[derive(Clone, Copy)]
pub enum ClearScope {
    Queue,
    Completed,
    Failed,
}

/// Identifies a song across restarts: its source and original ID.
fn song_key(item: &MusicItem) -> String {
    let raw = match &item.songmid {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    format!("{}:{raw}", item.source)
}

/// Writes all records atomically (temp file in the same directory, then rename).
fn persist(shared: &Shared) {
    let _ = persist_checked(shared, false);
}

fn persist_checked(shared: &Shared, final_shutdown: bool) -> Result<(), String> {
    let Some(path) = &shared.records else {
        return Ok(());
    };
    let _guard = shared
        .write_lock
        .lock()
        .map_err(|_| "Download record lock unavailable")?;
    // Check under the write lock so an already admitted stale operation cannot
    // overwrite the final shutdown snapshot after a replacement core opens.
    if shared.closed.load(Ordering::Acquire) && !final_shutdown {
        return Ok(());
    }
    let mut records: Vec<(i64, bool, Record)> = match shared.tasks.lock() {
        Ok(tasks) => tasks
            .iter()
            .map(|t| (t.created_at, t.view.status.in_progress(), t.record()))
            .collect(),
        Err(_) => return Err("Download queue unavailable".into()),
    };
    if records.len() > MAX_RECORDS {
        // Keep every active task; drop the oldest finished ones.
        records.sort_by_key(|(created, active, _)| (*active, *created));
        let excess = records.len() - MAX_RECORDS;
        records.drain(..excess);
    }
    records.sort_by_key(|(created, ..)| *created);
    let records: Vec<Record> = records.into_iter().map(|(.., r)| r).collect();
    let bytes =
        serde_json::to_vec_pretty(&records).map_err(|_| "Cannot encode download records")?;
    let parent = path
        .parent()
        .ok_or("Download record directory unavailable")?;
    std::fs::create_dir_all(parent).map_err(|_| "Cannot create download record directory")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Cannot create download record file")?;
    file.write_all(&bytes)
        .map_err(|_| "Cannot write download records")?;
    file.as_file()
        .sync_all()
        .map_err(|_| "Cannot sync download records")?;
    file.persist(path)
        .map_err(|_| "Cannot publish download records")?;
    Ok(())
}

/// The partial file of a task, if one is on disk: `*.<id>.part`, with its size.
fn find_partial(dir: &Path, id: &str) -> Option<(PathBuf, u64)> {
    let suffix = format!(".{id}.part");
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(&suffix) {
            return None;
        }
        let meta = entry.metadata().ok()?;
        meta.is_file().then(|| (entry.path(), meta.len()))
    })
}

fn remove_partial(dir: &Path, id: &str) {
    while let Some((path, _)) = find_partial(dir, id) {
        if std::fs::remove_file(path).is_err() {
            break;
        }
    }
}

fn remove_partial_if_open(shared: &Shared, id: &str) {
    let Ok(_guard) = shared.write_lock.lock() else {
        return;
    };
    if !shared.closed.load(Ordering::Acquire) {
        remove_partial(&shared.dir(), id);
    }
}

/// Removes partial files nobody can continue: those of cancelled and finished tasks. Paused,
/// waiting and failed tasks keep theirs (a failed one resumes on retry).
fn clean_partials(shared: &Shared) {
    let ids: Vec<String> = shared
        .tasks
        .lock()
        .map(|tasks| {
            tasks
                .iter()
                .filter(|t| matches!(t.view.status, Status::Cancelled | Status::Done))
                .map(|t| t.view.id.clone())
                .collect()
        })
        .unwrap_or_default();
    for id in ids {
        remove_partial(&shared.dir(), &id);
    }
}

/// Changes a task's view, unless the task has been started again or cancelled since the run
/// this belongs to (`generation`).
fn update(shared: &Shared, id: &str, generation: u64, change: impl FnOnce(&mut TaskView)) {
    if let Ok(mut tasks) = shared.tasks.lock() {
        if let Some(task) = tasks
            .iter_mut()
            .find(|t| t.view.id == id && t.generation == generation)
        {
            change(&mut task.view);
        }
    }
}

/// Wakes every worker, taking the queue lock so none misses it between its check and its wait.
fn wake_all(shared: &Shared) {
    let _queue = shared.queue.lock();
    shared.wake.notify_all();
}

/// One of the `limit` places for working on a task.
struct Slot(Arc<Shared>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
        wake_all(&self.0);
    }
}

enum Job {
    Ready(Box<(TaskView, MusicItem, Arc<AtomicBool>, Arc<AtomicBool>, u64)>),
    /// The task's previous worker has not finished unwinding yet.
    Busy,
    Gone,
}

fn worker(shared: Arc<Shared>) {
    loop {
        // Wait for a queued task and a plugin to resolve it with.
        let (id, online) = {
            let Ok(mut queue) = shared.queue.lock() else {
                return;
            };
            loop {
                if shared.closed.load(Ordering::Acquire) {
                    return;
                }
                let online = shared.online.lock().ok().and_then(|o| o.clone());
                let free =
                    shared.active.load(Ordering::SeqCst) < shared.limit.load(Ordering::SeqCst);
                if let (Some(online), false, true) = (online, queue.is_empty(), free) {
                    if let Some(id) = queue.pop_front() {
                        shared.active.fetch_add(1, Ordering::SeqCst);
                        break (id, online);
                    }
                }
                queue = match shared.wake.wait(queue) {
                    Ok(queue) => queue,
                    Err(_) => return,
                };
            }
        };
        // Gives the slot back (and lets a waiting worker take the next task) however this
        // round ends.
        let _slot = Slot(shared.clone());
        let job = match shared.tasks.lock() {
            Ok(mut tasks) => match tasks
                .iter_mut()
                .find(|t| t.view.id == id && t.view.status == Status::Queued)
            {
                None => Job::Gone, // cancelled or paused while queued
                Some(task) if task.running => Job::Busy,
                Some(task) => {
                    task.running = true;
                    Job::Ready(Box::new((
                        task.view.clone(),
                        task.item.clone(),
                        task.cancel.clone(),
                        task.pausing.clone(),
                        task.generation,
                    )))
                }
            },
            Err(_) => return,
        };
        let (view, item, cancel, pausing, generation) = match job {
            Job::Ready(job) => *job,
            Job::Busy => {
                // Continued right after a pause: wait for the old transfer to let go of the
                // partial file, then take the task again.
                std::thread::sleep(Duration::from_millis(100));
                if let Ok(mut queue) = shared.queue.lock() {
                    queue.push_back(id);
                }
                continue;
            }
            Job::Gone => continue,
        };
        update(&shared, &id, generation, |v| v.status = Status::Resolving);
        let outcome = run(
            &shared, &id, generation, &view, &item, &cancel, &pausing, &online,
        );
        let mut again = false;
        if let Ok(mut tasks) = shared.tasks.lock() {
            if let Some(task) = tasks.iter_mut().find(|t| t.view.id == id) {
                task.running = false;
                // A newer run or a cancel owns the state now.
                if task.generation == generation {
                    task.view.speed = 0.;
                    task.view.remaining = None;
                    task.view.status = match outcome {
                        Ok(()) => Status::Done,
                        Err(_) if pausing.load(Ordering::Relaxed) => Status::Paused,
                        Err(_) if cancel.load(Ordering::Relaxed) => Status::Cancelled,
                        // The original puts a failed transfer back in the queue up to three
                        // times (the partial file is continued) before it reports the error.
                        Err(_) if task.view.retries < AUTO_RETRIES => {
                            task.view.retries += 1;
                            again = true;
                            Status::Queued
                        }
                        Err(message) => Status::Failed(message),
                    };
                }
            }
        }
        if again {
            if let Ok(mut queue) = shared.queue.lock() {
                queue.push_back(id.clone());
            }
        }
        persist(&shared);
    }
}

/// `Content-Range: bytes <start>-<end>/<total>` → (start, total).
fn parse_content_range(value: &str) -> Option<(u64, u64)> {
    let value = value.trim().strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, _) = range.split_once('-')?;
    Some((start.trim().parse().ok()?, total.trim().parse().ok()?))
}

/// `Content-Range: bytes */<total>` of a 416 answer → total.
fn parse_unsatisfied_range(value: &str) -> Option<u64> {
    value
        .trim()
        .strip_prefix("bytes */")
        .and_then(|total| total.trim().parse().ok())
}

/// File publication and pause/cancel share the task lock as their commit point.
/// Once published, the task is Done and a later pause/cancel leaves it alone.
/// Tag writing and network work stay outside this lock.
fn commit_download(
    shared: &Shared,
    id: &str,
    generation: u64,
    cancel: &AtomicBool,
    target: &Path,
    temp: &Path,
) -> Result<bool, String> {
    let mut tasks = shared.tasks.lock().map_err(|_| "下载状态不可用")?;
    let task = tasks
        .iter_mut()
        .find(|t| t.view.id == id && t.generation == generation)
        .ok_or("已取消")?;
    if cancel.load(Ordering::Relaxed)
        || !matches!(task.view.status, Status::Resolving | Status::Downloading)
    {
        return Err("已取消".into());
    }
    let skipped = if target.exists() {
        let _ = std::fs::remove_file(temp);
        true
    } else {
        let mut partial_path =
            tempfile::TempPath::try_from_path(temp).map_err(|_| "无法识别下载临时文件")?;
        // Keep a complete partial on a failed OS commit so retry can resume it.
        partial_path.disable_cleanup(true);
        match partial_path.persist_noclobber(target) {
            Ok(()) => false,
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(temp);
                true
            }
            Err(_) => return Err("无法保存下载文件".into()),
        }
    };
    task.view.path = Some(target.to_owned());
    task.view.skipped = skipped;
    task.view.status = Status::Done;
    task.view.speed = 0.;
    task.view.remaining = None;
    Ok(!skipped)
}

// DNS may use an OS blocking resolver. Keep only this bounded network executor
// alive for the process, so closing a download worker never waits for a resolver
// runtime destructor; all file-owning workers are still joined before reopening.
fn network_runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static NETWORK: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    NETWORK
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .max_blocking_threads(8)
                .enable_all()
                .build()
                .map_err(|_| "无法创建下载运行时".to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

async fn cancellable_http<T, F>(
    cancel: &AtomicBool,
    operation: impl FnOnce() -> F,
) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, reqwest::Error>>,
{
    tokio::select! {
        biased;
        _ = async {
            while !cancel.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } => Err("已取消".into()),
        // RequestBuilder::send creates Tokio timers immediately. Construct the
        // future only when polled inside block_on, never on the plain worker.
        result = async { operation().await } => result.map_err(|_| "下载连接失败或中断".into()),
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    shared: &Shared,
    id: &str,
    generation: u64,
    view: &TaskView,
    item: &MusicItem,
    cancel: &Arc<AtomicBool>,
    pausing: &Arc<AtomicBool>,
    online: &Arc<Online>,
) -> Result<(), String> {
    let url = online.resolve_download(item, Some(&view.quality), cancel.clone())?;
    if cancel.load(Ordering::Relaxed) {
        return Err("已取消".into());
    }
    let runtime = network_runtime()?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        // The whole transfer, body included; a stalled read ends when it elapses.
        .timeout(Duration::from_secs(20 * 60))
        .read_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|_| "无法创建下载客户端")?;
    std::fs::create_dir_all(&shared.dir()).map_err(|_| "无法创建下载目录")?;

    // Ask for the rest of an earlier partial file; the server's answer decides whether it can
    // be used. A 200 means the range was ignored and the file starts over.
    let mut partial = find_partial(&shared.dir(), id);
    let (mut response, resume_total) = loop {
        let offset = partial.as_ref().map_or(0, |(_, size)| *size);
        let mut request = client.get(&url);
        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let response = runtime.block_on(cancellable_http(cancel, || request.send()))?;
        let status = response.status();
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        if offset > 0 && status.as_u16() == 416 {
            // The partial file is already the whole file, or no longer fits the server's.
            let complete = header(reqwest::header::CONTENT_RANGE)
                .and_then(|v| parse_unsatisfied_range(&v))
                .is_some_and(|total| total == offset);
            if complete {
                break (None, Some(offset));
            }
            remove_partial(&shared.dir(), id);
            partial = None;
            continue;
        }
        if offset > 0 && status.as_u16() == 206 {
            let matches = header(reqwest::header::CONTENT_RANGE)
                .and_then(|v| parse_content_range(&v))
                .is_some_and(|(start, total)| start == offset && total > offset);
            if !matches {
                remove_partial(&shared.dir(), id);
                partial = None;
                continue;
            }
        }
        break (Some(response), None);
    };

    let (append, total, content_type) = match (&mut response, resume_total) {
        (None, Some(total)) => (true, Some(total), None),
        (Some(response), _) => {
            if !response.status().is_success() {
                return Err(format!("下载返回 HTTP {}", response.status().as_u16()));
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            if content_type
                .as_deref()
                .is_some_and(|c| c.starts_with("text/") || c.contains("json") || c.contains("html"))
            {
                return Err("下载地址没有返回音频".into());
            }
            if response.status().as_u16() == 206 && partial.is_some() {
                let total = response
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_content_range)
                    .map(|(_, total)| total);
                (true, total, content_type)
            } else {
                (false, response.content_length(), content_type)
            }
        }
        (None, None) => return Err("下载连接失败或超时".into()),
    };
    if total.is_some_and(|t| t > MAX_BYTES) {
        return Err("文件超过 1GiB 上限".into());
    }

    // Name the file. A continued transfer keeps the name its partial file had.
    let template = shared
        .template
        .lock()
        .map(|t| t.clone())
        .unwrap_or_else(|_| DEFAULT_TEMPLATE.into());
    let stem = file_stem(&template, item, &view.quality);
    let suffix = format!(".{id}.part");
    let (target, temp) = match (&partial, append) {
        (Some((path, _)), true) => {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(&suffix))
                .map(str::to_owned)
                .ok_or("无法识别续传文件")?;
            (shared.dir().join(name), path.clone())
        }
        _ => {
            if let Some((old, _)) = &partial {
                let _ = std::fs::remove_file(old);
            }
            let extension = extension(&url, content_type.as_deref(), &view.quality);
            let target = shared.dir().join(format!("{stem}.{extension}"));
            let temp = shared.dir().join(format!("{stem}.{extension}{suffix}"));
            (target, temp)
        }
    };
    if target.exists() {
        let result = commit_download(shared, id, generation, cancel, &target, &temp);
        if result.is_err() && cancel.load(Ordering::Relaxed) && !pausing.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&temp);
        }
        return result.map(|_| ());
    }
    let offset = if append {
        partial.as_ref().map_or(0, |(_, size)| *size)
    } else {
        0
    };
    update(shared, id, generation, |v| {
        v.status = Status::Downloading;
        v.total = total;
        v.received = offset;
        v.path = Some(target.clone());
    });
    persist(shared);
    let result = (|| -> Result<(), String> {
        let mut received = offset;
        if let Some(response) = response.as_mut() {
            let mut file = if append {
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(&temp)
                    .map_err(|_| "无法打开续传文件")?
            } else {
                std::fs::File::create(&temp).map_err(|_| "无法创建下载文件")?
            };
            let mut reported = received;
            let mut sampled_at = std::time::Instant::now();
            let mut sampled_bytes = received;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err("已取消".into());
                }
                let chunk = runtime.block_on(cancellable_http(cancel, || response.chunk()))?;
                // A pause/cancel may have arrived while the final read was blocked.
                if cancel.load(Ordering::Relaxed) {
                    return Err("已取消".into());
                }
                let Some(buffer) = chunk else {
                    break;
                };
                let read = buffer.len();
                received += read as u64;
                if received > MAX_BYTES {
                    return Err("文件超过 1GiB 上限".into());
                }
                file.write_all(&buffer).map_err(|_| "写入文件失败")?;
                // Progress every 256 KB or half a second, and the speed over that stretch
                // (`download:task-progress` of the original).
                let now = std::time::Instant::now();
                let since = now.duration_since(sampled_at);
                if received - reported >= 256 * 1024 || since >= Duration::from_millis(500) {
                    reported = received;
                    let seconds = since.as_secs_f64();
                    let speed =
                        (seconds >= 0.2).then(|| (received - sampled_bytes) as f64 / seconds);
                    if speed.is_some() {
                        sampled_at = now;
                        sampled_bytes = received;
                    }
                    update(shared, id, generation, |v| {
                        v.received = received;
                        if let Some(speed) = speed {
                            v.speed = speed;
                            v.remaining = match (v.total, speed > 0.) {
                                (Some(total), true) => {
                                    Some(total.saturating_sub(received) as f64 / speed)
                                }
                                _ => None,
                            };
                        }
                    });
                }
            }
            file.flush().map_err(|_| "写入文件失败")?;
        }
        if received == 0 {
            return Err("下载内容为空".into());
        }
        if total.is_some_and(|t| t != received) {
            return Err("下载不完整".into());
        }
        update(shared, id, generation, |v| v.received = received);
        Ok(())
    })();
    match result.and_then(|()| commit_download(shared, id, generation, cancel, &target, &temp)) {
        Ok(committed) => {
            // A tag failure is reported on the task, not as a failed download.
            if committed {
                if let Err(error) = write_tags(shared, &target, item, cancel) {
                    update(shared, id, generation, |v| v.tag_error = Some(error));
                }
            }
            Ok(())
        }
        Err(error) => {
            // A pause keeps what was written, and so does a transfer that broke off: the next
            // run continues from it. Only a cancel (or a file that cannot be right) removes it.
            let cancelled = cancel.load(Ordering::Relaxed) && !pausing.load(Ordering::Relaxed);
            let too_big = std::fs::metadata(&temp)
                .map(|m| total.is_some_and(|t| m.len() > t))
                .unwrap_or(false);
            if cancelled || too_big {
                let _ = std::fs::remove_file(&temp);
            }
            Err(error)
        }
    }
}

/// Cover image of a song for tagging: http(s) only, small, quick.
fn fetch_cover(url: &str, cancel: &AtomicBool) -> Option<Vec<u8>> {
    const MAX: u64 = 8 * 1024 * 1024;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let runtime = network_runtime().ok()?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .ok()?;
    let mut response = runtime
        .block_on(cancellable_http(cancel, || client.get(url).send()))
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = runtime
        .block_on(cancellable_http(cancel, || response.chunk()))
        .ok()?
    {
        if bytes.len() as u64 + chunk.len() as u64 > MAX {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    (!bytes.is_empty() && bytes.len() as u64 <= MAX).then_some(bytes)
}

fn write_tags(
    shared: &Shared,
    path: &Path,
    item: &MusicItem,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let options = shared.tags.lock().map(|o| o.clone()).unwrap_or_default();
    if !options.any() {
        return Ok(());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("下载标签写入已中断".into());
    }
    let cover = if options.wants_cover() {
        let url = if item.img.is_empty() {
            mio_music_sdk::get_cover(item).unwrap_or_default()
        } else {
            item.img.clone()
        };
        fetch_cover(&url, cancel)
    } else {
        None
    };
    // Lyrics: the song's own `lrc` if it has one, else its source's lyric API. Only the
    // line-level text is written; word-timed forms differ per source and no player reads them
    // from a tag.
    if cancel.load(Ordering::Relaxed) {
        return Err("下载标签写入已中断".into());
    }
    let lyrics = if options.lyrics || options.download_lyrics {
        item.lrc
            .clone()
            .filter(|l| !l.trim().is_empty())
            .or_else(|| {
                mio_music_sdk::get_lyric(item)
                    .ok()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.lyric)
            })
    } else {
        None
    };
    if cancel.load(Ordering::Relaxed) {
        return Err("下载标签写入已中断".into());
    }
    crate::tags::write_download_tags(
        path,
        &item.name,
        &item.singer,
        &item.album_name,
        &options,
        cover.as_deref(),
        lyrics.as_deref(),
    )
}
