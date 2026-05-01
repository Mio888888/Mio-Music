use std::sync::Arc;

use parking_lot::Mutex;
use rodio::Source;

// ==================== 双二阶滤波器 ====================

#[derive(Clone)]
pub struct BiquadFilter {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl BiquadFilter {
    pub fn new() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    /// Peaking EQ 滤波器系数
    pub fn peaking_eq(sample_rate: f64, freq: f64, gain_db: f64, q: f64) -> Self {
        let a = 10.0_f64.powf(gain_db / 40.0);
        let omega = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let cos_w = omega.cos();
        let sin_w = omega.sin();
        let alpha = sin_w / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w;
        let a2 = 1.0 - alpha / a;

        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    /// Low Shelf 滤波器系数（用于低频增强）
    pub fn low_shelf(sample_rate: f64, freq: f64, gain_db: f64) -> Self {
        let a = 10.0_f64.powf(gain_db / 40.0);
        let omega = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let cos_w = omega.cos();
        let sin_w = omega.sin();
        let beta = (a * a + 1.0).sqrt() / 0.707; // Q 归一化

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w + beta * sin_w);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w - beta * sin_w);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w + beta * sin_w;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w - beta * sin_w;

        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    pub fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

// ==================== EQ 共享状态 ====================

pub const NUM_EQ_BANDS: usize = 10;
pub const EQ_FREQS: [f64; NUM_EQ_BANDS] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
const EQ_Q: f64 = 1.4;

pub struct EqState {
    pub gains: [f64; NUM_EQ_BANDS],
    // [band][channel] filters
    pub filters: Vec<Vec<BiquadFilter>>,
    pub channels: u16,
    pub sample_rate: u32,
}

impl EqState {
    pub fn new(channels: u16, sample_rate: u32) -> Self {
        let filters = (0..NUM_EQ_BANDS)
            .map(|_| (0..channels as usize).map(|_| BiquadFilter::new()).collect())
            .collect();
        Self { gains: [0.0; NUM_EQ_BANDS], filters, channels, sample_rate }
    }

    pub fn set_band(&mut self, band: usize, gain_db: f64) {
        if band >= NUM_EQ_BANDS { return }
        self.gains[band] = gain_db;
        for ch in 0..self.channels as usize {
            self.filters[band][ch] = BiquadFilter::peaking_eq(
                self.sample_rate as f64, EQ_FREQS[band], gain_db, EQ_Q,
            );
        }
    }
}

// ==================== EQ Source 包装器 ====================

pub struct EqSource<S> {
    inner: S,
    state: Arc<Mutex<EqState>>,
    ch: usize,
}

impl<S> EqSource<S> {
    pub fn new(inner: S, state: Arc<Mutex<EqState>>) -> Self {
        Self { inner, state, ch: 0 }
    }
}

impl<S> Iterator for EqSource<S>
where S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let mut st = self.state.lock();
        let ch = self.ch % st.channels as usize;
        self.ch += 1;

        let mut val = sample as f64;
        for band in 0..NUM_EQ_BANDS {
            val = st.filters[band][ch].run(val);
        }
        Some(val as f32)
    }
}

impl<S> Source for EqSource<S>
where S: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> { self.inner.current_frame_len() }
    fn channels(&self) -> u16 { self.inner.channels() }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<std::time::Duration> { self.inner.total_duration() }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> { self.inner.try_seek(pos) }
}

// ==================== Balance Source ====================

pub struct BalanceSource<S> {
    inner: S,
    balance: Arc<Mutex<f64>>, // -1.0 ~ 1.0
    ch: usize,
}

impl<S> BalanceSource<S> {
    pub fn new(inner: S, balance: Arc<Mutex<f64>>) -> Self {
        Self { inner, balance, ch: 0 }
    }
}

impl<S> Iterator for BalanceSource<S>
where S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let balance = *self.balance.lock();
        let ch = self.ch % 2; // stereo
        self.ch += 1;

        let val = if balance >= 0.0 {
            if ch == 0 { sample } else { (sample as f64 * (1.0 - balance)) as f32 }
        } else {
            if ch == 0 { (sample as f64 * (1.0 + balance)) as f32 } else { sample }
        };
        Some(val)
    }
}

impl<S> Source for BalanceSource<S>
where S: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> { self.inner.current_frame_len() }
    fn channels(&self) -> u16 { self.inner.channels() }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<std::time::Duration> { self.inner.total_duration() }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> { self.inner.try_seek(pos) }
}

// ==================== Bass Boost Source ====================

pub struct BassBoostSource<S> {
    inner: S,
    gain: Arc<Mutex<f64>>,
    filters: Vec<BiquadFilter>, // per channel
    ch: usize,
}

impl<S> BassBoostSource<S> {
    pub fn new(inner: S, gain: Arc<Mutex<f64>>, channels: u16, _sample_rate: u32) -> Self {
        let filters = (0..channels as usize)
            .map(|_| BiquadFilter::new())
            .collect();
        Self { inner, gain, filters, ch: 0 }
    }
}

impl<S> Iterator for BassBoostSource<S>
where S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let gain = *self.gain.lock();
        let ch = self.ch % self.filters.len();
        self.ch += 1;

        // 当 gain 变化时重建滤波器
        let val = if gain > 0.01 {
            let sr = self.inner.sample_rate() as f64;
            self.filters[ch] = BiquadFilter::low_shelf(sr, 120.0, gain * 12.0);
            self.filters[ch].run(sample as f64) as f32
        } else {
            sample
        };
        Some(val)
    }
}

impl<S> Source for BassBoostSource<S>
where S: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> { self.inner.current_frame_len() }
    fn channels(&self) -> u16 { self.inner.channels() }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<std::time::Duration> { self.inner.total_duration() }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> { self.inner.try_seek(pos) }
}
