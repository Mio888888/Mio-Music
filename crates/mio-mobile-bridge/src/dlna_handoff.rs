//! Original-owner pause lease for a single remote URI handoff.
use crate::{dlna_cast::Playback, runtime::Action};
use mio_core::{app_core::AppCore, dlna::Position, playback::Status};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

static TOKENS: AtomicU64 = AtomicU64::new(1);
#[derive(Clone)]
pub(crate) struct Transfer {
    pub(crate) token: u64,
    snapshot: Playback,
    valid: Arc<AtomicBool>,
}
impl Transfer {
    pub(crate) fn epoch(&self) -> u64 {
        self.snapshot.epoch()
    }
    pub(crate) fn host_changed(&self) -> bool {
        self.snapshot.lease().host_revision() != self.epoch()
    }
    pub(crate) fn retain_focus(&self) -> bool {
        self.valid.load(Ordering::Acquire) && self.snapshot.was_playing()
    }
    pub(crate) fn invalidate(&self) {
        self.valid.store(false, Ordering::Release);
    }
    pub(crate) fn admit(&self, core: &AppCore) -> Result<(), String> {
        self.snapshot.validate(core)
    }
    fn current(&self, core: &AppCore) -> bool {
        self.valid.load(Ordering::Acquire) && self.snapshot.transfer_valid(core)
    }
    pub(crate) fn finish(
        &self,
        core: &mut AppCore,
        position: Option<Position>,
        runtime_id: u64,
    ) -> Result<bool, String> {
        if !self.valid.load(Ordering::Acquire)
            || !self.snapshot.same_owner(core)
            || !matches!(
                core.player.status,
                Status::Paused | Status::Playing | Status::Loading
            )
        {
            return Ok(false);
        }
        // Native check only verifies retained focus. It never requests new focus.
        // Do it before the short Rust gate to avoid owner/main-thread deadlocks.
        if self.snapshot.was_playing() {
            #[cfg(target_os = "android")]
            if let Err(error) = crate::android::prepare_dlna_resume(runtime_id) {
                return if self.host_changed() {
                    Ok(false)
                } else {
                    Err(error)
                };
            }
            #[cfg(not(target_os = "android"))]
            let _ = runtime_id;
        }
        let restored = self.snapshot.lease().with_host_epoch(self.epoch(), || {
            if self.current(core) {
                if let Some(position) = position {
                    if position.duration > 0. {
                        core.player.duration = position.duration;
                    }
                    core.player.seek(position.position);
                }
            }
            // A newer seek/settings revision suppresses adoption but must release
            // our pause. New media intents invalidate the entire lease instead.
            if self.snapshot.was_playing() {
                core.player
                    .engine
                    .as_ref()
                    .ok_or("dlna_restore_failed")?
                    .pause(core.player.generation, false);
            }
            Ok(())
        });
        match restored {
            Ok(()) => Ok(self.snapshot.was_playing()),
            Err(_) if self.host_changed() => Ok(false),
            Err(error) => Err(error),
        }
    }
}

pub(crate) struct Lease {
    transfer: Transfer,
    finished: bool,
}
impl Lease {
    pub(crate) fn begin(snapshot: Playback) -> Result<Self, String> {
        let token = TOKENS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                (value < i64::MAX as u64).then_some(value + 1)
            })
            .map_err(|_| "capacity_exceeded: DLNA handoff")?;
        // Construct cleanup before owner admission, including partial pause failures.
        let lease = Self {
            transfer: Transfer {
                token,
                snapshot,
                valid: Arc::new(AtomicBool::new(true)),
            },
            finished: false,
        };
        lease
            .transfer
            .snapshot
            .lease()
            .execute(Action::BeginDlnaHandoff(lease.transfer.clone()))?;
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            lease.check()?;
            if lease
                .transfer
                .snapshot
                .lease()
                .inspect(|core| core.player.status == Status::Paused)?
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err("dlna_pause_timeout".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(lease)
    }
    pub(crate) fn check(&self) -> Result<(), String> {
        let transfer = self.transfer.clone();
        if self
            .transfer
            .snapshot
            .lease()
            .inspect(move |core| transfer.current(core))?
        {
            Ok(())
        } else {
            Err("dlna_playback_changed".into())
        }
    }
    pub(crate) fn wait(&self, duration: Duration) -> Result<(), String> {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.check()?;
            std::thread::sleep(Duration::from_millis(25));
        }
        self.check()
    }
    pub(crate) fn finish(mut self, position: Option<Position>) -> Result<(), String> {
        let invalid = position.is_some_and(|p| {
            !p.position.is_finite() || p.position < 0. || !p.duration.is_finite() || p.duration < 0.
        });
        let result = self
            .transfer
            .snapshot
            .lease()
            .end_dlna_handoff(self.transfer.token, if invalid { None } else { position });
        if result.is_err() && self.transfer.snapshot.lease().ensure_open().is_ok() {
            self.transfer.snapshot.restore_failed();
        }

        self.finished = true;
        result?;
        if invalid {
            Err("invalid_input: DLNA position".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.finished {
            let result = self
                .transfer
                .snapshot
                .lease()
                .end_dlna_handoff(self.transfer.token, None);
            if result.is_err() && self.transfer.snapshot.lease().ensure_open().is_ok() {
                self.transfer.snapshot.restore_failed();
            }
        }
    }
}
