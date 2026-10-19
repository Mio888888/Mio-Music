#[cfg(not(any(target_os = "android", target_os = "ios")))]
use super::*;
#[cfg(any(target_os = "android", target_os = "ios"))]
use gpui::{prelude::*, *};
use gpui_kit::base::motion::{Transition, transition};
use std::{sync::Arc, time::Duration};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    image: ImageId,
    width: u32,
    height: u32,
    narrow: bool,
    dark: bool,
}

#[derive(Default)]
pub(super) struct AlbumBackdrop {
    key: Option<Key>,
    busy: bool,
    attempted: bool,
    image: Option<Arc<RenderImage>>,
    pub retired: Vec<Arc<RenderImage>>,
}

impl AlbumBackdrop {
    pub fn clear(&mut self) {
        self.key = None;
        self.attempted = false;
        self.retired.extend(self.image.take());
        // Let the sole old job finish; its key will no longer match.
    }
}

#[derive(IntoElement)]
pub(super) struct Layer {
    pub state: Entity<AlbumBackdrop>,
    pub cover: Option<Arc<RenderImage>>,
    pub frame: std::rc::Rc<std::cell::Cell<usize>>,
    pub narrow: bool,
    pub dark: bool,
    pub corners: Corners<Pixels>,
}

impl RenderOnce for Layer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.read(cx);
        let loaded = state.image.is_some()
            && state.key.is_some_and(|key| {
                self.cover
                    .as_ref()
                    .is_some_and(|cover| cover.id == key.image)
            });
        let fade_id = SharedString::from(format!(
            "album-cover-background-{}",
            self.cover.as_ref().map_or(usize::MAX, |cover| cover.id.0)
        ));
        let opacity = transition(
            (fade_id, "opacity"),
            if loaded { 1_f32 } else { 0. },
            Transition::new(Duration::from_millis(800)).ease(ease_out),
            window,
            cx,
        );
        let state = self.state;
        let narrow = self.narrow;
        let dark = self.dark;
        let corners = self.corners;
        let frame = self.frame;
        let image = canvas(
            move |bounds, _, cx| {
                state.update(cx, |state, cx| {
                    let Some(cover) = self.cover else {
                        state.clear();
                        return None;
                    };
                    let key = Key {
                        image: cover.id,
                        width: f32::from(bounds.size.width).ceil().max(1.) as u32,
                        height: f32::from(bounds.size.height).ceil().max(1.) as u32,
                        narrow,
                        dark,
                    };
                    if state.key != Some(key) {
                        if state.key.is_some_and(|old| old.image == key.image) {
                            // Keep this cover visible while adapting its crop/filter.
                            state.attempted = false;
                        } else {
                            state.clear();
                        }
                        state.key = Some(key);
                    }
                    if !state.busy && !state.attempted {
                        state.busy = true;
                        state.attempted = true;
                        let task = cx
                            .background_executor()
                            .spawn(async move { rasterize(&cover, key) });
                        cx.spawn(async move |state, cx| {
                            let image = task.await;
                            let _ = state.update(cx, |state, cx| {
                                state.busy = false;
                                if state.key == Some(key) {
                                    state.retired.extend(state.image.take());
                                    state.image = image;
                                }
                                cx.notify();
                            });
                        })
                        .detach();
                    }
                    state.image.clone().map(|image| {
                        let index = frame.get().min(image.frame_count().saturating_sub(1));
                        (image, index)
                    })
                })
            },
            move |bounds, image, window, _| {
                if let Some((image, frame)) = image {
                    // CSS transforms the already blurred and masked pseudo-element.
                    let scale = if narrow { 1.08 } else { 1.1 };
                    let expanded = bounds.size.map(|v| v * scale);
                    let image_bounds = Bounds::new(
                        bounds.center() - point(expanded.width / 2., expanded.height / 2.),
                        expanded,
                    );
                    let _ = window.paint_image(bounds, image_bounds, corners, image, frame, false);
                }
            },
        )
        .size_full();
        div()
            .absolute()
            .inset_0()
            .size_full()
            .opacity(opacity)
            .child(image)
    }
}

fn rasterize(cover: &RenderImage, key: Key) -> Option<Arc<RenderImage>> {
    let count = cover.frame_count();
    if count == 0 || count > 1024 {
        return None;
    }
    // Bound all retained blur frames together; keep static-image resolution.
    let budget = 64 * 1024 * 1024_u64;
    let resolution = (2048. / key.width as f64)
        .min(1024. / key.height as f64)
        .min((budget as f64 / (4. * count as f64 * key.width as f64 * key.height as f64)).sqrt())
        .min(1.);
    let width = (key.width as f64 * resolution).floor().max(1.) as u32;
    let height = (key.height as f64 * resolution).floor().max(1.) as u32;
    if u64::from(width) * u64::from(height) * 4 * count as u64 > budget {
        return None;
    }
    let frames = (0..count)
        .map(|index| {
            rasterize_frame(cover, index, key, width, height, resolution as f32)
                .map(|pixels| image::Frame::from_parts(pixels, 0, 0, cover.delay(index)))
        })
        .collect::<Option<Vec<_>>>()?;
    let image = RenderImage::new(frames);
    // The mobile renderer currently has no animation repeat-count API.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let image = image.with_play_count(cover.play_count());
    Some(Arc::new(image))
}

fn rasterize_frame(
    cover: &RenderImage,
    index: usize,
    key: Key,
    width: u32,
    height: u32,
    resolution: f32,
) -> Option<image::RgbaImage> {
    let source_size = cover.size(index);
    let source_bytes = cover.as_bytes(index)?;
    if source_bytes.len() > 16 * 1024 * 1024 {
        return None;
    }
    let mut source = image::RgbaImage::from_raw(
        source_size.width.0.try_into().ok()?,
        source_size.height.0.try_into().ok()?,
        source_bytes.to_vec(),
    )?;
    if source.width() == 0 || source.height() == 0 {
        return None;
    }
    for pixel in source.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    let fit = (width as f32 / source.width() as f32).max(height as f32 / source.height() as f32);
    // Sample only the visible top-centred crop: an extreme source aspect ratio
    // must not allocate an enormous intermediate cover-sized image.
    let x_origin = (source.width() as f32 - width as f32 / fit) / 2.;
    let mut pixels = image::RgbaImage::from_fn(width, height, |x, y| {
        let x = (x_origin + (x as f32 + 0.5) / fit - 0.5).clamp(0., (source.width() - 1) as f32);
        let y = ((y as f32 + 0.5) / fit - 0.5).clamp(0., (source.height() - 1) as f32);
        let (left, top) = (x.floor() as u32, y.floor() as u32);
        let (right, bottom) = (
            (left + 1).min(source.width() - 1),
            (top + 1).min(source.height() - 1),
        );
        let (dx, dy) = (x.fract(), y.fract());
        image::Rgba(std::array::from_fn(|c| {
            let upper =
                source[(left, top)][c] as f32 * (1. - dx) + source[(right, top)][c] as f32 * dx;
            let lower = source[(left, bottom)][c] as f32 * (1. - dx)
                + source[(right, bottom)][c] as f32 * dx;
            (upper * (1. - dy) + lower * dy).round() as u8
        }))
    });
    pixels = image::imageops::blur(&pixels, 10. * resolution);
    for (_, y, pixel) in pixels.enumerate_pixels_mut() {
        let rgb = [pixel[0] as f32, pixel[1] as f32, pixel[2] as f32];
        let luminance = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        for (c, value) in rgb.into_iter().enumerate() {
            let value = if key.narrow {
                luminance + (value - luminance) * 1.2
            } else if key.dark {
                (value * 0.5 + luminance * 0.5) * 0.6
            } else {
                value
            };
            pixel[c] = value.round().clamp(0., 255.) as u8;
        }
        let y = (y as f32 + 0.5) / height as f32;
        let (start, middle, stop, end) = if key.narrow {
            (0.72, 0.12, 0.72, 1.)
        } else {
            (0.8, 0.05, 0.6, 0.7)
        };
        let mask = if y <= stop {
            start + (middle - start) * y / stop
        } else {
            middle * (1. - (y - stop) / (end - stop)).max(0.)
        };
        pixel[3] = (pixel[3] as f32 * mask).round() as u8;
        pixel.0.swap(0, 2);
    }
    Some(pixels)
}

// Same CSS ease-out curve on both renderer implementations.
fn ease_out(time: f32) -> f32 {
    let curve = [0., 0., 0.58, 1.];
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
