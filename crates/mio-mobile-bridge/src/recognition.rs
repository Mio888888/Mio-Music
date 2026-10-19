//! One immutable, cancellable recognition attempt against private picker staging.
//! No microphone entry exists until the native permission/audio-session lease is wired.
use crate::{api::*, catalog::SnapshotPermit, discovery::Worker, imports, runtime};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

static ACTIVE: AtomicBool = AtomicBool::new(false);
// Log only fixed local categories, never paths, fingerprints or provider payloads.
fn recognition_error(error: String) -> String {
    let category = match error.as_str() {
        "无法读取音频文件"
        | "无法读取文件信息"
        | "音频文件过大（上限 128 MB）"
        | "无法解码音频文件"
        | "不支持的音频格式"
        | "音频解码超时" => "audio",
        "未检测到足够的声音，请重试" => "silence",
        "无法加载指纹模块" => "fingerprint_module",
        "不支持的指纹模块导入" | "指纹模块初始化失败" => {
            "fingerprint_initialize"
        }
        "指纹绑定初始化失败" => "fingerprint_bindings",
        "指纹模块缺少内存" | "指纹模块缺少分配器" | "指纹内存分配失败" | "指纹输入失败" => {
            "fingerprint_memory"
        }
        "指纹模块缺少函数表" | "无效指纹函数" => "fingerprint_function",
        "指纹计算失败" | "指纹计算初始化失败" => "fingerprint_calculation",
        "无效指纹结果" | "未能提取音频指纹" => "fingerprint_result",
        "未识别到歌曲，请重试" => "no_match",
        "无法初始化识别请求"
        | "识别服务请求失败，请稍后重试"
        | "无法读取识别结果"
        | "识别结果过大"
        | "识别服务返回无效数据"
        | "识别服务暂时不可用" => "service",
        "识别已取消" => "cancelled",
        reason if reason.starts_with("指纹模块缺少 ") => "fingerprint_missing_binding",
        _ => "capture_or_unavailable",
    };
    eprintln!("Mio recognition outcome: {category}");
    format!("recognition_{category}")
}
struct RecognitionWorker;
impl RecognitionWorker {
    fn acquire() -> Result<Self, String> {
        ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| "busy: recognition in progress".into())
    }
}
impl Drop for RecognitionWorker {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
    }
}
struct HistoryState {
    snapshot: Option<mio_core::recognition_history::Snapshot>,
    revision: u64,
    warning: Option<String>,
}
impl HistoryState {
    fn read(store: &mio_core::storage::Storage) -> Self {
        match mio_core::recognition_history::Snapshot::read(store) {
            Ok(snapshot) => {
                let warning = snapshot.rows().err();
                Self {
                    snapshot: Some(snapshot),
                    revision: 0,
                    warning,
                }
            }
            Err(warning) => Self {
                snapshot: None,
                revision: 0,
                warning: Some(warning),
            },
        }
    }
    fn view(&self) -> Result<RecognitionHistory, String> {
        let rows = self
            .snapshot
            .as_ref()
            .and_then(|value| value.rows().ok())
            .unwrap_or(&[]);
        Ok(RecognitionHistory {
            rows: project(rows)?,
            revision: self.revision,
            warning: self.warning.clone(),
            can_clear: self.snapshot.is_some(),
        })
    }
}
pub(crate) struct HistoryCommit {
    state: Arc<Mutex<HistoryState>>,
    rows: Option<Vec<mio_core::recognition::Match>>,
    expected: u64,
    lease: runtime::RuntimeLease,
    source_revision: u64,
    cancelled: Arc<AtomicBool>,
}
impl HistoryCommit {
    pub(crate) fn apply(self, core: &mio_core::app_core::AppCore) -> Result<(), String> {
        self.lease.ensure_open()?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if self.lease.source_revision() != self.source_revision {
            return Err("source_changed".into());
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| "recognition_history_unreadable")?;
        if state.revision != self.expected {
            return Err("recognition_history_changed".into());
        }
        let next = state
            .revision
            .checked_add(1)
            .ok_or("recognition_history_changed")?;
        let snapshot = state
            .snapshot
            .as_mut()
            .ok_or("recognition_history_unreadable")?;
        let result = match self.rows {
            Some(rows) => snapshot.append(&core.store, &rows, &self.cancelled),
            None => snapshot.clear(&core.store, &self.cancelled),
        };
        match result {
            Ok(()) => {
                state.revision = next;
                state.warning = None;
                Ok(())
            }
            Err(error) => {
                state.warning = Some(error.clone());
                Err(error)
            }
        }
    }
}
type Attempt = Option<(String, Result<Vec<mio_core::recognition::Match>, String>)>;
pub(crate) struct Session {
    cache: PathBuf,
    history: Arc<Mutex<HistoryState>>,
    lease: runtime::RuntimeLease,
    source_revision: u64,
    cancelled: Arc<AtomicBool>,
    playback: Option<crate::discovery::Guard>,
    result: Mutex<Attempt>,
    microphone: Arc<crate::microphone::Control>,
    _permit: SnapshotPermit,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn begin() -> Result<RecognitionSession, String> {
    let permit = SnapshotPermit::acquire()?;
    let ((cache, playback, store), lease) = runtime::inspect_with_lease(|core| {
        let playback = core.online.clone().and_then(|online| {
            online
                .sources()
                .iter()
                .any(|source| source.id == "wy")
                .then_some(())?;
            let provider = mio_music_sdk::provider("wy")?;
            Some(crate::discovery::Guard::new(online, provider, "wy".into()))
        });
        (core.paths.cache.clone(), playback, core.store.clone())
    })?;
    let cancelled = playback
        .as_ref()
        .map(|guard| guard.cancelled.clone())
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    let microphone = crate::microphone::Control::new(cancelled.clone())?;
    let source_revision = lease.source_revision();
    let history = Arc::new(Mutex::new(HistoryState::read(&store)));
    lease.ensure_open()?;
    Ok(RecognitionSession {
        session: Session {
            cache,
            history,
            source_revision,
            lease,
            cancelled,
            playback,
            result: Mutex::new(None),
            microphone,
            _permit: permit,
        },
    })
}
impl Session {
    fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()?;
        if self.lease.source_revision() != self.source_revision {
            return Err("source_changed: reopen recognition".into());
        }
        Ok(())
    }
    pub(crate) fn close(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub(crate) fn play(&self, index: u32, jump_to_match: bool) -> Result<(), String> {
        self.check()?;
        let guard = self
            .playback
            .as_ref()
            .ok_or("source_unavailable: enable a NetEase source and reopen recognition")?;
        guard.check(&self.lease)?;
        let row = self
            .result
            .try_lock()
            .map_err(|_| "busy: recognition")?
            .as_ref()
            .and_then(|(_, rows)| rows.as_ref().ok())
            .and_then(|rows| rows.get(index as usize))
            .cloned()
            .ok_or("not_found: recognized song")?;
        self.lease.execute(runtime::Action::PlayRecognition(
            guard.clone(),
            row,
            jump_to_match,
        ))
    }
    pub(crate) fn history(&self) -> Result<RecognitionHistory, String> {
        self.check()?;
        let state = self
            .history
            .try_lock()
            .map_err(|_| "busy: recognition history")?;
        self.check()?;
        state.view()
    }
    fn commit_history(
        &self,
        rows: Option<Vec<mio_core::recognition::Match>>,
        expected: u64,
    ) -> Result<(), String> {
        self.check()?;
        self.lease
            .execute(runtime::Action::RecognitionHistory(HistoryCommit {
                state: self.history.clone(),
                rows,
                expected,
                lease: self.lease.clone(),
                source_revision: self.source_revision,
                cancelled: self.cancelled.clone(),
            }))
    }
    pub(crate) fn clear_history(&self, revision: u64) -> Result<RecognitionHistory, String> {
        self.check()?;
        // A clear cannot race a recognition attempt that is about to append.
        let _attempt = self.result.try_lock().map_err(|_| "busy: recognition")?;
        self.commit_history(None, revision)?;
        self.history
            .lock()
            .map_err(|_| "recognition_history_unreadable")?
            .view()
    }
    pub(crate) fn play_history(&self, index: u32, revision: u64) -> Result<(), String> {
        self.check()?;
        let guard = self.playback.as_ref().ok_or("source_unavailable")?;
        guard.check(&self.lease)?;
        let row = {
            let state = self
                .history
                .try_lock()
                .map_err(|_| "busy: recognition history")?;
            if state.revision != revision {
                return Err("recognition_history_changed".into());
            }
            state
                .snapshot
                .as_ref()
                .ok_or("recognition_history_unreadable")?
                .rows()?
                .get(index as usize)
                .cloned()
                .ok_or("not_found: recognition history")?
        };
        self.lease
            .execute(runtime::Action::PlayRecognition(guard.clone(), row, false))
    }
    pub(crate) fn microphone_token(&self) -> Result<u64, String> {
        self.check()?;
        Ok(self.microphone.token)
    }
    pub(crate) fn microphone_progress(&self) -> MicrophoneProgress {
        self.microphone.view()
    }
    pub(crate) fn recognize_microphone(&self) -> Result<Vec<RecognizedTrack>, String> {
        let mut saved = self.result.try_lock().map_err(|_| "busy: recognition")?;
        self.check()?;
        if let Some((previous, result)) = &*saved {
            return if previous == "microphone" {
                result
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|rows| project(rows))
            } else {
                Err("completed: reopen recognition".into())
            };
        }
        let _active = RecognitionWorker::acquire()?;
        let _worker = Worker::acquire()?;
        let _binding = crate::microphone::Binding::new(self.microphone.clone())?;
        let request = crate::microphone::Request {
            control: self.microphone.clone(),
            lease: self.lease.clone(),
            revision: self.source_revision,
        };
        let result = mio_core::recognition::recognize_microphone_with(
            self.cancelled.clone(),
            self.microphone.progress.clone(),
            move |_| crate::microphone::CaptureLease::acquire(request),
        )
        .map_err(recognition_error);
        self.finish(&mut saved, "microphone".into(), result)
    }
    pub(crate) fn recognize_file(&self, ticket: String) -> Result<Vec<RecognizedTrack>, String> {
        if ticket.len() != 65 {
            return Err("invalid_input: invalid audio selection".into());
        }
        let mut saved = self.result.try_lock().map_err(|_| "busy: recognition")?;
        self.check()?;
        if let Some((previous, result)) = &*saved {
            return if previous == &ticket {
                result
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|rows| project(rows))
            } else {
                Err("completed: reopen recognition".into())
            };
        }
        // Neither cancel nor Dart object disposal releases these gates while a
        // decoder, interpreter or blocking request still owns the input stage.
        let _active = RecognitionWorker::acquire()?;
        let _worker = Worker::acquire()?;
        let _staging = imports::Worker::acquire()?;
        let result = (|| {
            self.check()?;
            let stage = imports::Stage::open(&self.cache, &ticket)?;
            let paths = stage.files()?;
            if paths.len() != 1 {
                return Err("invalid_input: choose one audio file".into());
            }
            self.check()?;
            let recognized = mio_core::recognition::recognize_file(&paths[0].0, &self.cancelled);
            self.check()?;
            let recognized = recognized.map_err(recognition_error)?;
            self.check()?;
            Ok(recognized)
        })();
        self.finish(&mut saved, ticket, result)
    }
    fn finish(
        &self,
        saved: &mut Attempt,
        ticket: String,
        result: Result<Vec<mio_core::recognition::Match>, String>,
    ) -> Result<Vec<RecognizedTrack>, String> {
        if let Ok(rows) = &result {
            if !rows.is_empty() {
                let revision = self
                    .history
                    .lock()
                    .map(|state| state.revision)
                    .map_err(|_| "recognition_history_unreadable".to_owned());
                let commit =
                    revision.and_then(|revision| self.commit_history(Some(rows.clone()), revision));
                if let Err(error) = commit {
                    if let Ok(mut history) = self.history.lock() {
                        history.warning = Some(error);
                    }
                }
            }
        }
        // Both success and failure are remembered; repeating the same intent
        // cannot resend a fingerprint. Closing invalidates read-only results.
        let projected = result
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|rows| project(rows));
        *saved = Some((ticket, result));
        // A history write may outlive close/source changes. Keep its actual
        // outcome and cached attempt, but never return stale read-only rows.
        self.check()?;
        projected
    }
}

fn project(rows: &[mio_core::recognition::Match]) -> Result<Vec<RecognizedTrack>, String> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let raw = row.item.songmid.as_str().ok_or("recognition_invalid")?;
            Ok(RecognizedTrack {
                index: index as u32,
                track: Track {
                    id: mio_core::online::song_id("wy", raw),
                    title: row.item.name.clone(),
                    artist: row.item.singer.clone(),
                    album: row.item.album_name.clone(),
                    duration: 0.0,
                    cover: (!row.item.img.is_empty()).then(|| row.item.img.clone()),
                },
                timestamp: row.timestamp,
                start_time_ms: row.start_time_ms,
            })
        })
        .collect()
}
