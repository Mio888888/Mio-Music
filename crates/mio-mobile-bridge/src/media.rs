//! Native hosts observe/command the process core independently of Dart views.
use crate::runtime::{self, Action};
use mio_core::{app_core::AppCore, playback::Status};
use std::ffi::{CString, c_char};

#[derive(Clone, Copy)]
pub(crate) enum Event {
    TransientLoss,
    PermanentLoss,
    Gain,
}

#[derive(Default)]
pub(crate) struct Events {
    permanent: bool,
    transient: bool,
    gain: bool,
}
impl Events {
    pub(crate) fn push(&mut self, event: Event) {
        match event {
            Event::PermanentLoss => {
                self.permanent = true;
                self.gain = false;
            }
            Event::TransientLoss => {
                self.transient = true;
                self.gain = false;
            }
            Event::Gain => self.gain = true,
        }
    }
    pub(crate) fn take(&mut self) -> Vec<Event> {
        let mut events = Vec::with_capacity(2);
        if self.permanent {
            events.push(Event::PermanentLoss);
        } else if self.transient {
            events.push(Event::TransientLoss);
        }
        if self.gain {
            events.push(Event::Gain);
        }
        *self = Self::default();
        events
    }
}

struct Capture {
    request: crate::microphone::Request,
    generation: u64,
    song: Option<String>,
    intent: u64,
    resume: bool,
}
pub(crate) struct State {
    capture: Option<Capture>,
    handoff: Option<crate::dlna_handoff::Transfer>,
    pub(crate) allowed: bool,
    desired_playing: bool,
    intent: u64,
    resume: Option<u64>,
}

impl State {
    pub(crate) fn playback_desired(&self) -> bool {
        self.desired_playing
    }
    pub(crate) fn handoff_epoch(&self) -> u64 {
        self.handoff
            .as_ref()
            .filter(|transfer| transfer.retain_focus())
            .map_or(0, |transfer| transfer.epoch())
    }
    pub(crate) fn observe(&mut self, core: &AppCore) {
        if matches!(
            core.player.status,
            Status::Idle | Status::Ended | Status::Failed
        ) {
            self.desired_playing = false;
            self.allowed = false;
            self.resume = None;
        }
    }
    pub(crate) fn new() -> Self {
        Self {
            capture: None,
            handoff: None,
            allowed: !cfg!(any(target_os = "android", target_os = "ios")),
            desired_playing: false,
            intent: 0,
            resume: None,
        }
    }
    pub(crate) fn dispatch(
        &mut self,
        core: &mut AppCore,
        action: Action,
        runtime_id: u64,
    ) -> Result<(), String> {
        self.observe(core);
        match action {
            Action::BeginDlnaHandoff(transfer) => {
                transfer.admit(core)?;
                if self.capture.is_some() || self.handoff.is_some() {
                    return Err("busy: audio handoff".into());
                }
                self.handoff = Some(transfer);
                self.allowed = false;
                self.resume = None;
                return if matches!(core.player.status, Status::Playing | Status::Loading) {
                    core.set_paused(true)
                } else {
                    Ok(())
                };
            }
            Action::EndDlnaHandoff(token, position) => {
                if !self
                    .handoff
                    .as_ref()
                    .is_some_and(|transfer| transfer.token == token)
                {
                    return Ok(());
                }
                let transfer = self.handoff.take().unwrap();
                // A newer explicit playback command may already have set allowed.
                // An invalid old lease must leave that state untouched.
                if transfer.retain_focus() {
                    match transfer.finish(core, position, runtime_id) {
                        Ok(allowed) => {
                            self.allowed = allowed;
                            // A native event can enqueue while End waits for the
                            // main thread, before Host dispatch sees the lease.
                            // Preserve intent for that event's later Gain path.
                            if !allowed && transfer.host_changed() && self.desired_playing {
                                self.resume = Some(self.intent);
                            }
                        }
                        Err(error) => {
                            self.allowed = false;
                            self.desired_playing = false;
                            return Err(error);
                        }
                    }
                } else {
                    transfer.finish(core, position, runtime_id)?;
                }
                return Ok(());
            }
            Action::BeginCapture(request) => {
                request.check()?;
                if self.capture.is_some() || self.handoff.is_some() {
                    return Err("busy: microphone".into());
                }
                self.capture = Some(Capture {
                    request,
                    generation: core.player.generation,
                    song: core.player.song().map(|song| song.id.clone()),
                    intent: self.intent,
                    resume: self.allowed
                        && self.desired_playing
                        && matches!(core.player.status, Status::Playing | Status::Loading),
                });
                self.allowed = false;
                self.resume = None;
                return if matches!(core.player.status, Status::Playing | Status::Loading) {
                    core.set_paused(true)
                } else {
                    Ok(())
                };
            }
            Action::EndCapture(token, restored) => {
                if !self
                    .capture
                    .as_ref()
                    .is_some_and(|capture| capture.request.control.token == token)
                {
                    return Ok(());
                }
                let capture = self.capture.take().unwrap();
                let resume = restored
                    && capture.resume
                    && capture.intent == self.intent
                    && capture.request.lease.ensure_open().is_ok()
                    && capture.request.lease.source_revision() == capture.request.revision
                    && capture.generation == core.player.generation
                    && capture.song == core.player.song().map(|song| song.id.clone())
                    && matches!(
                        core.player.status,
                        Status::Paused | Status::Playing | Status::Loading
                    );
                if resume {
                    // Native preparation may wait for the main thread. Never hold
                    // resume_gate here: native revoke enters it on that thread.
                    prepare_capture_resume(runtime_id, token)?;
                    let allowed = capture
                        .request
                        .control
                        .resume_gate
                        .lock()
                        .map_err(|_| "microphone_restore_failed")?;
                    if *allowed
                        && capture.request.lease.ensure_open().is_ok()
                        && capture.request.lease.source_revision() == capture.request.revision
                    {
                        // The original engine is already loaded; enqueue only the
                        // same-generation resume under the short cancellation gate.
                        // No filesystem write or host call is allowed in this lock.
                        let engine = core
                            .player
                            .engine
                            .as_ref()
                            .ok_or("microphone_restore_failed")?;
                        engine.pause(capture.generation, false);
                        self.allowed = true;
                    }
                }
                return Ok(());
            }
            _ => (),
        }
        let handoff_was_active = self
            .handoff
            .as_ref()
            .is_some_and(|transfer| transfer.retain_focus());
        // A later user/media intent cancels capture and permanently suppresses
        // this lease's automatic resume. Non-playback settings may still work.
        if matches!(
            &action,
            Action::Host(_)
                | Action::Toggle
                | Action::Pause(_)
                | Action::Step(_)
                | Action::PlayIndex(_)
                | Action::PlayLocal(_)
                | Action::PlayLocalAt(..)
                | Action::PlayLocalAllAt(..)
                | Action::PlaySaved(..)
                | Action::PlaySavedSession(..)
                | Action::PlaySavedSelection(..)
                | Action::PlayDiscovery(..)
                | Action::PlayRecognition(..)
                | Action::Queue(..)
                | Action::MergeQueue(..)
                | Action::Seek(_)
                | Action::SeekRelative(_)
                | Action::SeekDetails(..)
                | Action::CommitLocalTags(..)
                | Action::Plugins(..)
                | Action::RestoreBackup(..)
                | Action::Stop
                | Action::Shutdown
        ) {
            let supersedes_handoff = match &action {
                Action::Seek(_)
                | Action::SeekRelative(_)
                | Action::SeekDetails(..)
                | Action::MergeQueue(..) => false,
                Action::Queue(_, operation) => {
                    matches!(operation, crate::queue::Operation::Play(..))
                }
                _ => true,
            };
            if supersedes_handoff {
                if let Some(transfer) = &self.handoff {
                    transfer.invalidate();
                }
            }
            if let Some(capture) = &mut self.capture {
                capture.resume = false;
                capture
                    .request
                    .control
                    .cancelled
                    .store(true, std::sync::atomic::Ordering::Release);
            }
        }
        let action = match action {
            Action::CommitLocalTags(prepared, revision, closed, output) => {
                let (persistence_warning, reopened) =
                    core.commit_local_tag_edit(prepared, revision, || {
                        !closed.load(std::sync::atomic::Ordering::Acquire)
                    })?;
                // Reopening cannot acquire focus or create a new playback intent.
                // Resume only the already authorized desire of this same host owner.
                let playback_warning = reopened
                    && self.allowed
                    && self.desired_playing
                    && core.set_paused(false).is_err();
                *output.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(crate::api::LocalTagSaveResult {
                        persistence_warning,
                        playback_warning,
                    });
                return Ok(());
            }

            Action::Queue(guard, op) => {
                guard.preflight(core, &op)?;
                Action::Queue(guard, op)
            }
            Action::PlaySavedSession(row, guard) => {
                guard.validate(core)?;
                Action::PlaySavedSession(row, guard)
            }
            Action::PlaySavedSelection(rows, guard) => {
                guard.validate(core)?;
                Action::PlaySavedSelection(rows, guard)
            }
            Action::SeekDetails(token, seconds) => {
                token.validate(core)?;
                Action::Seek(seconds)
            }
            Action::PlayLocalAllAt(query, revision, closed) => {
                if closed.load(std::sync::atomic::Ordering::Acquire) {
                    return Err("cancelled".into());
                }
                if core.local_revision() != revision {
                    return Err("catalog_changed: refresh local music".into());
                }
                Action::PlayLocalAllAt(query, revision, closed)
            }
            Action::PlayLocalAt(id, revision) => {
                if core.local_revision() != revision {
                    return Err("catalog_changed: refresh local music".into());
                }
                Action::PlayLocal(id)
            }
            Action::Toggle => Action::Pause(self.desired_playing),
            Action::Host(event) => {
                return match event {
                    Event::TransientLoss => {
                        if self.allowed || handoff_was_active {
                            self.resume = self.desired_playing.then_some(self.intent);
                        }
                        self.allowed = false;
                        core.set_paused(true)
                    }
                    Event::PermanentLoss => {
                        self.resume = None;
                        self.allowed = false;
                        self.desired_playing = false;
                        core.set_paused(true)
                    }
                    Event::Gain => {
                        let resume = self.resume.take() == Some(self.intent) || handoff_was_active;
                        if resume && self.desired_playing {
                            prepare(runtime_id)?;
                            self.allowed = true;
                            core.set_paused(false)
                        } else {
                            Ok(())
                        }
                    }
                };
            }
            action => action,
        };
        // Exhaustive: adding an Action must explicitly classify playback intent.
        // Guard preflight above runs before requesting native audio focus.
        if let Action::PlayDiscovery(guard, _) | Action::PlayRecognition(guard, _, _) = &action {
            guard.validate(core)?;
        }
        let starts = match &action {
            Action::Pause(paused) => !paused,
            Action::Step(_)
            | Action::PlayIndex(_)
            | Action::PlayLocal(_)
            | Action::PlayLocalAt(..)
            | Action::PlayLocalAllAt(..)
            | Action::PlaySaved(..)
            | Action::PlaySavedSession(..)
            | Action::PlaySavedSelection(..)
            | Action::PlayDiscovery(..)
            | Action::PlayRecognition(..) => true,
            Action::Queue(_, operation) => matches!(operation, crate::queue::Operation::Play(..)),
            Action::DlnaPosition(..)
            | Action::BeginDlnaHandoff(..)
            | Action::EndDlnaHandoff(..)
            | Action::BeginCapture(..)
            | Action::EndCapture(..)
            | Action::CycleMode
            | Action::MergeQueue(..)
            | Action::AppendDiscovery(..)
            | Action::AppendLocal(..)
            | Action::AppendSaved(..)
            | Action::Equalizer(..)
            | Action::StorageSettings(..)
            | Action::Background(..)
            | Action::Visuals(..)
            | Action::GeneralSettings(..)
            | Action::LyricStyle(..)
            | Action::Balance(..)
            | Action::Seamless(..)
            | Action::RememberDownload(..)
            | Action::Host(_)
            | Action::Toggle
            | Action::Inspect(_)
            | Action::ImportLocal(..)
            | Action::CommitLocalTags(..)
            | Action::SeekDetails(..)
            | Action::Plugins(..)
            | Action::Seek(_)
            | Action::SeekRelative(_)
            | Action::Volume(_)
            | Action::Appearance(..)
            | Action::Stop
            | Action::Shutdown
            | Action::RestoreBackup(..)
            | Action::SaveS3(..)
            | Action::AutoUpdate(..)
            | Action::SelectSource(..)
            | Action::RecognitionHistory(..) => false,
        };
        let pauses = matches!(
            &action,
            Action::Pause(true) | Action::Stop | Action::Shutdown
        );
        if starts || pauses {
            self.intent = self.intent.wrapping_add(1);
            self.resume = None;
        }
        if starts {
            if let Err(error) = prepare(runtime_id) {
                self.allowed = false;
                self.desired_playing = false;
                let _ = core.set_paused(true);
                return Err(error);
            }
            self.allowed = true;
        }
        if pauses {
            self.desired_playing = false;
            self.allowed = false;
        }
        let result = runtime::apply(core, action);
        if starts {
            self.desired_playing = (result.is_ok() && core.player.current.is_some())
                || matches!(core.player.status, Status::Playing | Status::Loading);
            // A rejected command must never create an automatic resume intent.
        }
        if matches!(
            core.player.status,
            Status::Idle | Status::Ended | Status::Failed
        ) && core.player.current.is_none()
        {
            self.allowed = false;
        }
        result
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(transfer) = &self.handoff {
            transfer.invalidate();
        }
        if let Some(capture) = &self.capture {
            capture
                .request
                .control
                .cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
}

fn prepare_capture_resume(runtime_id: u64, token: u64) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return crate::android::prepare_capture_resume(runtime_id, token);
    }
    #[cfg(target_os = "ios")]
    {
        return crate::microphone::host_operation(runtime_id, token, 4);
    }
    // OHOS has no focus handshake; interruptions arrive from OHAudio instead.
    #[cfg(target_env = "ohos")]
    {
        let _ = (runtime_id, token);
        Ok(())
    }
    #[cfg(not(any(target_os = "android", target_os = "ios", target_env = "ohos")))]
    {
        let _ = (runtime_id, token);
        Err("microphone_unavailable".into())
    }
}

fn prepare(runtime_id: u64) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        return crate::android::prepare_playback(runtime_id);
    }
    #[cfg(target_os = "ios")]
    {
        let prepare = *PREPARE.lock().map_err(|_| "media_host_failed")?;
        if prepare.is_some_and(|f| f(runtime_id) != 0) {
            return Ok(());
        }
        return Err("media_host_unavailable: audio session could not activate".into());
    }
    // Starting playback is where Android regains focus; on OHOS it restarts any
    // renderer a call or another app paused or stopped.
    #[cfg(target_env = "ohos")]
    {
        let _ = runtime_id;
        cpal::ohaudio::restart_interrupted();
        return Ok(());
    }
    #[cfg(not(any(target_os = "android", target_os = "ios", target_env = "ohos")))]
    {
        let _ = runtime_id;
        Ok(())
    }
}

#[cfg(target_os = "ios")]
type Prepare = extern "C" fn(u64) -> i32;
#[cfg(target_os = "ios")]
static PREPARE: std::sync::Mutex<Option<Prepare>> = std::sync::Mutex::new(None);
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn mio_media_register_prepare(callback: Option<Prepare>) -> i32 {
    let Ok(mut slot) = PREPARE.lock() else {
        return 0;
    };
    *slot = callback;
    1
}

pub(crate) fn command(runtime_id: u64, command: u32, value: f64) -> Result<(), String> {
    if let Some(event) = match command {
        8 => Some(Event::TransientLoss),
        9 | 11 => Some(Event::PermanentLoss),
        10 => Some(Event::Gain),
        _ => None,
    } {
        return runtime::enqueue_media_event(runtime_id, event);
    }
    let action = match command {
        1 => Action::Pause(false),
        2 => Action::Pause(true),
        3 => Action::Step(true),
        4 => Action::Step(false),
        5 => Action::Stop,
        6 if value.is_finite() && value >= 0.0 => Action::Seek(value),
        7 => Action::Toggle,
        12 if value.is_finite() && value.abs() <= 600.0 => Action::SeekRelative(value),
        8 => Action::Host(Event::TransientLoss),
        9 | 11 => Action::Host(Event::PermanentLoss),
        10 => Action::Host(Event::Gain),
        _ => return Err("invalid_input: media command".into()),
    };
    runtime::execute_media(runtime_id, action)
}

/// A caller owns this UTF-8 JSON string and must free it with the paired export.
#[unsafe(no_mangle)]
pub extern "C" fn mio_media_snapshot(seen_artwork: u64, lyrics_enabled: u8) -> *mut c_char {
    std::panic::catch_unwind(|| {
        runtime::media_snapshot(seen_artwork, lyrics_enabled != 0)
            .ok()
            .and_then(|text| CString::new(text).ok())
            .map_or(std::ptr::null_mut(), CString::into_raw)
    })
    .unwrap_or(std::ptr::null_mut())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mio_media_free(text: *mut c_char) {
    if !text.is_null() {
        unsafe {
            drop(CString::from_raw(text));
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn mio_media_command(runtime_id: u64, action: u32, value: f64) -> i32 {
    std::panic::catch_unwind(|| command(runtime_id, action, value).is_ok() as i32).unwrap_or(0)
}
