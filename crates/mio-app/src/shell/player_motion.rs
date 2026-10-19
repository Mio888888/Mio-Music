use super::*;
use std::sync::OnceLock;

pub(super) struct Tween {
    from: f32,
    to: f32,
    since: Instant,
}
impl Tween {
    pub(super) fn new(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            since: Instant::now(),
        }
    }
    pub(super) fn value(&self, now: Instant, duration: f32, curve: [f32; 4]) -> f32 {
        let time = (now.duration_since(self.since).as_secs_f32() / duration).min(1.);
        let progress = easing(time, curve);
        self.from + (self.to - self.from) * progress
    }
    pub(super) fn target(
        &mut self,
        target: f32,
        now: Instant,
        duration: f32,
        curve: [f32; 4],
        immediate: bool,
    ) {
        if immediate {
            self.from = target;
            self.to = target;
            self.since = now - Duration::from_secs(1);
        } else if self.to != target {
            self.from = self.value(now, duration, curve);
            self.to = target;
            self.since = now;
        }
    }
    pub(super) fn active(&self, now: Instant, duration: f32) -> bool {
        self.from != self.to && now.duration_since(self.since).as_secs_f32() < duration
    }
}

pub(super) struct RotationFrame {
    pub cover: Option<Arc<RenderImage>>,
    pub pointer: Arc<RenderImage>,
    pub pointer_offset: (f32, f32),
    pub pointer_size: (f32, f32),
    pub pointer_width: f32,
    pub generation: u64,
    pub(super) angle: f32,
    pointer_angle: f32,
}
pub(super) struct PlayerMotion {
    open: Tween,
    playing: Tween,
    pointer: Tween,
    idle: Tween,
    settings: Tween,
    volume: Tween,
    queue: Tween,
    last_tick: Instant,
    was_playing: bool,
    angle: f32,
    pub visibility: f32,
    pub idle_visibility: f32,
    pub settings_visibility: f32,
    pub volume_visibility: f32,
    pub queue_visibility: f32,
    pub cover_scale: f32,
    pub texture_time: f32,
    pub title_clock: Option<(String, bool, u32, Instant)>,
    pub frame: Option<RotationFrame>,
    retired: Vec<Arc<RenderImage>>,
    busy: bool,
    last_frame: Instant,
    requested: Option<(u64, f32, f32)>,
}
impl Default for PlayerMotion {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            open: Tween::new(0.),
            playing: Tween::new(0.),
            pointer: Tween::new(0.),
            idle: Tween::new(0.),
            settings: Tween::new(0.),
            volume: Tween::new(0.),
            queue: Tween::new(0.),
            last_tick: now,
            was_playing: false,
            angle: 0.,
            visibility: 0.,
            idle_visibility: 0.,
            settings_visibility: 0.,
            volume_visibility: 0.,
            queue_visibility: 0.,
            cover_scale: 0.8,
            texture_time: 0.,
            title_clock: None,
            frame: None,
            retired: Vec::new(),
            busy: false,
            last_frame: now - Duration::from_secs(1),
            requested: None,
        }
    }
}
impl PlayerMotion {
    pub(super) fn invalidate_cover(&mut self) {
        self.requested = None;
    }
    fn retire(&mut self) {
        if let Some(frame) = self.frame.take() {
            self.retired.extend(frame.cover);
            self.retired.push(frame.pointer);
        }
    }
    /// Shows `frame`, keeping a tonearm image it reuses from the frame it replaces.
    fn replace(&mut self, frame: RotationFrame) {
        if let Some(old) = self.frame.take() {
            self.retired.extend(old.cover);
            if !Arc::ptr_eq(&old.pointer, &frame.pointer) {
                self.retired.push(old.pointer);
            }
        }
        self.frame = Some(frame);
    }
}

impl Mio {
    pub(super) fn update_player_motion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        let playing = self.player.status == Status::Playing;
        let motion = &mut self.player_motion;
        for image in motion.retired.drain(..) {
            let _ = window.drop_image(image);
        }
        if motion
            .frame
            .as_ref()
            .is_some_and(|frame| frame.generation != self.player.generation)
        {
            motion.retire();
            for image in motion.retired.drain(..) {
                let _ = window.drop_image(image);
            }
        }
        if motion.was_playing && !cx.reduce_motion() {
            if window.is_window_active() {
                motion.texture_time = (motion.texture_time
                    + now.duration_since(motion.last_tick).as_secs_f32())
                    % 264.;
            }
            motion.angle = (motion.angle
                + now.duration_since(motion.last_tick).as_secs_f32() * std::f32::consts::TAU / 33.)
                % std::f32::consts::TAU;
        }
        if motion.was_playing && !playing {
            // Pause at the displayed phase, including when the window is inactive.
            // Activating the window or seeking must not reveal a hidden clock jump.
            if let Some(frame) = motion
                .frame
                .as_ref()
                .filter(|frame| frame.generation == self.player.generation)
            {
                motion.angle = frame.angle;
            }
        }
        motion.last_tick = now;
        motion.was_playing = playing;
        let immediate = cx.reduce_motion() || self.player.song().is_none();
        motion.idle.target(
            if self.full_player_open && self.player_idle.hidden {
                1.
            } else {
                0.
            },
            now,
            0.44,
            [0.34, 1.35, 0.64, 1.],
            immediate,
        );
        motion.idle_visibility = motion.idle.value(now, 0.44, [0.34, 1.35, 0.64, 1.]);
        motion.settings.target(
            if self.full_player_open && self.player_settings_open {
                1.
            } else {
                0.
            },
            now,
            0.28,
            [0.34, 1.35, 0.64, 1.],
            immediate,
        );
        motion.settings_visibility = motion.settings.value(now, 0.28, [0.34, 1.35, 0.64, 1.]);
        motion.queue.target(
            if self.queue_open { 1. } else { 0. },
            now,
            0.25,
            [0.4, 0., 0.2, 1.],
            immediate,
        );
        motion.queue_visibility = motion.queue.value(now, 0.25, [0.4, 0., 0.2, 1.]);
        motion.volume.target(
            if self.volume_open { 1. } else { 0. },
            now,
            0.2,
            [0.8, 0., 0.8, 0.43],
            immediate,
        );
        motion.volume_visibility = motion.volume.value(now, 0.2, [0.8, 0., 0.8, 0.43]);
        motion.open.target(
            if self.full_player_open { 1. } else { 0. },
            now,
            0.28,
            [0.22, 1., 0.36, 1.],
            immediate,
        );
        motion.playing.target(
            if playing { 1. } else { 0. },
            now,
            0.44,
            [0.34, 1.35, 0.64, 1.],
            immediate,
        );
        motion.pointer.target(
            if playing { 1. } else { 0. },
            now,
            0.28,
            [0.2, 0., 0., 1.],
            immediate,
        );
        motion.visibility = motion.open.value(now, 0.28, [0.22, 1., 0.36, 1.]);
        motion.cover_scale = 0.8 + 0.2 * motion.playing.value(now, 0.44, [0.34, 1.35, 0.64, 1.]);
        let pointer =
            -20_f32.to_radians() * (1. - motion.pointer.value(now, 0.28, [0.2, 0., 0., 1.]));
        let visible = motion.visibility > 0.;
        if !cx.reduce_motion()
            && (motion.open.active(now, 0.28)
                || motion.idle.active(now, 0.44)
                || motion.settings.active(now, 0.28)
                || motion.volume.active(now, 0.2)
                || motion.queue.active(now, 0.25)
                || (visible
                    && (motion.playing.active(now, 0.44)
                        || (playing && !self.cover_layout && window.is_window_active()))))
        {
            window.request_animation_frame();
        }
        let angle = if window.is_window_active() && !cx.reduce_motion() {
            motion.angle
        } else {
            motion
                .frame
                .as_ref()
                .filter(|frame| frame.generation == self.player.generation)
                .map(|frame| frame.angle)
                .unwrap_or(motion.angle)
        };
        let key = (self.player.generation, angle, pointer);
        if !visible
            || self.cover_layout
            || motion.busy
            || motion.requested == Some(key)
            || now.duration_since(motion.last_frame) < Duration::from_millis(33)
        {
            return;
        }
        motion.busy = true;
        motion.last_frame = now;
        motion.requested = Some(key);
        let pixels = self.cover_pixels.clone();
        // The tonearm only moves around play/pause; while it rests, its image is reused.
        let resting = motion
            .frame
            .as_ref()
            .filter(|frame| frame.pointer_angle == key.2)
            .map(|frame| (frame.pointer.clone(), frame.pointer_offset));
        // One bounded job at a time; no frame archive, timer loop or audio thread work.
        let task = cx.background_executor().spawn(async move {
            static POINTER: OnceLock<image::RgbaImage> = OnceLock::new();
            let pointer_pixels = POINTER.get_or_init(|| {
                image::load_from_memory(include_bytes!("../../assets/images/pointer.png"))
                    .expect("embedded pointer PNG")
                    .to_rgba8()
            });
            let cover = pixels.map(|pixels| rotate_pixels(&pixels, key.1, (0.5, 0.5), false).0);
            let (pointer, offset) =
                resting.unwrap_or_else(|| rotate_pixels(pointer_pixels, key.2, (0.16, 0.16), true));
            let size = pointer.size(0);
            RotationFrame {
                cover,
                pointer,
                pointer_offset: offset,
                pointer_size: (size.width.0 as f32, size.height.0 as f32),
                pointer_width: pointer_pixels.width() as f32,
                generation: key.0,
                angle: key.1,
                pointer_angle: key.2,
            }
        });
        cx.spawn(async move |this, cx| {
            let frame = task.await;
            let _ = this.update(cx, |this, cx| {
                this.player_motion.busy = false;
                if this.player.generation == frame.generation
                    && this.full_player_open
                    && !this.cover_layout
                    && (this.player.status == Status::Playing
                        || frame.angle == this.player_motion.angle)
                {
                    this.player_motion.replace(frame);
                } else if this.player_motion.requested == Some(key) {
                    this.player_motion.requested = None;
                }
                cx.notify();
            });
        })
        .detach();
    }
}

// Inverse bilinear sampling directly into GPUI's BGRA frame. The original
// PNG and album pixels stay immutable; no encoding or disk I/O per frame.
fn rotate_pixels(
    source: &image::RgbaImage,
    angle: f32,
    pivot: (f32, f32),
    expand: bool,
) -> (Arc<RenderImage>, (f32, f32)) {
    let (width, height) = (source.width() as f32, source.height() as f32);
    let (pivot_x, pivot_y) = (width * pivot.0, height * pivot.1);
    let (sin, cos) = angle.sin_cos();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (0., 0., width, height);
    if expand {
        min_x = f32::INFINITY;
        min_y = f32::INFINITY;
        max_x = f32::NEG_INFINITY;
        max_y = f32::NEG_INFINITY;
        for (x, y) in [(0., 0.), (width, 0.), (width, height), (0., height)] {
            let rx = cos * (x - pivot_x) - sin * (y - pivot_y) + pivot_x;
            let ry = sin * (x - pivot_x) + cos * (y - pivot_y) + pivot_y;
            min_x = min_x.min(rx);
            min_y = min_y.min(ry);
            max_x = max_x.max(rx);
            max_y = max_y.max(ry);
        }
        min_x = min_x.floor();
        min_y = min_y.floor();
        max_x = max_x.ceil();
        max_y = max_y.ceil();
    }
    // Sample straight from the raw buffers: this runs for every frame of the spinning disc,
    // so no per-pixel bounds-checked accessors. Coordinates use the same expression per pixel
    // (not an accumulated step), so edge pixels stay exactly where they were.
    let (out_width, out_height) = ((max_x - min_x) as usize, (max_y - min_y) as usize);
    let (source_width, source_height) = (source.width() as usize, source.height() as usize);
    let input = source.as_raw();
    let mut buffer = vec![0u8; out_width * out_height * 4];
    for (y, row) in buffer.chunks_exact_mut(out_width * 4).enumerate() {
        let dy = y as f32 + min_y + 0.5 - pivot_y;
        for (x, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let dx = x as f32 + min_x + 0.5 - pivot_x;
            let sx = cos * dx + sin * dy + pivot_x - 0.5;
            let sy = -sin * dx + cos * dy + pivot_y - 0.5;
            if sx >= 0. && sy >= 0. && sx <= width - 1. && sy <= height - 1. {
                let (x0, y0) = (sx as usize, sy as usize);
                let (x1, y1) = (
                    (x0 + 1).min(source_width - 1),
                    (y0 + 1).min(source_height - 1),
                );
                let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
                let at = |x: usize, y: usize| (y * source_width + x) * 4;
                let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
                // RGBA in, BGRA out for GPUI's frame format.
                for (out, channel) in [2, 1, 0, 3].into_iter().enumerate() {
                    let top =
                        input[a + channel] as f32 * (1. - fx) + input[b + channel] as f32 * fx;
                    let bottom =
                        input[c + channel] as f32 * (1. - fx) + input[d + channel] as f32 * fx;
                    pixel[out] = (top * (1. - fy) + bottom * fy).round() as u8;
                }
            }
        }
    }
    let output = image::RgbaImage::from_raw(out_width as u32, out_height as u32, buffer)
        .expect("rotation buffer matches its size");
    (
        Arc::new(RenderImage::new(vec![image::Frame::new(output)])),
        (min_x, min_y),
    )
}

pub(super) fn easing(time: f32, curve: [f32; 4]) -> f32 {
    // CSS cubic-bezier: invert x before evaluating y.
    let cubic = |t: f32, a: f32, b: f32| {
        3. * (1. - t).powi(2) * t * a + 3. * (1. - t) * t * t * b + t * t * t
    };
    let (mut low, mut high) = (0., 1.);
    for _ in 0..12 {
        let mid = (low + high) * 0.5;
        if cubic(mid, curve[0], curve[2]) < time {
            low = mid;
        } else {
            high = mid;
        }
    }
    if time >= 1. {
        1.
    } else {
        cubic((low + high) * 0.5, curve[1], curve[3])
    }
}
