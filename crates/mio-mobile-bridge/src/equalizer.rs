//! Bounded EQ snapshots and optimistic edits bound to their original runtime.
use crate::{api::*, catalog::SnapshotPermit, runtime};
use mio_core::{
    app_core::{AppCore, EqualizerChange},
    effects::FilterType,
    equalizer::State,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) enum Edit {
    Enabled(bool),
    GlobalGain(f64),
    Band { index: u32, value: EqBand },
    Preset(String),
    SavePreset(String),
    UpdatePreset(String),
    DeletePreset(String),
    ImportConfig(String),
    ImportText { name: String, text: String },
    Reset,
}
pub(crate) struct Session {
    lease: runtime::RuntimeLease,
    closed: Arc<AtomicBool>,
    snapshot: Mutex<(u64, Option<Arc<State>>)>,
    spectrum: crate::spectrum::Sampler,
    _permit: SnapshotPermit,
}
// Validate before cloning legacy/imported state, including fields not sent to Dart.
fn bounded(state: &State) -> Result<(), String> {
    let gains_ok = |g: &mio_core::equalizer::Gains| {
        g.bands.len() <= 10
            && g.global.is_finite()
            && (-24.0..=24.0).contains(&g.global)
            && g.bands.iter().all(|b| {
                b.frequency.is_finite()
                    && (20.0..=20000.0).contains(&b.frequency)
                    && b.gain.is_finite()
                    && (-24.0..=24.0).contains(&b.gain)
                    && b.q.is_finite()
                    && (0.1..=10.0).contains(&b.q)
            })
    };
    if state.current_preset_id.len() > 512
        || !gains_ok(&state.gains)
        || state.user_presets.len() > 100
        || state.logs.len() > 100
        || state.logs.iter().any(|s| s.len() > 4096)
        || state
            .user_presets
            .iter()
            .any(|p| p.id.len() > 512 || p.name.len() > 400 || !gains_ok(&p.gains))
    {
        return Err("limit_exceeded: equalizer configuration".into());
    }
    Ok(())
}
fn project(state: &State, revision: u64) -> EqSnapshot {
    let mut gains = state.gains.clone();
    mio_core::equalizer::fit_bands(&mut gains);
    EqSnapshot {
        revision,
        enabled: state.enabled,
        global_gain: gains.global,
        response: mio_core::equalizer::response_curve(&gains)
            .iter()
            .map(|p| EqResponsePoint {
                frequency: p.frequency,
                db: p.db,
            })
            .collect(),
        reference_rate: mio_core::equalizer::RESPONSE_RATE,
        bands: gains
            .bands
            .into_iter()
            .map(|b| EqBand {
                frequency: b.frequency,
                gain: b.gain,
                q: b.q,
                enabled: b.enabled,
                kind: match b.kind {
                    FilterType::Peak => EqFilter::Peak,
                    FilterType::LowShelf => EqFilter::LowShelf,
                    FilterType::HighShelf => EqFilter::HighShelf,
                    FilterType::LowPass => EqFilter::LowPass,
                    FilterType::HighPass => EqFilter::HighPass,
                    FilterType::Notch => EqFilter::Notch,
                },
            })
            .collect(),
        current_preset_id: state.current_preset_id.clone(),
        presets: mio_core::equalizer::default_presets()
            .iter()
            .chain(state.user_presets.iter())
            .map(|p| EqPreset {
                id: p.id.clone(),
                name: p.name.clone(),
                builtin: p.is_default,
                locale_key: mio_core::equalizer::preset_locale_key(&p.id).map(str::to_owned),
            })
            .collect(),
    }
}
const CONFIG_BYTES: usize = 512 * 1024;
const TEXT_BYTES: usize = 64 * 1024;
fn import_config(text: &str) -> Result<serde_json::Value, String> {
    if text.is_empty() || text.len() > CONFIG_BYTES {
        return Err("limit_exceeded: equalizer file".into());
    }
    let raw: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "invalid_input: equalizer JSON")?;
    let object = raw.as_object().ok_or("invalid_input: equalizer object")?;
    let check_gains = |v: &serde_json::Value| -> Result<(), String> {
        let bands = v
            .as_array()
            .or_else(|| v.get("bands").and_then(|v| v.as_array()));
        if bands.is_some_and(|b| b.len() > 10) {
            return Err("limit_exceeded: equalizer bands".into());
        }
        Ok(())
    };
    if let Some(gains) = object.get("gains") {
        check_gains(gains)?;
    }
    for key in ["userPresets", "presets", "defaultPresets"] {
        if let Some(list) = object.get(key).and_then(|v| v.as_array()) {
            let max = if key == "userPresets" {
                100
            } else if key == "defaultPresets" {
                8
            } else {
                108
            };
            if list.len() > max {
                return Err("limit_exceeded: equalizer presets".into());
            }
            for p in list {
                if let Some(g) = p.get("gains") {
                    check_gains(g)?;
                }
            }
        }
    }
    if object
        .get("logs")
        .and_then(|v| v.as_array())
        .is_some_and(|l| {
            l.len() > 100 || l.iter().any(|v| v.as_str().is_some_and(|s| s.len() > 4096))
        })
    {
        return Err("limit_exceeded: equalizer logs".into());
    }
    bounded(&mio_core::equalizer::hydrate(&raw, true)?)?;
    Ok(raw)
}
fn change(edit: Edit) -> Result<EqualizerChange, String> {
    Ok(match edit {
        Edit::Enabled(v) => EqualizerChange::Enabled(v),
        Edit::GlobalGain(v) => EqualizerChange::GlobalGain(v),
        Edit::Band { index, value: b } => {
            if index >= 10 {
                return Err("invalid_input: band index".into());
            }
            EqualizerChange::Band(
                index as usize,
                mio_core::equalizer::Band {
                    frequency: b.frequency,
                    gain: b.gain,
                    q: b.q,
                    enabled: b.enabled,
                    kind: match b.kind {
                        EqFilter::Peak => FilterType::Peak,
                        EqFilter::LowShelf => FilterType::LowShelf,
                        EqFilter::HighShelf => FilterType::HighShelf,
                        EqFilter::LowPass => FilterType::LowPass,
                        EqFilter::HighPass => FilterType::HighPass,
                        EqFilter::Notch => FilterType::Notch,
                    },
                },
            )
        }
        Edit::Preset(id) | Edit::UpdatePreset(id) | Edit::DeletePreset(id) if id.len() > 512 => {
            return Err("invalid_input: preset id".into());
        }
        Edit::Preset(id) => EqualizerChange::Preset(id),
        Edit::UpdatePreset(id) => EqualizerChange::UpdatePreset(id),
        Edit::DeletePreset(id) => EqualizerChange::DeletePreset(id),
        Edit::SavePreset(name) => {
            if name.len() > 400 || name.trim().is_empty() || name.trim().chars().count() > 100 {
                return Err("invalid_input: preset name".into());
            }
            EqualizerChange::SavePreset(name)
        }
        Edit::ImportConfig(text) => EqualizerChange::ImportConfig(import_config(&text)?),
        Edit::ImportText { name, text } => {
            if name.len() > 400
                || name.trim().is_empty()
                || name.trim().chars().count() > 100
                || text.is_empty()
                || text.len() > TEXT_BYTES
            {
                return Err("invalid_input: AutoEq name or text size".into());
            }
            // Parse off the owner before admitting any persistent change.
            mio_core::equalizer::parse_text_preset(&text).ok_or("invalid_input: AutoEq text")?;
            EqualizerChange::ImportText { name, text }
        }
        Edit::Reset => EqualizerChange::Reset,
    })
}
pub(crate) struct Prepared {
    expected: Arc<State>,
    change: EqualizerChange,
    closed: Arc<AtomicBool>,
    result: Arc<Mutex<Option<State>>>,
}
impl Prepared {
    pub(crate) fn apply(self, core: &mut AppCore) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        if core.equalizer() != self.expected.as_ref() {
            return Err("stale: equalizer changed; reload".into());
        }
        let value = core.change_equalizer(self.change)?;
        *self.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
        Ok(())
    }
}
pub(crate) fn begin() -> Result<EqualizerSession, String> {
    let permit = SnapshotPermit::acquire()?;
    let (_, lease) = runtime::inspect_with_lease(|_| ())?;
    Ok(EqualizerSession {
        session: Session {
            lease,
            closed: Arc::new(AtomicBool::new(false)),
            snapshot: Mutex::new((0, None)),
            spectrum: crate::spectrum::Sampler::new(),
            _permit: permit,
        },
    })
}
impl Session {
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.spectrum.stop();
    }
    fn ensure_open(&self) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        self.lease.ensure_open()
    }
    pub(crate) fn spectrum_active(&self, active: bool) -> Result<(), String> {
        self.spectrum.configure(&self.lease, &self.closed, active)
    }
    pub(crate) fn spectrum(&self) -> Result<SpectrumSnapshot, String> {
        self.ensure_open()?;
        self.spectrum.read(&self.lease, &self.closed)
    }
    pub(crate) fn state(&self) -> Result<EqSnapshot, String> {
        let mut snapshot = self.snapshot.try_lock().map_err(|_| "busy: equalizer")?;
        self.ensure_open()?;
        let state = self.lease.inspect(|core| {
            bounded(core.equalizer())?;
            Ok::<_, String>(core.equalizer().clone())
        })??;
        self.ensure_open()?;
        snapshot.0 = snapshot.0.checked_add(1).ok_or("revision_exhausted")?;
        let value = project(&state, snapshot.0);
        snapshot.1 = Some(Arc::new(state));
        Ok(value)
    }
    pub(crate) fn export(&self, revision: u64) -> Result<String, String> {
        let snapshot = self.snapshot.try_lock().map_err(|_| "busy: equalizer")?;
        self.ensure_open()?;
        if snapshot.0 != revision {
            return Err("stale: equalizer revision".into());
        }
        let expected = snapshot
            .1
            .as_ref()
            .ok_or("not_ready: read equalizer first")?
            .clone();
        let owner_expected = expected.clone();
        self.lease.inspect(move |core| {
            if core.equalizer() == owner_expected.as_ref() {
                Ok(())
            } else {
                Err("stale: equalizer changed")
            }
        })??;
        let text = serde_json::to_string_pretty(&mio_core::equalizer::export_config(&expected))
            .map_err(|_| "internal: equalizer serialization")?;
        if text.len() > CONFIG_BYTES {
            return Err("limit_exceeded: equalizer export".into());
        }
        self.ensure_open()?;
        Ok(text)
    }
    pub(crate) fn edit(&self, revision: u64, edit: Edit) -> Result<EqSnapshot, String> {
        let mut snapshot = self.snapshot.try_lock().map_err(|_| "busy: equalizer")?;
        self.ensure_open()?;
        if snapshot.0 != revision {
            return Err("stale: equalizer revision".into());
        }
        let expected = snapshot
            .1
            .as_ref()
            .ok_or("not_ready: read equalizer first")?
            .clone();
        let next_revision = snapshot.0.checked_add(1).ok_or("revision_exhausted")?;
        let result = Arc::new(Mutex::new(None));
        self.lease.execute(runtime::Action::Equalizer(Prepared {
            expected,
            change: change(edit)?,
            closed: self.closed.clone(),
            result: result.clone(),
        }))?;
        // An admitted write reports its actual result even if the view closes.
        let state = result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .ok_or("internal: equalizer result")?;
        let value = project(&state, next_revision);
        *snapshot = (next_revision, Some(Arc::new(state)));
        Ok(value)
    }
}
