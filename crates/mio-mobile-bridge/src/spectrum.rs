//! One bounded FFT worker at a time, bound to the original owner and generation.
use crate::{api::SpectrumSnapshot, runtime::RuntimeLease};
use mio_core::{effects, playback::Status};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
static FFT_WORKER: Mutex<Option<Instant>> = Mutex::new(None);
struct Capture {
    revision: u64,
    demand: Option<effects::SpectrumDemand>,
}
pub(crate) struct Sampler {
    capture: Mutex<Capture>,
    last: Mutex<Option<Instant>>,
}
impl Sampler {
    pub(crate) fn new() -> Self {
        Self {
            capture: Mutex::new(Capture {
                revision: 0,
                demand: None,
            }),
            last: Mutex::new(None),
        }
    }
    pub(crate) fn configure(
        &self,
        lease: &RuntimeLease,
        closed: &AtomicBool,
        active: bool,
    ) -> Result<(), String> {
        let mut capture = self.capture.lock().map_err(|_| "spectrum_unavailable")?;
        if active {
            ensure_open(lease, closed)?;
            if capture.demand.is_none() {
                capture.demand = Some(effects::shared().spectrum_demand(lease.runtime_id())?);
                capture.revision = capture.revision.wrapping_add(1);
            }
        } else {
            capture.demand = None;
            capture.revision = capture.revision.wrapping_add(1);
        }
        Ok(())
    }
    pub(crate) fn stop(&self) {
        let mut capture = self.capture.lock().unwrap_or_else(|e| e.into_inner());
        capture.demand = None;
        capture.revision = capture.revision.wrapping_add(1);
    }
    fn revision(&self) -> Result<u64, String> {
        let capture = self.capture.lock().map_err(|_| "spectrum_unavailable")?;
        if capture.demand.is_none() {
            return Err("cancelled: spectrum inactive".into());
        }
        Ok(capture.revision)
    }
    pub(crate) fn read(
        &self,
        lease: &RuntimeLease,
        closed: &AtomicBool,
    ) -> Result<SpectrumSnapshot, String> {
        let revision = self.revision()?;
        let mut last = self.last.try_lock().map_err(|_| "busy: spectrum read")?;
        ensure_open(lease, closed)?;
        if last.is_some_and(|at| at.elapsed() < Duration::from_millis(33)) {
            return Err("busy: spectrum rate limit".into());
        }
        let mut worker = FFT_WORKER.try_lock().map_err(|_| "busy: spectrum worker")?;
        if worker.is_some_and(|at| at.elapsed() < Duration::from_millis(33)) {
            return Err("busy: spectrum global rate limit".into());
        }
        let started = Instant::now();
        *last = Some(started);
        *worker = Some(started);
        let (generation, playing, frame) = lease.inspect(|core| {
            let playing = core.player.status == Status::Playing;
            (
                core.player.generation,
                playing,
                if playing {
                    effects::shared().take_spectrum_frame()
                } else {
                    None
                },
            )
        })?;
        ensure_open(lease, closed)?;
        let mut fresh = false;
        let mut completed_at = None;
        let mut levels = vec![-80.0; effects::SPECTRUM_BANDS];
        if let Some(frame) = frame {
            if frame.is_fresh() && frame.generation() == generation {
                completed_at = Some(frame.completed_at());
                levels = effects::shared().analyze_spectrum(frame);
                fresh = true;
            }
        }
        if self.revision()? != revision {
            return Err("cancelled: spectrum visibility changed".into());
        }
        // Reading/FFT can cross a track switch, pause, shutdown or stale block.
        // Reject that result instead of presenting a later runtime's samples.
        let (current_generation, current_playing) = lease.inspect(|core| {
            (
                core.player.generation,
                core.player.status == Status::Playing,
            )
        })?;
        ensure_open(lease, closed)?;
        if current_generation != generation || !current_playing || !playing {
            return Ok(SpectrumSnapshot {
                age_ms: 201,
                generation: current_generation,
                playing: current_playing,
                fresh: false,
                levels: vec![-80.0; effects::SPECTRUM_BANDS],
            });
        }
        if completed_at.is_none_or(|at| at.elapsed() > Duration::from_millis(200)) {
            fresh = false;
            levels.fill(-80.0);
        }
        for level in &mut levels {
            *level = if level.is_finite() {
                level.clamp(-80.0, 24.0)
            } else {
                -80.0
            };
        }
        if self.revision()? != revision {
            return Err("cancelled: spectrum visibility changed".into());
        }
        Ok(SpectrumSnapshot {
            age_ms: completed_at
                .map(|at| at.elapsed().as_millis().min(u32::MAX as u128) as u32)
                .unwrap_or(201),
            generation,
            playing,
            fresh,
            levels,
        })
    }
}
fn ensure_open(lease: &RuntimeLease, closed: &AtomicBool) -> Result<(), String> {
    if closed.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    lease.ensure_open()
}

/// Full-player sampler owns its original runtime and track generation.
pub(crate) struct Session {
    lease: RuntimeLease,
    generation: u64,
    closed: AtomicBool,
    sampler: Sampler,
    _permit: crate::catalog::SnapshotPermit,
}
impl Session {
    pub(crate) fn begin(generation: u64) -> Result<Self, String> {
        let permit = crate::catalog::SnapshotPermit::acquire()?;
        let (current, lease) = crate::runtime::inspect_with_lease(|core| core.player.generation)?;
        if current != generation {
            return Err("player_changed".into());
        }
        let closed = AtomicBool::new(false);
        let sampler = Sampler::new();
        sampler.configure(&lease, &closed, true)?;
        Ok(Self {
            lease,
            generation,
            closed,
            sampler,
            _permit: permit,
        })
    }
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.sampler.stop();
    }
    pub(crate) fn frame(&self) -> Result<SpectrumSnapshot, String> {
        ensure_open(&self.lease, &self.closed)?;
        let allowed = self
            .lease
            .inspect(|core| (core.player.generation, core.preferences.audio_visualization))?;
        if allowed != (self.generation, true) {
            return Err("player_changed".into());
        }
        let frame = self.sampler.read(&self.lease, &self.closed)?;
        if frame.generation != self.generation {
            return Err("player_changed".into());
        }
        Ok(frame)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
