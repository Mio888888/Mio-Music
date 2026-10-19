//! The audio effects of the original's `player/effects.rs` and `player/spectrum.rs`: a ten-band
//! parametric equalizer with a global gain, a channel balance and the spectrum tap, as iterator
//! adapters around the decoded samples. The chain order is the original's:
//! decoder → EQ → balance → spectrum.
//!
//! The parameters live in atomics shared by every track, so changing a setting is audible at once
//! and survives track changes; the audio thread never takes a lock for them.
use rodio::Source;
use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

pub const NUM_EQ_BANDS: usize = 10;
pub const EQ_GAIN_MIN: f64 = -24.;
pub const EQ_GAIN_MAX: f64 = 24.;
pub const EQ_FREQ_MIN: f64 = 20.;
pub const EQ_FREQ_MAX: f64 = 20000.;
pub const EQ_Q_MIN: f64 = 0.1;
pub const EQ_Q_MAX: f64 = 10.;
pub const EQ_FREQS: [f64; NUM_EQ_BANDS] = [
    32., 64., 125., 250., 500., 1000., 2000., 4000., 8000., 16000.,
];

pub const SPECTRUM_BANDS: usize = 128;
const FFT_SIZE: usize = 1024;
/// How often (in frames) the audio thread looks at the shared parameters.
const REFRESH_FRAMES: usize = 64;

pub fn clamp_gain(gain: f64) -> f64 {
    if gain.is_finite() {
        gain.clamp(EQ_GAIN_MIN, EQ_GAIN_MAX)
    } else {
        0.
    }
}
pub fn clamp_freq(freq: f64) -> f64 {
    if freq.is_finite() {
        freq.clamp(EQ_FREQ_MIN, EQ_FREQ_MAX)
    } else {
        1000.
    }
}
pub fn clamp_q(q: f64) -> f64 {
    if q.is_finite() {
        q.clamp(EQ_Q_MIN, EQ_Q_MAX)
    } else {
        1.
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterType {
    Peak,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    Notch,
}
pub const FILTER_TYPES: [FilterType; 6] = [
    FilterType::Peak,
    FilterType::LowShelf,
    FilterType::HighShelf,
    FilterType::LowPass,
    FilterType::HighPass,
    FilterType::Notch,
];
impl FilterType {
    /// `getDefaultQ` of the original's store.
    pub fn default_q(self) -> f64 {
        match self {
            Self::LowShelf | Self::HighShelf => 0.7,
            _ => 1.414,
        }
    }
    /// LP, HP and notch have no gain.
    pub fn has_gain(self) -> bool {
        !matches!(self, Self::LowPass | Self::HighPass | Self::Notch)
    }
    fn index(self) -> u32 {
        FILTER_TYPES.iter().position(|t| *t == self).unwrap_or(0) as u32
    }
    fn from_index(index: u32) -> Self {
        FILTER_TYPES
            .get(index as usize)
            .copied()
            .unwrap_or(Self::Peak)
    }
    /// The locale key of its name.
    pub fn locale_key(self) -> &'static str {
        match self {
            Self::Peak => "settings.equalizer.typePeak",
            Self::LowShelf => "settings.equalizer.typeLowShelf",
            Self::HighShelf => "settings.equalizer.typeHighShelf",
            Self::LowPass => "settings.equalizer.typeLowPass",
            Self::HighPass => "settings.equalizer.typeHighPass",
            Self::Notch => "settings.equalizer.typeNotch",
        }
    }
}

/// Normalised biquad coefficients (`a0` divided out).
#[derive(Clone, Copy, Debug)]
pub struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}
impl Coeffs {
    pub const IDENTITY: Coeffs = Coeffs {
        b0: 1.,
        b1: 0.,
        b2: 0.,
        a1: 0.,
        a2: 0.,
    };

    /// The RBJ cookbook filters of the original (`effects.rs` and the settings page's
    /// `calcBiquadCoeffs`, which share these formulas).
    pub fn new(kind: FilterType, sample_rate: f64, freq: f64, gain_db: f64, q: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.);
        let omega = 2. * std::f64::consts::PI * freq / sample_rate;
        let (sin_w, cos_w) = omega.sin_cos();
        let alpha = sin_w / (2. * q);
        let sqrt_a = a.sqrt();
        let (b0, b1, b2, a0, a1, a2) = match kind {
            FilterType::Peak => (
                1. + alpha * a,
                -2. * cos_w,
                1. - alpha * a,
                1. + alpha / a,
                -2. * cos_w,
                1. - alpha / a,
            ),
            FilterType::LowShelf => (
                a * ((a + 1.) - (a - 1.) * cos_w + 2. * sqrt_a * alpha),
                2. * a * ((a - 1.) - (a + 1.) * cos_w),
                a * ((a + 1.) - (a - 1.) * cos_w - 2. * sqrt_a * alpha),
                (a + 1.) + (a - 1.) * cos_w + 2. * sqrt_a * alpha,
                -2. * ((a - 1.) + (a + 1.) * cos_w),
                (a + 1.) + (a - 1.) * cos_w - 2. * sqrt_a * alpha,
            ),
            FilterType::HighShelf => (
                a * ((a + 1.) + (a - 1.) * cos_w + 2. * sqrt_a * alpha),
                -2. * a * ((a - 1.) + (a + 1.) * cos_w),
                a * ((a + 1.) + (a - 1.) * cos_w - 2. * sqrt_a * alpha),
                (a + 1.) - (a - 1.) * cos_w + 2. * sqrt_a * alpha,
                2. * ((a - 1.) - (a + 1.) * cos_w),
                (a + 1.) - (a - 1.) * cos_w - 2. * sqrt_a * alpha,
            ),
            FilterType::LowPass => (
                (1. - cos_w) / 2.,
                1. - cos_w,
                (1. - cos_w) / 2.,
                1. + alpha,
                -2. * cos_w,
                1. - alpha,
            ),
            FilterType::HighPass => (
                (1. + cos_w) / 2.,
                -(1. + cos_w),
                (1. + cos_w) / 2.,
                1. + alpha,
                -2. * cos_w,
                1. - alpha,
            ),
            FilterType::Notch => (1., -2. * cos_w, 1., 1. + alpha, -2. * cos_w, 1. - alpha),
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Magnitude of the response at `freq`, in dB (`calcFilterResponseDb`).
    pub fn response_db(&self, freq: f64, sample_rate: f64) -> f64 {
        let omega = 2. * std::f64::consts::PI * freq / sample_rate;
        let (sin_w, cos_w) = omega.sin_cos();
        let cos_2w = 2. * cos_w * cos_w - 1.;
        let real_num = self.b0 + self.b1 * cos_w + self.b2 * cos_2w;
        let imag_num = -(self.b1 * sin_w + self.b2 * 2. * sin_w * cos_w);
        let real_den = 1. + self.a1 * cos_w + self.a2 * cos_2w;
        let imag_den = -(self.a1 * sin_w + self.a2 * 2. * sin_w * cos_w);
        let num = (real_num * real_num + imag_num * imag_num).sqrt();
        let den = (real_den * real_den + imag_den * imag_den).sqrt();
        20. * (num / den.max(1e-10)).log10()
    }
}

#[derive(Clone, Copy)]
struct Biquad {
    c: Coeffs,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}
impl Biquad {
    fn new() -> Self {
        Self {
            c: Coeffs::IDENTITY,
            x1: 0.,
            x2: 0.,
            y1: 0.,
            y2: 0.,
        }
    }
    fn run(&mut self, x: f64) -> f64 {
        let c = &self.c;
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

fn load_f64(value: &AtomicU64) -> f64 {
    f64::from_bits(value.load(Ordering::Relaxed))
}
fn store_f64(value: &AtomicU64, v: f64) {
    value.store(v.to_bits(), Ordering::Relaxed)
}

/// One band as the audio thread should apply it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandParams {
    pub kind: FilterType,
    pub frequency: f64,
    pub gain: f64,
    pub q: f64,
    /// A band that is off, or the whole equalizer being off, passes the signal untouched.
    pub active: bool,
}

/// Everything the effects chain reads, shared by all tracks.
pub struct Effects {
    gains: [AtomicU64; NUM_EQ_BANDS],
    freqs: [AtomicU64; NUM_EQ_BANDS],
    qs: [AtomicU64; NUM_EQ_BANDS],
    kinds: [AtomicU32; NUM_EQ_BANDS],
    active: [AtomicU32; NUM_EQ_BANDS],
    global_gain: AtomicU64,
    balance: AtomicU64,
    spectrum: Mutex<SpectrumBuffer>,
    spectrum_producer: AtomicU64,
    spectrum_capture: AtomicBool,
    fft: Arc<dyn rustfft::Fft<f64>>,
    window: Vec<f64>,
}

static EFFECTS: OnceLock<Arc<Effects>> = OnceLock::new();

/// The process-wide effects state: the audio worker reads it, the settings page writes it.
pub fn shared() -> Arc<Effects> {
    EFFECTS.get_or_init(|| Arc::new(Effects::new())).clone()
}

/// Immutable samples plus their actual completion time, never external paths.
pub struct SpectrumFrame {
    data: Vec<f32>,
    completed_at: std::time::Instant,
    generation: u64,
}
impl SpectrumFrame {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn completed_at(&self) -> std::time::Instant {
        self.completed_at
    }
    pub fn is_fresh(&self) -> bool {
        self.completed_at.elapsed() <= std::time::Duration::from_millis(200)
    }
}

struct SpectrumBuffer {
    capture_owner: Option<u64>,
    capture_count: usize,
    generation: u64,
    data: Vec<f32>,
    completed: Vec<f32>,
    write_pos: usize,
    ready: bool,
    completed_at: Option<std::time::Instant>,
}

/// Process runtime owns the capture epoch; stale UI handles cannot enable its successor.
pub struct SpectrumOwner {
    effects: Arc<Effects>,
    id: u64,
}
pub struct SpectrumDemand {
    effects: Arc<Effects>,
    owner: u64,
}
fn reset_spectrum(buffer: &mut SpectrumBuffer) {
    buffer.write_pos = 0;
    buffer.ready = false;
    buffer.completed_at = None;
}
impl Drop for SpectrumOwner {
    fn drop(&mut self) {
        let mut buffer = self
            .effects
            .spectrum
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if buffer.capture_owner == Some(self.id) {
            buffer.capture_owner = None;
            buffer.capture_count = 0;
            self.effects
                .spectrum_capture
                .store(false, Ordering::Release);
            reset_spectrum(&mut buffer);
        }
    }
}
impl Drop for SpectrumDemand {
    fn drop(&mut self) {
        let mut buffer = self
            .effects
            .spectrum
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if buffer.capture_owner == Some(self.owner) {
            buffer.capture_count -= 1;
            if buffer.capture_count == 0 {
                self.effects
                    .spectrum_capture
                    .store(false, Ordering::Release);
                reset_spectrum(&mut buffer);
            }
        }
    }
}
impl Effects {
    pub fn spectrum_owner(self: &Arc<Self>, id: u64) -> Result<SpectrumOwner, String> {
        let mut buffer = self.spectrum.lock().map_err(|_| "spectrum_unavailable")?;
        if buffer.capture_owner.is_some() {
            return Err("busy: spectrum owner".into());
        }
        buffer.capture_owner = Some(id);
        buffer.capture_count = 0;
        self.spectrum_capture.store(false, Ordering::Release);
        reset_spectrum(&mut buffer);
        Ok(SpectrumOwner {
            effects: self.clone(),
            id,
        })
    }
    /// Desktop (no capture owner): capture samples only while a spectrum is on screen.
    pub fn set_unowned_capture(&self, enabled: bool) {
        if self.spectrum_capture.load(Ordering::Relaxed) == enabled {
            return;
        }
        if let Ok(mut buffer) = self.spectrum.lock()
            && buffer.capture_owner.is_none()
        {
            reset_spectrum(&mut buffer);
            self.spectrum_capture.store(enabled, Ordering::Release);
        }
    }
    pub fn spectrum_demand(self: &Arc<Self>, owner: u64) -> Result<SpectrumDemand, String> {
        let mut buffer = self.spectrum.lock().map_err(|_| "spectrum_unavailable")?;
        if buffer.capture_owner != Some(owner) {
            return Err("runtime_closed".into());
        }
        if buffer.capture_count >= 32 {
            return Err("capacity_exceeded: spectrum demand".into());
        }
        if buffer.capture_count == 0 {
            reset_spectrum(&mut buffer);
        }
        buffer.capture_count += 1;
        self.spectrum_capture.store(true, Ordering::Release);
        Ok(SpectrumDemand {
            effects: self.clone(),
            owner,
        })
    }
    fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        let window = (0..FFT_SIZE)
            .map(|i| 0.5 * (1. - (2. * std::f64::consts::PI * i as f64 / FFT_SIZE as f64).cos()))
            .collect();
        Self {
            gains: std::array::from_fn(|_| AtomicU64::new(0f64.to_bits())),
            freqs: std::array::from_fn(|i| AtomicU64::new(EQ_FREQS[i].to_bits())),
            qs: std::array::from_fn(|_| AtomicU64::new(1.414f64.to_bits())),
            kinds: std::array::from_fn(|_| AtomicU32::new(FilterType::Peak.index())),
            active: std::array::from_fn(|_| AtomicU32::new(0)),
            global_gain: AtomicU64::new(0f64.to_bits()),
            balance: AtomicU64::new(0f64.to_bits()),
            spectrum_producer: AtomicU64::new(0),
            // Desktop keeps its existing capture behavior until a mobile runtime opts in.
            spectrum_capture: AtomicBool::new(true),
            spectrum: Mutex::new(SpectrumBuffer {
                capture_owner: None,
                capture_count: 0,
                generation: 0,
                data: vec![0.; FFT_SIZE],
                completed: vec![0.; FFT_SIZE],
                write_pos: 0,
                ready: false,
                completed_at: None,
            }),
            fft,
            window,
        }
    }

    /// `set_eq_settings`: the whole equalizer at once. `global_gain` and every band's gain are
    /// applied only while `enabled`.
    pub fn set_equalizer(&self, enabled: bool, global_gain: f64, bands: &[BandParams]) {
        store_f64(
            &self.global_gain,
            if enabled { clamp_gain(global_gain) } else { 0. },
        );
        for (index, band) in bands.iter().take(NUM_EQ_BANDS).enumerate() {
            store_f64(&self.gains[index], clamp_gain(band.gain));
            store_f64(&self.freqs[index], clamp_freq(band.frequency));
            store_f64(&self.qs[index], clamp_q(band.q));
            self.kinds[index].store(band.kind.index(), Ordering::Relaxed);
            self.active[index].store(u32::from(enabled && band.active), Ordering::Relaxed);
        }
    }

    /// `player__set_balance`: -1 (left only) to 1 (right only).
    pub fn set_balance(&self, value: f64) {
        store_f64(
            &self.balance,
            if value.is_finite() {
                value.clamp(-1., 1.)
            } else {
                0.
            },
        );
    }

    /// Capture a bounded block under the short buffer lock. FFT can run later
    /// outside the application's owner, without reading a replacement runtime.
    pub fn take_spectrum_frame(&self) -> Option<SpectrumFrame> {
        let mut buffer = self.spectrum.lock().ok()?;
        let completed_at = buffer.completed_at?;
        if !buffer.ready || completed_at.elapsed() > std::time::Duration::from_millis(200) {
            return None;
        }
        buffer.ready = false;
        Some(SpectrumFrame {
            data: buffer.completed.clone(),
            completed_at,
            generation: buffer.generation,
        })
    }

    /// 128 band levels in dB, when a fresh block completed since the last call.
    /// Retains the desktop's consume-once behavior and FFT formulas.
    pub fn take_spectrum(&self) -> Option<Vec<f64>> {
        let frame = self.take_spectrum_frame()?;
        Some(self.analyze_spectrum(frame))
    }

    pub fn analyze_spectrum(&self, frame: SpectrumFrame) -> Vec<f64> {
        let data = frame.data;
        let mut input: Vec<Complex<f64>> = data
            .iter()
            .zip(&self.window)
            .map(|(sample, window)| Complex::new(f64::from(*sample) * window, 0.))
            .collect();
        self.fft.process(&mut input);
        let start = |band: usize| {
            let max_bin = FFT_SIZE / 2 - 1;
            let t = band as f64 / SPECTRUM_BANDS as f64;
            (t.powf(1.6) * max_bin as f64).round() as usize
        };
        (0..SPECTRUM_BANDS)
            .map(|band| {
                let lo = start(band);
                let hi = start(band + 1).max(lo + 1);
                let mut max_sq = 0f64;
                for i in lo..hi {
                    if i + 1 >= input.len() {
                        break;
                    }
                    let c = &input[i + 1];
                    max_sq = max_sq.max(c.re * c.re + c.im * c.im);
                }
                10. * (max_sq / FFT_SIZE as f64 + 1e-10).log10()
            })
            .collect()
    }
}

/// The equalizer stage; the filters live here, only the parameters are shared.
pub struct EqSource<S> {
    inner: S,
    effects: Arc<Effects>,
    filters: Vec<[Biquad; NUM_EQ_BANDS]>,
    cached: [(f64, f64, f64, u32, bool); NUM_EQ_BANDS],
    gain_factor: f64,
    /// False while every band passes the signal untouched (the common case).
    any_active: bool,
    channel: usize,
    until_refresh: usize,
}
impl<S: Source<Item = f32>> EqSource<S> {
    pub fn new(inner: S, effects: Arc<Effects>) -> Self {
        let channels = usize::from(inner.channels().max(1));
        Self {
            inner,
            effects,
            filters: (0..channels)
                .map(|_| [Biquad::new(); NUM_EQ_BANDS])
                .collect(),
            cached: [(f64::NAN, 0., 0., 0, false); NUM_EQ_BANDS],
            gain_factor: 1.,
            any_active: false,
            channel: 0,
            until_refresh: 0,
        }
    }
    fn refresh(&mut self) {
        let rate = f64::from(self.inner.sample_rate());
        for band in 0..NUM_EQ_BANDS {
            let state = (
                load_f64(&self.effects.gains[band]),
                load_f64(&self.effects.freqs[band]),
                load_f64(&self.effects.qs[band]),
                self.effects.kinds[band].load(Ordering::Relaxed),
                self.effects.active[band].load(Ordering::Relaxed) != 0,
            );
            if state == self.cached[band] || state.0.is_nan() {
                continue;
            }
            self.cached[band] = state;
            // Coefficients change, the filter memory stays: no click when a slider moves.
            let coeffs = if state.4 {
                Coeffs::new(
                    FilterType::from_index(state.3),
                    rate,
                    state.1,
                    state.0,
                    state.2,
                )
            } else {
                Coeffs::IDENTITY
            };
            for channel in &mut self.filters {
                channel[band].c = coeffs;
            }
        }
        let global = load_f64(&self.effects.global_gain);
        self.gain_factor = if global.abs() > 0.001 {
            10f64.powf(clamp_gain(global) / 20.)
        } else {
            1.
        };
        self.any_active = self.cached.iter().any(|band| band.4);
    }
}
impl<S: Source<Item = f32>> Iterator for EqSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let channels = self.filters.len();
        if self.until_refresh == 0 {
            self.refresh();
            self.until_refresh = REFRESH_FRAMES * channels;
        }
        self.until_refresh -= 1;
        let channel = self.channel;
        self.channel = if channel + 1 == channels {
            0
        } else {
            channel + 1
        };
        if !self.any_active && self.gain_factor == 1. {
            return Some(sample);
        }
        let mut value = f64::from(sample);
        for (filter, cached) in self.filters[channel].iter_mut().zip(&self.cached) {
            if cached.4 {
                value = filter.run(value);
            }
        }
        Some((value * self.gain_factor) as f32)
    }
}
impl<S: Source<Item = f32>> Source for EqSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

/// Left/right balance: the louder side stays, the other is reduced (`BalanceSource`).
pub struct BalanceSource<S> {
    inner: S,
    effects: Arc<Effects>,
    channels: usize,
    channel: usize,
}
impl<S: Source<Item = f32>> BalanceSource<S> {
    pub fn new(inner: S, effects: Arc<Effects>) -> Self {
        let channels = usize::from(inner.channels().max(1));
        Self {
            inner,
            effects,
            channels,
            channel: 0,
        }
    }
}
impl<S: Source<Item = f32>> Iterator for BalanceSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let channel = self.channel;
        self.channel = if channel + 1 == self.channels {
            0
        } else {
            channel + 1
        };
        // Mono has no opposite side; other surround channels remain untouched.
        if self.channels < 2 || channel > 1 {
            return Some(sample);
        }
        let balance = load_f64(&self.effects.balance);
        // Negative favors left (channel 0), positive favors right (channel 1).
        Some(if channel == 0 && balance > 0. {
            (f64::from(sample) * (1. - balance)) as f32
        } else if channel == 1 && balance < 0. {
            (f64::from(sample) * (1. + balance)) as f32
        } else {
            sample
        })
    }
}
impl<S: Source<Item = f32>> Source for BalanceSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

/// Copies the first channel into the analyser's block (`SpectrumSource`).
pub struct SpectrumSource<S> {
    inner: S,
    effects: Arc<Effects>,
    channels: usize,
    sample_count: usize,
    stride: usize,
    stride_counter: usize,
    producer: u64,
    generation: Arc<AtomicU64>,
}
impl<S: Source<Item = f32>> SpectrumSource<S> {
    pub fn new(inner: S, effects: Arc<Effects>, generation: Arc<AtomicU64>) -> Self {
        let producer;
        if let Ok(mut buffer) = effects.spectrum.lock() {
            producer = effects.spectrum_producer.fetch_add(1, Ordering::SeqCst) + 1;
            buffer.generation = generation.load(Ordering::SeqCst);
            buffer.write_pos = 0;
            buffer.ready = false;
            buffer.completed_at = None;
        } else {
            producer = 0; // Poisoned buffers cannot publish samples.
        }
        let channels = usize::from(inner.channels().max(1));
        let per_fill = inner.sample_rate() as usize / 30;
        let stride = if per_fill > FFT_SIZE {
            per_fill / FFT_SIZE
        } else {
            1
        };
        Self {
            inner,
            effects,
            channels,
            sample_count: 0,
            stride,
            stride_counter: 0,
            producer,
            generation,
        }
    }
}
impl<S: Source<Item = f32>> Iterator for SpectrumSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        // Only `Continuous`, on this same audio thread, advances the generation.
        let before = self.generation.load(Ordering::Relaxed);
        let sample = self.inner.next()?;
        self.sample_count += 1;
        if !self.effects.spectrum_capture.load(Ordering::Acquire) {
            return Some(sample);
        }
        // The last crossfade sample may include the outgoing lane. Do not
        // publish that boundary sample under the incoming generation.
        if self.generation.load(Ordering::Relaxed) != before {
            return Some(sample);
        }
        if (self.sample_count - 1) % self.channels == 0 {
            self.stride_counter += 1;
            if self.stride_counter >= self.stride {
                self.stride_counter = 0;
                // Never wait for the reader: a missed sample does not matter here.
                if let Ok(mut buffer) = self.effects.spectrum.try_lock() {
                    // Release of the last demand shares this gate with writes.
                    if !self.effects.spectrum_capture.load(Ordering::Acquire) {
                        return Some(sample);
                    }
                    if self.producer == 0
                        || self.producer != self.effects.spectrum_producer.load(Ordering::SeqCst)
                    {
                        return Some(sample);
                    }
                    // Read after inner.next: Continuous may have swapped lanes during it.
                    let generation = self.generation.load(Ordering::Relaxed);
                    if buffer.generation != generation {
                        buffer.generation = generation;
                        buffer.write_pos = 0;
                        buffer.ready = false;
                        buffer.completed_at = None;
                    }
                    let position = buffer.write_pos;
                    buffer.data[position] = sample;
                    buffer.write_pos = position + 1;
                    if buffer.write_pos >= FFT_SIZE {
                        buffer.write_pos = 0;
                        let SpectrumBuffer {
                            data, completed, ..
                        } = &mut *buffer;
                        std::mem::swap(data, completed);
                        buffer.ready = true;
                        buffer.completed_at = Some(std::time::Instant::now());
                    }
                }
            }
        }
        Some(sample)
    }
}
impl<S: Source<Item = f32>> Source for SpectrumSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}
