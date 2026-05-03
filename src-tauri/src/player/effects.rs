use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rodio::Source;

// ==================== Atomic f64 helper ====================

/// Lock-free f64 通过 AtomicU64 实现，用于音频热路径上的参数共享。
/// Relaxed ordering 足够：单值读写，无复杂同步协议。
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    pub fn new(v: f64) -> Self { Self(AtomicU64::new(v.to_bits())) }
    pub fn load(&self) -> f64 { f64::from_bits(self.0.load(Ordering::Relaxed)) }
    pub fn store(&self, v: f64) { self.0.store(v.to_bits(), Ordering::Relaxed) }
}

// ==================== 双二阶滤波器 ====================

#[derive(Clone)]
pub struct BiquadFilter {
    b0: f64, b1: f64, b2: f64, a1: f64, a2: f64,
    x1: f64, x2: f64, y1: f64, y2: f64,
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
        let beta = (a * a + 1.0).sqrt() / 0.707;

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w + beta * sin_w);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w - beta * sin_w);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w + beta * sin_w;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w - beta * sin_w;

        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    /// 从另一个滤波器更新系数，保留内部状态 (x1, x2, y1, y2)，
    /// 避免 gain 变化时的音频爆破噪声。
    fn update_coeffs_from(&mut self, other: &BiquadFilter) {
        self.b0 = other.b0;
        self.b1 = other.b1;
        self.b2 = other.b2;
        self.a1 = other.a1;
        self.a2 = other.a2;
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

/// EQ 状态通过原子操作共享 gain 值，音频线程在本地持有滤波器实例。
/// IPC 线程调用 set_band 写入 gain → 音频线程通过原子读取检测变化 → 本地重建系数。
pub struct EqState {
    gains: [AtomicU64; NUM_EQ_BANDS],
    pub channels: u16,
    pub sample_rate: u32,
}

impl EqState {
    pub fn new(channels: u16, sample_rate: u32) -> Self {
        Self {
            gains: std::array::from_fn(|_| AtomicU64::new(0.0f64.to_bits())),
            channels,
            sample_rate,
        }
    }

    pub fn set_band(&self, band: usize, gain_db: f64) {
        if band >= NUM_EQ_BANDS { return }
        self.gains[band].store(gain_db.to_bits(), Ordering::Relaxed);
    }

    pub fn get_gains(&self) -> [f64; NUM_EQ_BANDS] {
        std::array::from_fn(|i| f64::from_bits(self.gains[i].load(Ordering::Relaxed)))
    }
}

// ==================== EQ Source 包装器 ====================

/// EQ 滤波器 Source：本地持有滤波器状态，通过原子操作检测 gain 变化。
/// 零锁竞争：音频热路径完全不涉及 Mutex。
pub struct EqSource<S> {
    inner: S,
    eq_state: Arc<EqState>,
    filters: Vec<Vec<BiquadFilter>>,
    cached_gains: [f64; NUM_EQ_BANDS],
    ch: usize,
}

impl<S> EqSource<S> {
    pub fn new(inner: S, eq_state: Arc<EqState>) -> Self {
        let channels = eq_state.channels as usize;
        let filters = (0..NUM_EQ_BANDS)
            .map(|_| (0..channels).map(|_| BiquadFilter::new()).collect())
            .collect();
        Self { inner, eq_state, filters, cached_gains: [0.0; NUM_EQ_BANDS], ch: 0 }
    }
}

impl<S> Iterator for EqSource<S>
where S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let ch = self.ch % self.eq_state.channels as usize;
        self.ch += 1;

        // 通过原子读取检测 gain 变化（每采样 10 次原子 load，开销 < 0.1% CPU）
        for band in 0..NUM_EQ_BANDS {
            let gain = f64::from_bits(self.eq_state.gains[band].load(Ordering::Relaxed));
            if gain != self.cached_gains[band] {
                self.cached_gains[band] = gain;
                let new_filter = BiquadFilter::peaking_eq(
                    self.eq_state.sample_rate as f64, EQ_FREQS[band], gain, EQ_Q,
                );
                for c in 0..self.eq_state.channels as usize {
                    self.filters[band][c].update_coeffs_from(&new_filter);
                }
            }
        }

        let mut val = sample as f64;
        for band in 0..NUM_EQ_BANDS {
            val = self.filters[band][ch].run(val);
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
    balance: Arc<AtomicF64>,
    ch: usize,
}

impl<S> BalanceSource<S> {
    pub fn new(inner: S, balance: Arc<AtomicF64>) -> Self {
        Self { inner, balance, ch: 0 }
    }
}

impl<S> Iterator for BalanceSource<S>
where S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let balance = self.balance.load();
        let ch = self.ch % 2;
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

/// 低频增强：仅在 gain 实际变化时重建滤波器系数，
/// 避免每采样调用 cos/sin/sqrt/powf。
pub struct BassBoostSource<S> {
    inner: S,
    gain: Arc<AtomicF64>,
    filters: Vec<BiquadFilter>,
    last_gain: f64,
    sample_rate: u32,
    ch: usize,
}

impl<S> BassBoostSource<S> {
    pub fn new(inner: S, gain: Arc<AtomicF64>, channels: u16, sample_rate: u32) -> Self {
        let filters = (0..channels as usize)
            .map(|_| BiquadFilter::new())
            .collect();
        Self { inner, gain, filters, last_gain: 0.0, sample_rate, ch: 0 }
    }
}

impl<S> Iterator for BassBoostSource<S>
where S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let gain = self.gain.load();
        let ch = self.ch % self.filters.len();
        self.ch += 1;

        let val = if gain > 0.01 {
            if (gain - self.last_gain).abs() > 0.001 {
                self.last_gain = gain;
                let new_filter = BiquadFilter::low_shelf(
                    self.sample_rate as f64, 120.0, gain * 12.0,
                );
                for f in &mut self.filters {
                    f.update_coeffs_from(&new_filter);
                }
            }
            self.filters[ch].run(sample as f64) as f32
        } else {
            self.last_gain = gain;
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
