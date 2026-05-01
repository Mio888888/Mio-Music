use std::sync::Arc;

use parking_lot::Mutex;
use rodio::Source;
use rustfft::{FftPlanner, num_complex::Complex};

const FFT_SIZE: usize = 512;
const OUTPUT_BANDS: usize = 64;

pub struct SpectrumState {
    buffer: Vec<f32>,
    write_pos: usize,
    ready: bool,
    // 缓存规划器和 FFT 方案，避免每帧重建
    fft: Arc<dyn rustfft::Fft<f64>>,
    window: Vec<f64>,
}

impl SpectrumState {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);

        // 预计算 Hann 窗
        let window: Vec<f64> = (0..FFT_SIZE)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / FFT_SIZE as f64).cos()))
            .collect();

        Self {
            buffer: vec![0.0; FFT_SIZE],
            write_pos: 0,
            ready: false,
            fft,
            window,
        }
    }

    pub fn take_spectrum(&mut self) -> Option<Vec<f64>> {
        if !self.ready { return None }
        self.ready = false;

        let mut input: Vec<Complex<f64>> = self.buffer
            .iter()
            .zip(self.window.iter())
            .map(|(&s, &w)| Complex::new(s as f64 * w, 0.0))
            .collect();

        self.fft.process(&mut input);

        // 取前 OUTPUT_BANDS 个频率 bin 的幅度 (dB)
        // 使用分段映射：低频多 bin 合并，高频稀疏采样
        let magnitudes: Vec<f64> = (0..OUTPUT_BANDS)
            .map(|band| {
                // 对数映射：低频段窄，高频段宽
                let lo = self.band_start(band);
                let hi = self.band_start(band + 1);
                let mut sum = 0.0;
                let mut count = 0;
                for i in lo..hi {
                    if i >= input.len() { break }
                    let c = &input[i + 1]; // skip DC bin
                    let mag = (c.re * c.re + c.im * c.im).sqrt();
                    sum += mag;
                    count += 1;
                }
                let avg = if count > 0 { sum / count as f64 } else { 0.0 };
                20.0 * (avg / FFT_SIZE as f64 + 1e-10).log10()
            })
            .collect();

        Some(magnitudes)
    }

    /// 对数频率映射：模仿人耳对频率的感知
    /// 将 OUTPUT_BANDS 个柱子映射到 FFT bin 上
    fn band_start(&self, band: usize) -> usize {
        // 从 bin 1 开始（跳过 DC），到 bin OUTPUT_BANDS 结束
        // 使用幂函数分布让低频更密集
        let t = band as f64 / OUTPUT_BANDS as f64;
        (t.powf(1.6) * (OUTPUT_BANDS as f64)).round() as usize
    }
}

// ==================== Spectrum Source 包装器 ====================

pub struct SpectrumSource<S> {
    inner: S,
    state: Arc<Mutex<SpectrumState>>,
    sample_count: usize,
    channel_count: u16,
    // 每 N 个采样才写一次 buffer，降低锁竞争
    stride: usize,
    stride_counter: usize,
}

impl<S: Source<Item = f32>> SpectrumSource<S> {
    pub fn new(inner: S, state: Arc<Mutex<SpectrumState>>) -> Self {
        let channels = inner.channels();
        let sr = inner.sample_rate();
        // 目标：FFT buffer 每秒填充约 30 次
        // FFT_SIZE 个 mono 样本 / 30 ≈ 每次写多少
        // 44100Hz / 30 ≈ 1470 mono 样本/帧
        // stride = 跳过多少 mono 样本来匹配
        let target_fills_per_sec = 30;
        let mono_per_fill = sr as usize / target_fills_per_sec;
        let stride = if mono_per_fill > 512 { mono_per_fill / 512 } else { 1 };
        Self { inner, state, sample_count: 0, channel_count: channels, stride, stride_counter: 0 }
    }
}

impl<S> Iterator for SpectrumSource<S>
where S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        self.sample_count += 1;

        // 只取第一个声道
        if (self.sample_count - 1) % self.channel_count as usize == 0 {
            self.stride_counter += 1;
            if self.stride_counter >= self.stride {
                self.stride_counter = 0;
                let mut st = self.state.lock();
                let pos = st.write_pos;
                st.buffer[pos] = sample;
                st.write_pos = pos + 1;
                if st.write_pos >= FFT_SIZE {
                    st.write_pos = 0;
                    st.ready = true;
                }
            }
        }

        Some(sample)
    }
}

impl<S> Source for SpectrumSource<S>
where S: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> { self.inner.current_frame_len() }
    fn channels(&self) -> u16 { self.inner.channels() }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<std::time::Duration> { self.inner.total_duration() }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> { self.inner.try_seek(pos) }
}

// ==================== Position Source ====================

pub struct PositionState {
    pub samples_played: u64,
    pub seek_offset_secs: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
}

impl PositionState {
    pub fn new() -> Self {
        Self { samples_played: 0, seek_offset_secs: 0.0, sample_rate: 44100, channels: 2, duration_secs: 0.0 }
    }

    pub fn position_secs(&self) -> f64 {
        let total_samples = self.samples_played as f64;
        let base = total_samples / (self.sample_rate as f64 * self.channels as f64);
        base + self.seek_offset_secs
    }
}

pub struct PositionSource<S> {
    inner: S,
    state: Arc<Mutex<PositionState>>,
}

impl<S> PositionSource<S> {
    pub fn new(inner: S, state: Arc<Mutex<PositionState>>) -> Self {
        Self { inner, state }
    }
}

impl<S> Iterator for PositionSource<S>
where S: Source<Item = f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let mut st = self.state.lock();
        st.samples_played += 1;
        st.sample_rate = self.inner.sample_rate();
        st.channels = self.inner.channels();
        Some(sample)
    }
}

impl<S> Source for PositionSource<S>
where S: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> { self.inner.current_frame_len() }
    fn channels(&self) -> u16 { self.inner.channels() }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<std::time::Duration> { self.inner.total_duration() }
    fn try_seek(&mut self, pos: std::time::Duration) -> Result<(), rodio::source::SeekError> {
        let result = self.inner.try_seek(pos);
        if result.is_ok() {
            let mut st = self.state.lock();
            st.samples_played = 0;
            st.seek_offset_secs = pos.as_secs_f64();
        }
        result
    }
}
