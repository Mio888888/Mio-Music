use super::types::{DownloadStatus, DownloadTask};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

#[derive(Clone)]
pub struct DownloadManager {
    tasks: Arc<RwLock<HashMap<String, DownloadTask>>>,
    queue: Arc<Mutex<Vec<String>>>,
    max_concurrent: Arc<RwLock<usize>>,
    abort_handles: Arc<RwLock<HashMap<String, tokio::task::AbortHandle>>>,
    client: Client,
    app_data_dir: PathBuf,
}

impl DownloadManager {
    pub fn new(app_data_dir: &std::path::Path) -> Self {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            queue: Arc::new(Mutex::new(Vec::new())),
            max_concurrent: Arc::new(RwLock::new(3)),
            abort_handles: Arc::new(RwLock::new(HashMap::new())),
            client,
            app_data_dir: app_data_dir.to_path_buf(),
        }
    }

    fn downloads_file(&self) -> PathBuf {
        self.app_data_dir.join("downloads.json")
    }

    async fn save_tasks(&self) {
        let tasks = self.tasks.read().await;
        let vec: Vec<&DownloadTask> = tasks.values().collect();
        if let Ok(data) = serde_json::to_string_pretty(&vec) {
            let _ = fs::write(self.downloads_file(), data);
        }
    }

    #[allow(dead_code)]
    pub async fn load_tasks(&self) {
        if let Ok(data) = fs::read_to_string(self.downloads_file()) {
            if let Ok(mut vec) = serde_json::from_str::<Vec<DownloadTask>>(&data) {
                for t in vec.iter_mut() {
                    if t.status == DownloadStatus::Downloading {
                        t.status = DownloadStatus::Paused;
                    }
                }
                let mut tasks = self.tasks.write().await;
                let mut queue = self.queue.lock().await;
                for t in vec {
                    if t.status == DownloadStatus::Queued {
                        queue.push(t.id.clone());
                    }
                    tasks.insert(t.id.clone(), t);
                }
            }
        }
    }

    pub async fn get_tasks(&self) -> Vec<DownloadTask> {
        self.tasks.read().await.values().cloned().collect()
    }

    pub async fn add_task(
        &self,
        song_info: Value,
        url: String,
        file_path: String,
        plugin_id: Option<String>,
        quality: Option<String>,
        priority: u32,
    ) -> Result<DownloadTask, String> {
        {
            let tasks = self.tasks.read().await;
            for t in tasks.values() {
                if t.file_path == file_path {
                    match t.status {
                        DownloadStatus::Completed => {
                            if std::path::Path::new(&file_path).exists() {
                                return Err("歌曲已下载完成".into());
                            }
                        }
                        DownloadStatus::Downloading | DownloadStatus::Queued | DownloadStatus::Paused => {
                            return Err("歌曲正在下载中".into());
                        }
                        _ => {}
                    }
                }
            }
        }

        let task = DownloadTask {
            id: Uuid::new_v4().to_string(),
            song_info,
            url,
            plugin_id,
            quality,
            file_path,
            status: DownloadStatus::Queued,
            progress: 0.0,
            speed: 0.0,
            total_size: 0,
            downloaded_size: 0,
            remaining_time: None,
            retries: 0,
            error: None,
            priority,
            created_at: chrono::Utc::now().timestamp_millis(),
        };

        self.tasks.write().await.insert(task.id.clone(), task.clone());
        self.queue.lock().await.push(task.id.clone());
        self.save_tasks().await;
        self.spawn_next().await;
        Ok(task)
    }

    pub async fn pause_task(&self, task_id: &str) -> Result<(), String> {
        {
            let mut tasks = self.tasks.write().await;
            if let Some(t) = tasks.get_mut(task_id) {
                if t.status != DownloadStatus::Downloading {
                    return Ok(());
                }
                t.status = DownloadStatus::Paused;
                t.speed = 0.0;
                t.remaining_time = None;
            }
        }
        if let Some(h) = self.abort_handles.write().await.remove(task_id) {
            h.abort();
        }
        self.save_tasks().await;
        Ok(())
    }

    pub async fn resume_task(&self, task_id: &str) -> Result<(), String> {
        {
            let mut tasks = self.tasks.write().await;
            if let Some(t) = tasks.get_mut(task_id) {
                if t.status != DownloadStatus::Paused {
                    return Ok(());
                }
                t.status = DownloadStatus::Queued;
            }
        }
        self.queue.lock().await.push(task_id.to_string());
        self.save_tasks().await;
        self.spawn_next().await;
        Ok(())
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<(), String> {
        let file_path = {
            let mut tasks = self.tasks.write().await;
            match tasks.get_mut(task_id) {
                Some(t) => {
                    t.status = DownloadStatus::Cancelled;
                    t.speed = 0.0;
                    t.file_path.clone()
                }
                None => return Err("任务不存在".into()),
            }
        };
        let _ = fs::remove_file(format!("{}.temp", file_path));
        if let Some(h) = self.abort_handles.write().await.remove(task_id) {
            h.abort();
        }
        self.save_tasks().await;
        Ok(())
    }

    pub async fn delete_task(&self, task_id: &str, delete_file: bool) -> Result<(), String> {
        let task = {
            let tasks = self.tasks.read().await;
            tasks.get(task_id).cloned().ok_or("任务不存在")?
        };
        if task.status == DownloadStatus::Downloading || task.status == DownloadStatus::Queued {
            let _ = self.cancel_task(task_id).await;
        }
        if delete_file && !task.file_path.is_empty() {
            let _ = fs::remove_file(&task.file_path);
        }
        self.tasks.write().await.remove(task_id);
        self.queue.lock().await.retain(|id| id != task_id);
        self.save_tasks().await;
        Ok(())
    }

    pub async fn retry_task(&self, task_id: &str) -> Result<(), String> {
        {
            let mut tasks = self.tasks.write().await;
            if let Some(t) = tasks.get_mut(task_id) {
                if !matches!(t.status, DownloadStatus::Error | DownloadStatus::Cancelled | DownloadStatus::Completed) {
                    return Ok(());
                }
                t.status = DownloadStatus::Queued;
                t.retries = 0;
                t.error = None;
                t.progress = 0.0;
                t.downloaded_size = 0;
            }
        }
        self.queue.lock().await.push(task_id.to_string());
        self.save_tasks().await;
        self.spawn_next().await;
        Ok(())
    }

    pub async fn pause_all_tasks(&self) {
        let ids: Vec<String> = self.tasks.read().await.values()
            .filter(|t| t.status == DownloadStatus::Downloading)
            .map(|t| t.id.clone()).collect();
        for id in &ids {
            let _ = self.pause_task(id).await;
        }
        {
            let mut tasks = self.tasks.write().await;
            let mut queue = self.queue.lock().await;
            for id in queue.drain(..) {
                if let Some(t) = tasks.get_mut(&id) {
                    if t.status == DownloadStatus::Queued {
                        t.status = DownloadStatus::Paused;
                    }
                }
            }
        }
        self.save_tasks().await;
    }

    pub async fn resume_all_tasks(&self) {
        let ids: Vec<String> = self.tasks.read().await.values()
            .filter(|t| t.status == DownloadStatus::Paused)
            .map(|t| t.id.clone()).collect();
        for id in ids {
            let _ = self.resume_task(&id).await;
        }
    }

    pub async fn clear_tasks(&self, task_type: &str) {
        let ids: Vec<String> = self.tasks.read().await.values()
            .filter(|t| match task_type {
                "queue" => matches!(t.status, DownloadStatus::Downloading | DownloadStatus::Queued | DownloadStatus::Paused),
                "completed" => t.status == DownloadStatus::Completed,
                "failed" => matches!(t.status, DownloadStatus::Error | DownloadStatus::Cancelled),
                "all" => true,
                _ => false,
            })
            .map(|t| t.id.clone()).collect();
        for id in &ids {
            let _ = self.cancel_task(id).await;
        }
        {
            let mut tasks = self.tasks.write().await;
            for id in &ids {
                tasks.remove(id);
            }
        }
        self.save_tasks().await;
    }

    pub async fn set_max_concurrent(&self, max: usize) {
        *self.max_concurrent.write().await = max;
        self.spawn_next().await;
    }

    pub async fn get_max_concurrent(&self) -> usize {
        *self.max_concurrent.read().await
    }

    pub async fn validate_files(&self) {
        let mut changed = false;
        {
            let mut tasks = self.tasks.write().await;
            for t in tasks.values_mut() {
                if t.status == DownloadStatus::Completed
                    && !t.file_path.is_empty()
                    && !std::path::Path::new(&t.file_path).exists()
                {
                    t.status = DownloadStatus::Error;
                    t.error = Some("文件已删除或移动".into());
                    changed = true;
                }
            }
        }
        if changed {
            self.save_tasks().await;
        }
    }

    pub async fn open_file_location(&self, file_path: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        { std::process::Command::new("open").args(["-R", file_path]).spawn().map_err(|e| e.to_string())?; }
        #[cfg(target_os = "windows")]
        { std::process::Command::new("explorer").args(["/select,", file_path]).spawn().map_err(|e| e.to_string())?; }
        #[cfg(target_os = "linux")]
        { if let Some(p) = std::path::Path::new(file_path).parent() { std::process::Command::new("xdg-open").arg(p).spawn().map_err(|e| e.to_string())?; } }
        Ok(())
    }

    /// Try to start queued downloads if under the concurrency limit.
    /// This does NOT recursively spawn - callers should call this after state changes.
    pub(crate) async fn spawn_next(&self) {
        loop {
            let mc = *self.max_concurrent.read().await;
            let active = self.tasks.read().await.values()
                .filter(|t| t.status == DownloadStatus::Downloading).count();
            if active >= mc {
                return;
            }

            let task_id = {
                let mut q = self.queue.lock().await;
                match q.first() {
                    None => return,
                    Some(_) => q.remove(0),
                }
            };

            let ok = {
                let tasks = self.tasks.read().await;
                matches!(tasks.get(&task_id), Some(t) if t.status == DownloadStatus::Queued)
            };
            if !ok {
                continue;
            }

            let task = {
                let mut tasks = self.tasks.write().await;
                match tasks.get_mut(&task_id) {
                    Some(t) => {
                        t.status = DownloadStatus::Downloading;
                        Some(t.clone())
                    }
                    None => None,
                }
            };
            let task = match task {
                Some(t) => t,
                None => continue,
            };

            // Spawn the download task
            let dm = self.clone();
            let id = task_id.clone();
            let client = self.client.clone();
            let file_path = task.file_path.clone();
            let url = task.url.clone();

            let handle = tokio::spawn(run_download_task(dm.clone(), id.clone(), client, url, file_path));

            self.abort_handles.write().await.insert(task_id, handle.abort_handle());
        }
    }
}

/// Standalone async function to run a single download task.
/// Takes all data by value to avoid borrow issues with tokio::spawn.
async fn run_download_task(
    dm: DownloadManager,
    id: String,
    client: Client,
    url: String,
    file_path: String,
) {
    match execute_download(&client, &url, &file_path).await {
        Ok(final_path) => {
            let mut tasks = dm.tasks.write().await;
            if let Some(t) = tasks.get_mut(&id) {
                t.status = DownloadStatus::Completed;
                t.progress = 100.0;
                t.downloaded_size = t.total_size;
                t.file_path = final_path;
            }
            drop(tasks);
            dm.save_tasks().await;
        }
        Err(e) => {
            let mut tasks = dm.tasks.write().await;
            if let Some(t) = tasks.get_mut(&id) {
                if t.retries < 3 {
                    t.retries += 1;
                    t.status = DownloadStatus::Queued;
                    t.error = None;
                    t.progress = 0.0;
                    t.downloaded_size = 0;
                } else {
                    t.status = DownloadStatus::Error;
                    t.error = Some(e);
                }
            }
            drop(tasks);
            dm.save_tasks().await;
        }
    }
    dm.abort_handles.write().await.remove(&id);
    // Don't recursively call spawn_next - callers will trigger it
}

async fn execute_download(
    client: &Client,
    url: &str,
    file_path: &str,
) -> Result<String, String> {
    let temp_path = format!("{}.temp", file_path);

    if let Some(parent) = std::path::Path::new(file_path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let start_byte = fs::metadata(&temp_path).map(|m| m.len()).unwrap_or(0);

    let mut request = client.get(url);
    if start_byte > 0 {
        request = request.header("Range", format!("bytes={}-", start_byte));
    }

    let response = request.send().await.map_err(|e| e.to_string())?;
    let total_size = response.content_length().unwrap_or(0) + start_byte;

    if total_size == 0 {
        return Err("文件大小为0".into());
    }

    let mut stream = response.bytes_stream();

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&temp_path)
        .await
        .map_err(|e| e.to_string())?;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }

    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(&temp_path, file_path).map_err(|e| e.to_string())?;
    Ok(file_path.to_string())
}
