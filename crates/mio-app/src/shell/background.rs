//! macOS GPU port of the installed AMLL Pixi background behavior.
//! One in-flight command, no CPU readback and no reuse of a displayed surface.
use super::*;
use crate::background_config::BackgroundConfig;
use core_foundation::{
    base::TCFType, boolean::CFBoolean, dictionary::CFDictionary, string::CFString,
};
use core_video::{
    metal_texture::CVMetalTexture,
    metal_texture_cache::CVMetalTextureCache,
    pixel_buffer::{
        CVPixelBuffer, kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey,
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    },
};
use metal::*;

pub(super) struct Background {
    pixels: Option<Arc<image::RgbaImage>>,
    revision: u64,
    gpu: Option<Gpu>,
    failed: bool,
    last_tick: Instant,
    last_submit: Instant,
    frame: Option<CVPixelBuffer>,
    frame_size: (u32, u32),
    reduced: bool,
    dirty: bool,
    awaiting_cover: bool,
    config: BackgroundConfig,
    settled: bool,
    monitor: PerformanceMonitor,
}
impl Default for Background {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            pixels: None,
            revision: 0,
            gpu: None,
            failed: false,
            last_tick: now,
            last_submit: now - Duration::from_secs(1),
            frame: None,
            frame_size: (0, 0),
            reduced: false,
            dirty: true,
            awaiting_cover: false,
            config: BackgroundConfig::default(),
            settled: false,
            monitor: PerformanceMonitor::default(),
        }
    }
}
impl Background {
    pub(super) fn configure(&mut self, config: BackgroundConfig) {
        if self.config == config {
            return;
        }
        let enabled_changed = self.config.enabled != config.enabled;
        self.config = config;
        self.dirty = true;
        self.settled = false;
        self.monitor = PerformanceMonitor::default();
        self.last_tick = Instant::now();
        if enabled_changed {
            self.gpu = None;
            self.frame = None;
            self.failed = false;
        }
    }
    pub(super) fn set_cover(&mut self, pixels: Arc<image::RgbaImage>) {
        if self
            .pixels
            .as_ref()
            .is_some_and(|old| old.as_ref() == pixels.as_ref())
        {
            self.awaiting_cover = false;
            if let Some(gpu) = &mut self.gpu
                && gpu
                    .uploaded_pixels
                    .as_ref()
                    .is_some_and(|uploaded| Arc::ptr_eq(uploaded, self.pixels.as_ref().unwrap()))
            {
                // CPU equality alone cannot prove a queued cover has reached the GPU.
                gpu.revision = self.revision;
            }
            return;
        }
        self.awaiting_cover = false;
        self.dirty = true;
        self.settled = false;
        self.pixels = Some(pixels);
        self.revision += 1;
    }
    pub(super) fn invalidate_track(&mut self) {
        self.revision += 1;
        self.awaiting_cover = true;
        self.settled = false;
        self.frame = None;
        self.dirty = true;
        self.last_tick = Instant::now();
        // Keep the outgoing texture only for a transition once the new cover arrives.
        // No old background is displayed or submitted while awaiting this song's cover.
    }
    pub(super) fn frame(&self) -> Option<CVPixelBuffer> {
        self.frame.clone()
    }
    fn tick(
        &mut self,
        dims: (u32, u32),
        visible: bool,
        active: bool,
        reduced: bool,
    ) -> Result<(), String> {
        let now = Instant::now();
        let delta = now.duration_since(self.last_tick).as_secs_f32().min(0.1);
        self.last_tick = now;
        if self.reduced != reduced {
            self.reduced = reduced;
            self.dirty = true;
        }
        if !self.config.enabled || self.failed || self.pixels.is_none() || self.awaiting_cover {
            return Ok(());
        }
        if self.gpu.is_none() {
            if !visible {
                return Ok(());
            }
            self.gpu = Some(Gpu::new()?);
        }
        let gpu = self.gpu.as_mut().unwrap();
        // The clock advances independently of GPU completion, without queuing more work.
        if visible && active && !reduced {
            if !self.config.static_mode {
                gpu.time += delta as f64 * 60. * self.config.flow_speed as f64;
            }
            gpu.alpha = (gpu.alpha + delta).min(1.);
            gpu.old_alpha = (gpu.old_alpha - delta).max(0.);
        } else if reduced || !active {
            gpu.alpha = 1.;
            gpu.old_alpha = 0.;
        }
        if let Some(pending) = &gpu.pending {
            match pending.command.status() {
                MTLCommandBufferStatus::Completed => {
                    let pending = gpu.pending.take().unwrap();
                    gpu.cache.flush(0);
                    if pending.revision == self.revision && pending.size == dims {
                        self.frame = Some(pending.buffer);
                        self.frame_size = dims;
                    }
                }
                MTLCommandBufferStatus::Error => return Err("background GPU command failed".into()),
                _ => return Ok(()),
            }
        }
        if !visible {
            return Ok(());
        }
        if gpu.revision != self.revision {
            gpu.set_cover(self.pixels.as_ref().unwrap().clone(), self.revision);
            if !active {
                gpu.alpha = 1.;
                gpu.old_alpha = 0.;
            }
        }
        if reduced {
            gpu.alpha = 1.;
            gpu.old_alpha = 0.;
        }
        if gpu.old_alpha == 0. {
            gpu.old = None;
        }
        let changed = self.dirty
            || self.frame.is_none()
            || self.frame_size != dims
            || gpu.submitted_revision != self.revision
            || !self.settled;
        if !changed && (!active || reduced || self.config.static_mode) {
            return Ok(());
        }
        if now.duration_since(self.last_submit)
            < Duration::from_secs_f64(1. / self.config.fps as f64)
        {
            return Ok(());
        }
        self.last_submit = now;
        gpu.submit(dims)?;
        self.settled = gpu.alpha == 1. && gpu.old_alpha == 0.;
        self.dirty = false;
        Ok(())
    }
}
impl Mio {
    pub(super) fn update_background(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let viewport = window.viewport_size();
        let width = f32::from(viewport.width);
        let height = f32::from(viewport.height);
        self.background.configure(self.background_config);
        // Bound exceptional display sizes to 2048 per axis.
        let scale = (window.scale_factor() * self.background_config.render_scale)
            .min(2048. / width.max(height).max(1.));
        let dims = (
            ((width * scale) as u32 / 2 * 2).max(2),
            ((height * scale) as u32 / 2 * 2).max(2),
        );
        let visible = self.player_motion.visibility > 0.;
        let active = window.is_window_active();
        let reduced = cx.reduce_motion();
        let result =
            objc::rc::autoreleasepool(|| self.background.tick(dims, visible, active, reduced));
        if let Err(error) = result {
            // Do not repeatedly retry/allocate on an unsupported device. Existing blur remains available.
            eprintln!("Mio background GPU unavailable; using static cover background: {error}");
            self.background.failed = true;
            self.background.frame = None;
            self.background.gpu = None;
        }
        let monitoring = visible
            && active
            && !reduced
            && self.background_config.enabled
            && !self.background_config.static_mode
            && !self.background.failed
            && self.background.frame.is_some()
            && !self.background.awaiting_cover;
        if self.background.monitor.tick(monitoring) {
            self.background_config = self.background_config.degraded();
            self.background.configure(self.background_config);
            self.background.monitor.degraded = true;
            self.background_warning = Some(Instant::now());
            self.sync_background_controls(window, cx);
            self.save_preferences();
        }
        if self
            .background_warning
            .is_some_and(|started| started.elapsed() >= Duration::from_secs(5))
        {
            self.background_warning = None;
        }
        if visible
            && self.background_config.enabled
            && !self.background.failed
            && self.background.pixels.is_some()
            && !self.background.awaiting_cover
            && (self.background.dirty
                || self.background.frame.is_none()
                || self.background.frame_size != dims
                || !self.background.settled
                || (active && !reduced && !self.background_config.static_mode)
                || self
                    .background
                    .gpu
                    .as_ref()
                    .is_some_and(|gpu| gpu.pending.is_some()))
        {
            window.request_animation_frame();
        }
    }
}
// Monitor UI render cadence, independently of the GPU submission FPS cap.
// Stop sampling hidden/inactive/static views instead of counting intentional idle time.
struct PerformanceMonitor {
    started: Instant,
    frames: u32,
    fps: f32,
    low_frames: u32,
    sampling: bool,
    degraded: bool,
}
impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            frames: 0,
            fps: 60.,
            low_frames: 0,
            sampling: false,
            degraded: false,
        }
    }
}
impl PerformanceMonitor {
    fn tick(&mut self, sampling: bool) -> bool {
        if self.degraded {
            return false;
        }
        if !sampling || !self.sampling {
            self.started = Instant::now();
            self.frames = 0;
            self.fps = 60.;
            self.low_frames = 0;
            self.sampling = sampling;
            return false;
        }
        self.frames += 1;
        let elapsed = self.started.elapsed().as_secs_f32();
        if elapsed >= 0.5 {
            self.fps = self.frames as f32 / elapsed;
            self.frames = 0;
            self.started = Instant::now();
        }
        if self.fps < 20. {
            self.low_frames += 1;
        } else {
            self.low_frames = 0;
        }
        if self.low_frames >= 300 {
            self.degraded = true;
            return true;
        }
        false
    }
}
struct Pending {
    command: CommandBuffer,
    buffer: CVPixelBuffer,
    // Keep the CoreVideo texture wrappers alive until our GPU has finished writing.
    _planes: [CVMetalTexture; 2],
    revision: u64,
    size: (u32, u32),
}
struct Gpu {
    device: Device,
    queue: CommandQueue,
    cache: CVMetalTextureCache,
    pipelines: [ComputePipelineState; 5],
    textures: Option<((u32, u32), [Texture; 2])>,
    mirrored: bool,
    cover: Option<Texture>,
    uploaded_pixels: Option<Arc<image::RgbaImage>>,
    old: Option<Texture>,
    angles: [f32; 4],
    old_angles: [f32; 4],
    time: f64,
    old_time: f32,
    alpha: f32,
    old_alpha: f32,
    revision: u64,
    submitted_revision: u64,
    pending: Option<Pending>,
}
impl Gpu {
    fn new() -> Result<Self, String> {
        let device = Device::system_default().ok_or("no Metal device")?;
        let queue = device.new_command_queue();
        let library = device
            .new_library_with_source(include_str!("background.metal"), &CompileOptions::new())?;
        let mut pipelines = Vec::new();
        for name in ["compose", "blur", "tint", "bulge", "surface_output"] {
            let function = library.get_function(name, None)?;
            pipelines.push(device.new_compute_pipeline_state_with_function(&function)?);
        }
        let cache = CVMetalTextureCache::new(None, device.clone(), None)
            .map_err(|_| "texture cache unavailable")?;
        Ok(Self {
            device,
            queue,
            cache,
            pipelines: pipelines.try_into().ok().unwrap(),
            textures: None,
            mirrored: false,
            cover: None,
            uploaded_pixels: None,
            old: None,
            angles: [0.; 4],
            old_angles: [0.; 4],
            time: 0.,
            old_time: 0.,
            alpha: 0.,
            old_alpha: 0.,
            revision: 0,
            submitted_revision: 0,
            pending: None,
        })
    }
    fn texture(
        &self,
        size: (u32, u32),
        usage: MTLTextureUsage,
        storage: MTLStorageMode,
    ) -> Texture {
        let descriptor = TextureDescriptor::new();
        descriptor.set_texture_type(MTLTextureType::D2);
        descriptor.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
        descriptor.set_width(size.0 as u64);
        descriptor.set_height(size.1 as u64);
        descriptor.set_usage(usage);
        descriptor.set_storage_mode(storage);
        self.device.new_texture(&descriptor)
    }
    fn set_cover(&mut self, pixels: Arc<image::RgbaImage>, revision: u64) {
        // Retain only one outgoing cover. Rapid changes never accumulate textures.
        self.old = self.cover.take();
        self.old_time = self.time as f32;
        self.old_angles = self.angles;
        self.old_alpha = self.alpha;
        let texture = self.texture(
            pixels.dimensions(),
            MTLTextureUsage::ShaderRead,
            MTLStorageMode::Shared,
        );
        texture.replace_region(
            MTLRegion::new_2d(0, 0, pixels.width() as u64, pixels.height() as u64),
            0,
            pixels.as_raw().as_ptr().cast(),
            pixels.width() as u64 * 4,
        );
        self.cover = Some(texture);
        self.uploaded_pixels = Some(pixels);
        self.time = 0.;
        self.alpha = 0.;
        self.revision = revision;
        let seed = uuid::Uuid::new_v4().as_u128().to_le_bytes();
        self.angles = std::array::from_fn(|i| {
            u32::from_le_bytes(seed[i * 4..i * 4 + 4].try_into().unwrap()) as f32 / u32::MAX as f32
                * std::f32::consts::TAU
        });
    }
    fn dispatch(
        &self,
        command: &CommandBufferRef,
        index: usize,
        textures: &[&TextureRef],
        params: Option<&[f32]>,
        dims: (u32, u32),
    ) {
        let encoder = command.new_compute_command_encoder();
        encoder.set_compute_pipeline_state(&self.pipelines[index]);
        for (i, texture) in textures.iter().enumerate() {
            encoder.set_texture(i as u64, Some(texture));
        }
        if let Some(params) = params {
            encoder.set_bytes(
                0,
                std::mem::size_of_val(params) as u64,
                params.as_ptr().cast(),
            );
        }
        encoder.dispatch_threads(
            MTLSize::new(dims.0 as u64, dims.1 as u64, 1),
            MTLSize::new(8, 8, 1),
        );
        encoder.end_encoding();
    }
    fn submit(&mut self, dims: (u32, u32)) -> Result<(), String> {
        if self.textures.as_ref().is_none_or(|(size, _)| *size != dims) {
            self.mirrored = uuid::Uuid::new_v4().as_u128() & 1 != 0;
            self.textures = Some((
                dims,
                std::array::from_fn(|_| {
                    self.texture(
                        dims,
                        MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite,
                        MTLStorageMode::Private,
                    )
                }),
            ));
        }
        let empty = CFDictionary::<CFString, core_foundation::base::CFType>::from_CFType_pairs(&[]);
        let options = CFDictionary::from_CFType_pairs(&[
            (
                unsafe { CFString::wrap_under_get_rule(kCVPixelBufferIOSurfacePropertiesKey) },
                empty.as_CFType(),
            ),
            (
                unsafe { CFString::wrap_under_get_rule(kCVPixelBufferMetalCompatibilityKey) },
                CFBoolean::true_value().as_CFType(),
            ),
        ]);
        // A fresh output prevents the producer from overwriting a surface still being sampled by GPUI.
        let buffer = CVPixelBuffer::new(
            kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            dims.0 as usize,
            dims.1 as usize,
            Some(&options),
        )
        .map_err(|_| "pixel buffer unavailable")?;
        let y = self
            .cache
            .create_texture_from_image(
                buffer.as_concrete_TypeRef(),
                None,
                MTLPixelFormat::R8Unorm,
                dims.0 as usize,
                dims.1 as usize,
                0,
            )
            .map_err(|_| "luma texture unavailable")?;
        let uv = self
            .cache
            .create_texture_from_image(
                buffer.as_concrete_TypeRef(),
                None,
                MTLPixelFormat::RG8Unorm,
                dims.0 as usize / 2,
                dims.1 as usize / 2,
                1,
            )
            .map_err(|_| "chroma texture unavailable")?;
        // CVMetalTextureGetTexture follows the Get rule. core-video 0.5.2 wraps
        // it as an owned metal::Texture without retaining; dropping that wrapper
        // over-releases the cache's texture. Borrow it for encoding, retaining the
        // CVMetalTexture wrappers in Pending until completion. No reference is leaked.
        let y_texture = std::mem::ManuallyDrop::new(y.get_texture().ok_or("missing luma texture")?);
        let uv_texture =
            std::mem::ManuallyDrop::new(uv.get_texture().ok_or("missing chroma texture")?);
        let textures = &self.textures.as_ref().unwrap().1;
        let command = self.queue.new_command_buffer();
        let cover = self.cover.as_ref().unwrap();
        let mut angles = [0.; 8];
        angles[..4].copy_from_slice(&self.angles);
        angles[4..].copy_from_slice(&self.old_angles);
        // Sixteen contiguous f32 values match Composition in background.metal.
        let params = [
            dims.0 as f32,
            dims.1 as f32,
            self.time as f32,
            self.old_time,
            self.alpha,
            self.old_alpha,
            if self.old.is_some() { 1. } else { 0. },
            0.,
            angles[0],
            angles[1],
            angles[2],
            angles[3],
            angles[4],
            angles[5],
            angles[6],
            angles[7],
        ];
        self.dispatch(
            command,
            0,
            &[cover, self.old.as_ref().unwrap_or(cover), &textures[0]],
            Some(&params),
            dims,
        );
        let mut current = 0;
        let mut blur_pass = |strength: f32, quality: u32| {
            for axis in 0..2 {
                for _ in 0..quality {
                    let mut step = [0.; 2];
                    step[axis] = strength
                        / quality as f32
                        / if axis == 0 {
                            dims.0 as f32
                        } else {
                            dims.1 as f32
                        };
                    self.dispatch(
                        command,
                        1,
                        &[&textures[current], &textures[1 - current]],
                        Some(&step),
                        dims,
                    );
                    current = 1 - current;
                }
            }
        };
        for (strength, quality) in [(5., 1), (10., 1), (20., 2), (40., 2), (80., 2)] {
            blur_pass(strength, quality);
        }
        if dims.0.min(dims.1) > 768 {
            blur_pass(160., 4);
        }
        if dims.0.min(dims.1) > 1536 {
            blur_pass(320., 4);
        }
        for mode in [0., 1., 2.] {
            self.dispatch(
                command,
                2,
                &[&textures[current], &textures[1 - current]],
                Some(&[mode]),
                dims,
            );
            current = 1 - current;
        }
        for step in [[5. / dims.0 as f32, 0.], [0., 5. / dims.1 as f32]] {
            self.dispatch(
                command,
                1,
                &[&textures[current], &textures[1 - current]],
                Some(&step),
                dims,
            );
            current = 1 - current;
        }
        for center in [
            [if self.mirrored { 0.75 } else { 0.25 }, 1.],
            [if self.mirrored { 0.25 } else { 0.75 }, 0.],
        ] {
            self.dispatch(
                command,
                3,
                &[&textures[current], &textures[1 - current]],
                Some(&center),
                dims,
            );
            current = 1 - current;
        }
        self.dispatch(
            command,
            4,
            &[&textures[current], &y_texture, &uv_texture],
            None,
            (dims.0 / 2, dims.1 / 2),
        );
        // CoreVideo requires its wrappers to outlive GPU execution, even if a
        // source change or window close drops Background/Pending in the meantime.
        // This completion callback owns independent references. It only releases
        // thread-safe CoreVideo objects; it never touches GPUI or app state.
        let retained = std::cell::RefCell::new(Some((buffer.clone(), y.clone(), uv.clone())));
        let completion = block::ConcreteBlock::new(move |_command: &CommandBufferRef| {
            drop(retained.borrow_mut().take());
        })
        .copy();
        command.add_completed_handler(&completion);
        command.commit();
        self.submitted_revision = self.revision;
        self.pending = Some(Pending {
            command: command.to_owned(),
            buffer,
            _planes: [y, uv],
            revision: self.revision,
            size: dims,
        });
        Ok(())
    }
}
