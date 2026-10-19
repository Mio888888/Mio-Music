//! Immutable, bounded download choices. Enqueue/persistence stay off the owner;
//! the preference command and the manager each validate the captured source.
use crate::{api::*, discovery::Guard, runtime};
use mio_core::{app_core::DownloadRequest, download::Enqueued};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

static HANDLES: AtomicUsize = AtomicUsize::new(0);
struct Permit;
impl Permit {
    fn acquire() -> Result<Self, String> {
        HANDLES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 4).then_some(n + 1)
            })
            .map_err(|_| "busy: too many download choices")?;
        Ok(Self)
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        HANDLES.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(crate) struct Choice {
    request: DownloadRequest,
    info: DownloadChoiceInfo,
    lease: runtime::RuntimeLease,
    closed: Arc<AtomicBool>,
    guard: Option<Guard>,
    result: Mutex<Option<(String, Result<DownloadEnqueueResult, String>)>>,
    _permit: Permit,
}
pub(crate) fn begin(
    lease: runtime::RuntimeLease,
    capture: impl FnOnce() -> Result<(DownloadRequest, String), String>,
) -> Result<DownloadChoice, String> {
    begin_inner(lease, None, capture)
}
pub(crate) fn begin_guarded(
    lease: runtime::RuntimeLease,
    guard: Guard,
    capture: impl FnOnce() -> Result<(DownloadRequest, String), String>,
) -> Result<DownloadChoice, String> {
    begin_inner(lease, Some(guard), capture)
}
fn begin_inner(
    lease: runtime::RuntimeLease,
    guard: Option<Guard>,
    capture: impl FnOnce() -> Result<(DownloadRequest, String), String>,
) -> Result<DownloadChoice, String> {
    let permit = Permit::acquire()?;
    let (request, title) = capture()?;
    let mut bytes = title
        .len()
        .checked_add(request.source.len())
        .and_then(|n| n.checked_add(request.remembered.len()))
        .ok_or("capacity_exceeded: download choices")?;
    if request.choices.is_empty() || request.choices.len() > 64 {
        return Err("capacity_exceeded: download choices".into());
    }
    for row in &request.choices {
        bytes = bytes
            .checked_add(row.quality.len())
            .and_then(|n| n.checked_add(row.size.as_ref().map_or(0, String::len)))
            .and_then(|n| {
                n.checked_add(mio_core::quality::display_key(&row.quality).map_or(0, str::len))
            })
            .ok_or("capacity_exceeded: download choices")?;
    }
    if bytes > 64 * 1024 {
        return Err("capacity_exceeded: download choices".into());
    }
    let info = DownloadChoiceInfo {
        title,
        source: request.source.clone(),
        remembered: request.remembered.clone(),
        choices: request
            .choices
            .iter()
            .map(|row| DownloadQuality {
                quality: row.quality.clone(),
                label_key: mio_core::quality::display_key(&row.quality).map(str::to_owned),
                size: row.size.clone(),
            })
            .collect(),
    };
    lease.ensure_open()?;
    Ok(DownloadChoice {
        choice: Choice {
            request,
            info,
            lease,
            closed: Arc::new(AtomicBool::new(false)),
            guard,
            result: Mutex::new(None),
            _permit: permit,
        },
    })
}
impl Choice {
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
    fn ensure_open(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()?;
        if let Some(guard) = &self.guard {
            guard.check(&self.lease)?;
        }
        Ok(())
    }
    pub(crate) fn info(&self) -> Result<DownloadChoiceInfo, String> {
        self.ensure_open()?;
        Ok(self.info.clone())
    }
    pub(crate) fn enqueue(&self, quality: String) -> Result<DownloadEnqueueResult, String> {
        let mut saved = self
            .result
            .try_lock()
            .map_err(|_| "busy: download choice in progress")?;
        if let Some((previous, result)) = saved.as_ref() {
            return if previous == &quality {
                result.clone()
            } else {
                Err("already_submitted: reopen download choice".into())
            };
        }
        self.ensure_open()?;
        if !self
            .request
            .choices
            .iter()
            .any(|choice| choice.quality == quality)
        {
            return Err("invalid_input: download quality unavailable".into());
        }
        self.lease.execute(runtime::Action::RememberDownload(
            self.request.clone(),
            quality.clone(),
            self.closed.clone(),
            self.guard.clone(),
        ))?;
        // The remembered preference may already be saved if the view closes
        // here. The enqueue has its own cancellation/admission point.
        self.ensure_open()?;
        let result = self
            .request
            .enqueue(&quality)
            .map(|result| match result {
                Enqueued::Added => DownloadEnqueueResult::Added,
                Enqueued::Duplicate => DownloadEnqueueResult::Duplicate,
                Enqueued::AlreadyDone => DownloadEnqueueResult::AlreadyDone,
            })
            .map_err(|_| "download_unavailable: close and reopen before retry".to_owned());
        *saved = Some((quality, result.clone()));
        // No late validation: report the actual accepted result, even if this
        // view/runtime closed during enqueue. Repeating never enqueues twice.
        result
    }
}
impl Drop for Choice {
    fn drop(&mut self) {
        self.close();
    }
}
