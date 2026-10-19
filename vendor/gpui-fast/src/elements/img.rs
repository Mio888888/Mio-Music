use crate::{
    AnyElement, AnyImageCache, App, Asset, AssetLogger, Bounds, DefiniteLength, Element, ElementId,
    Entity, GlobalElementId, Hitbox, Image, ImageCache, InspectorElementId, InteractiveElement,
    Interactivity, IntoElement, LayoutId, Length, ObjectFit, Pixels, RenderImage, Resource,
    SharedString, SharedUri, StyleRefinement, Styled, Task, Window, px,
};
use anyhow::Result;

use futures::Future;
use gpui_util::ResultExt;
use image::{AnimationDecoder, ImageDecoder as _, ImageError, Rgba};
use scheduler::Instant;
use smallvec::SmallVec;
use std::{
    fs,
    io::{self, Cursor},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use thiserror::Error;

use super::{Stateful, StatefulInteractiveElement};

/// The delay before showing the loading state.
pub const LOADING_DELAY: Duration = Duration::from_millis(200);

/// A type alias to the resource loader that the `img()` element uses.
///
/// Note: that this is only for Resources, like URLs or file paths.
/// Custom loaders, or external images will not use this asset loader
pub type ImgResourceLoader = AssetLogger<ImageAssetLoader>;

/// A source of image content.
#[derive(Clone)]
pub enum ImageSource {
    /// The image content will be loaded from some resource location
    Resource(Resource),
    /// Cached image data
    Render(Arc<RenderImage>),
    /// Cached image data
    Image(Arc<Image>),
    /// A custom loading function to use
    Custom(Arc<dyn Fn(&mut Window, &mut App) -> Option<Result<Arc<RenderImage>, ImageCacheError>>>),
}

fn is_uri(uri: &str) -> bool {
    url::Url::from_str(uri).is_ok()
}

impl From<SharedUri> for ImageSource {
    fn from(value: SharedUri) -> Self {
        Self::Resource(Resource::Uri(value))
    }
}

impl<'a> From<&'a str> for ImageSource {
    fn from(s: &'a str) -> Self {
        if is_uri(s) {
            Self::Resource(Resource::Uri(s.to_string().into()))
        } else {
            Self::Resource(Resource::Embedded(s.to_string().into()))
        }
    }
}

impl From<String> for ImageSource {
    fn from(s: String) -> Self {
        if is_uri(&s) {
            Self::Resource(Resource::Uri(s.into()))
        } else {
            Self::Resource(Resource::Embedded(s.into()))
        }
    }
}

impl From<SharedString> for ImageSource {
    fn from(s: SharedString) -> Self {
        s.as_ref().into()
    }
}

impl From<&Path> for ImageSource {
    fn from(value: &Path) -> Self {
        Self::Resource(value.to_path_buf().into())
    }
}

impl From<Arc<Path>> for ImageSource {
    fn from(value: Arc<Path>) -> Self {
        Self::Resource(value.into())
    }
}

impl From<PathBuf> for ImageSource {
    fn from(value: PathBuf) -> Self {
        Self::Resource(value.into())
    }
}

impl From<Arc<RenderImage>> for ImageSource {
    fn from(value: Arc<RenderImage>) -> Self {
        Self::Render(value)
    }
}

impl From<Arc<Image>> for ImageSource {
    fn from(value: Arc<Image>) -> Self {
        Self::Image(value)
    }
}

impl<F> From<F> for ImageSource
where
    F: Fn(&mut Window, &mut App) -> Option<Result<Arc<RenderImage>, ImageCacheError>> + 'static,
{
    fn from(value: F) -> Self {
        Self::Custom(Arc::new(value))
    }
}

/// The style of an image element.
pub struct ImageStyle {
    grayscale: bool,
    object_fit: ObjectFit,
    image_scale: f32,
    on_frame: Option<Box<dyn Fn(usize)>>,
    loading: Option<Box<dyn Fn() -> AnyElement>>,
    fallback: Option<Box<dyn Fn() -> AnyElement>>,
}

impl Default for ImageStyle {
    fn default() -> Self {
        Self {
            grayscale: false,
            object_fit: ObjectFit::Contain,
            image_scale: 1.,
            on_frame: None,
            loading: None,
            fallback: None,
        }
    }
}

/// Style an image element.
pub trait StyledImage: Sized {
    /// Get a mutable [ImageStyle] from the element.
    fn image_style(&mut self) -> &mut ImageStyle;

    /// Set the image to be displayed in grayscale.
    fn grayscale(mut self, grayscale: bool) -> Self {
        self.image_style().grayscale = grayscale;
        self
    }

    /// Set the object fit for the image.
    fn object_fit(mut self, object_fit: ObjectFit) -> Self {
        self.image_style().object_fit = object_fit;
        self
    }

    /// Scale the fitted pixels around their center without changing layout or clipping.
    fn image_scale(mut self, scale: f32) -> Self {
        self.image_style().image_scale = if scale.is_finite() && scale > 0. {
            scale
        } else {
            1.
        };
        self
    }

    /// Observe the selected frame during layout, before any element prepaints.
    fn on_frame(mut self, callback: impl Fn(usize) + 'static) -> Self {
        self.image_style().on_frame = Some(Box::new(callback));
        self
    }

    /// Set a fallback function that will be invoked to render an error view should
    /// the image fail to load.
    fn with_fallback(mut self, fallback: impl Fn() -> AnyElement + 'static) -> Self {
        self.image_style().fallback = Some(Box::new(fallback));
        self
    }

    /// Set a fallback function that will be invoked to render a view while the image
    /// is still being loaded.
    fn with_loading(mut self, loading: impl Fn() -> AnyElement + 'static) -> Self {
        self.image_style().loading = Some(Box::new(loading));
        self
    }
}

impl StyledImage for Img {
    fn image_style(&mut self) -> &mut ImageStyle {
        &mut self.style
    }
}

impl StyledImage for Stateful<Img> {
    fn image_style(&mut self) -> &mut ImageStyle {
        &mut self.element.style
    }
}

/// An image element.
pub struct Img {
    interactivity: Interactivity,
    source: ImageSource,
    style: ImageStyle,
    image_cache: Option<AnyImageCache>,
}

/// Create a new image element.
#[track_caller]
pub fn img(source: impl Into<ImageSource>) -> Img {
    Img {
        interactivity: Interactivity::new(),
        source: source.into(),
        style: ImageStyle::default(),
        image_cache: None,
    }
}

impl Img {
    /// A list of all format extensions currently supported by this img element
    pub fn extensions() -> &'static [&'static str] {
        // This is the list in [image::ImageFormat::from_extension] + `svg`
        &[
            "avif", "jpg", "jpeg", "png", "gif", "webp", "tif", "tiff", "tga", "dds", "bmp", "ico",
            "hdr", "exr", "pbm", "pam", "ppm", "pgm", "ff", "farbfeld", "qoi", "svg", "svgz",
        ]
    }

    /// Sets the image cache for the current node.
    ///
    /// If the `image_cache` is not explicitly provided, the function will determine the image cache by:
    ///
    /// 1. Checking if any ancestor node of the current node contains an `ImageCacheElement`, If such a node exists, the image cache specified by that ancestor will be used.
    /// 2. If no ancestor node contains an `ImageCacheElement`, the global image cache will be used as a fallback.
    ///
    /// This mechanism provides a flexible way to manage image caching, allowing precise control when needed,
    /// while ensuring a default behavior when no cache is explicitly specified.
    #[inline]
    pub fn image_cache<I: ImageCache>(self, image_cache: &Entity<I>) -> Self {
        Self {
            image_cache: Some(image_cache.clone().into()),
            ..self
        }
    }
}

impl Deref for Stateful<Img> {
    type Target = Img;

    fn deref(&self) -> &Self::Target {
        &self.element
    }
}

impl DerefMut for Stateful<Img> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.element
    }
}

/// The image state between frames
struct ImgState {
    image_id: Option<crate::ImageId>,
    frame_index: usize,
    completed_plays: u32,
    finished: bool,
    last_frame_time: Option<Instant>,
    started_loading: Option<(Instant, Task<()>)>,
}

/// The image layout state between frames
pub struct ImgLayoutState {
    frame_index: usize,
    animate: bool,
    replacement: Option<AnyElement>,
}

impl Element for Img {
    type RequestLayoutState = ImgLayoutState;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        self.interactivity.element_id.clone()
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        self.interactivity.source_location()
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut layout_state = ImgLayoutState {
            frame_index: 0,
            animate: false,
            replacement: None,
        };

        window.with_optional_element_state(global_id, |state, window| {
            let mut state = state.map(|state| {
                state.unwrap_or(ImgState {
                    image_id: None,
                    frame_index: 0,
                    completed_plays: 0,
                    finished: false,
                    last_frame_time: None,
                    started_loading: None,
                })
            });

            let mut frame_index = state.as_ref().map(|state| state.frame_index).unwrap_or(0);

            let layout_id = self.interactivity.request_layout(
                global_id,
                inspector_id,
                window,
                cx,
                |mut style, window, cx| {
                    let mut replacement_id = None;

                    match self.source.use_data(
                        self.image_cache
                            .clone()
                            .or_else(|| window.image_cache_stack.last().cloned()),
                        window,
                        cx,
                    ) {
                        Some(Ok(data)) => {
                            let frame_count = data.frame_count();
                            let max_frame_index = frame_count.saturating_sub(1);

                            if let Some(state) = &mut state {
                                if state.image_id != Some(data.id) {
                                    state.image_id = Some(data.id);
                                    state.frame_index = 0;
                                    state.completed_plays = 0;
                                    state.finished = false;
                                    state.last_frame_time = None;
                                }
                                state.frame_index = state.frame_index.min(max_frame_index);
                                if frame_count > 1 && !cx.reduce_motion() && !state.finished {
                                    if window.is_window_active() {
                                        let current_time = Instant::now();
                                        if let Some(last_frame_time) = state.last_frame_time {
                                            let elapsed = current_time - last_frame_time;
                                            let frame_duration =
                                                Duration::from(data.delay(state.frame_index));

                                            if elapsed >= frame_duration {
                                                if state.frame_index == max_frame_index {
                                                    state.completed_plays =
                                                        state.completed_plays.saturating_add(1);
                                                    state.finished = matches!(
                                                        data.play_count(),
                                                        image::metadata::LoopCount::Finite(count)
                                                            if state.completed_plays >= count.get()
                                                    );
                                                }
                                                if state.finished {
                                                    state.last_frame_time = None;
                                                } else {
                                                    state.frame_index =
                                                        (state.frame_index + 1) % frame_count;
                                                    state.last_frame_time = Some(
                                                        current_time - (elapsed - frame_duration),
                                                    );
                                                }
                                            }
                                        } else {
                                            state.last_frame_time = Some(current_time);
                                        }
                                    } else {
                                        state.last_frame_time = None;
                                    }
                                } else {
                                    state.last_frame_time = None;
                                }
                                state.started_loading = None;
                                frame_index = state.frame_index;
                            }

                            let image_size = data.render_size(frame_index);

                            if style.aspect_ratio.is_none() {
                                style.aspect_ratio = Some(image_size.width / image_size.height);
                            }

                            if let Length::Auto = style.size.width {
                                style.size.width = match style.size.height {
                                    Length::Definite(DefiniteLength::Absolute(abs_length)) => {
                                        let height_px = abs_length.to_pixels(window.rem_size());
                                        Length::Definite(
                                            px(image_size.width.0 * height_px.0
                                                / image_size.height.0)
                                            .into(),
                                        )
                                    }
                                    _ => Length::Definite(image_size.width.into()),
                                };
                            }

                            if let Length::Auto = style.size.height {
                                style.size.height = match style.size.width {
                                    Length::Definite(DefiniteLength::Absolute(abs_length)) => {
                                        let width_px = abs_length.to_pixels(window.rem_size());
                                        Length::Definite(
                                            px(image_size.height.0 * width_px.0
                                                / image_size.width.0)
                                            .into(),
                                        )
                                    }
                                    _ => Length::Definite(image_size.height.into()),
                                };
                            }

                            if global_id.is_some()
                                && data.frame_count() > 1
                                && window.is_window_active()
                                && !cx.reduce_motion()
                                && state.as_ref().is_some_and(|state| !state.finished)
                            {
                                layout_state.animate = true;
                            }
                        }
                        Some(_err) => {
                            if let Some(fallback) = self.style.fallback.as_ref() {
                                let mut element = fallback();
                                replacement_id = Some(element.request_layout(window, cx));
                                layout_state.replacement = Some(element);
                            }
                            if let Some(state) = &mut state {
                                state.started_loading = None;
                            }
                        }
                        None => {
                            if let Some(state) = &mut state {
                                if let Some((started_loading, _)) = state.started_loading {
                                    if started_loading.elapsed() > LOADING_DELAY
                                        && let Some(loading) = self.style.loading.as_ref()
                                    {
                                        let mut element = loading();
                                        replacement_id = Some(element.request_layout(window, cx));
                                        layout_state.replacement = Some(element);
                                    }
                                } else {
                                    let current_view = window.current_view();
                                    let task = window.spawn(cx, async move |cx| {
                                        cx.background_executor().timer(LOADING_DELAY).await;
                                        cx.update(move |_, cx| {
                                            cx.notify(current_view);
                                        })
                                        .ok();
                                    });
                                    state.started_loading = Some((Instant::now(), task));
                                }
                            }
                        }
                    }

                    window.request_layout(style, replacement_id, cx)
                },
            );

            layout_state.frame_index = frame_index;
            if let Some(on_frame) = &self.style.on_frame {
                on_frame(frame_index);
            }

            ((layout_id, layout_state), state)
        })
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.interactivity.prepaint(
            global_id,
            inspector_id,
            bounds,
            bounds.size,
            window,
            cx,
            |_, _, hitbox, window, cx| {
                if request_layout.animate {
                    if bounds.intersects(&window.content_mask().bounds) {
                        window.request_animation_frame();
                    } else if let Some(global_id) = global_id {
                        window.with_element_state(global_id, |state: Option<ImgState>, _| {
                            let mut state = state.expect("image state initialized in layout");
                            state.last_frame_time = None;
                            ((), state)
                        });
                    }
                }
                if let Some(replacement) = &mut request_layout.replacement {
                    replacement.prepaint(window, cx);
                }

                hitbox
            },
        )
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout_state: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !bounds.intersects(&window.content_mask().bounds) {
            return;
        }
        let source = self.source.clone();
        self.interactivity.paint(
            global_id,
            inspector_id,
            bounds,
            hitbox.as_ref(),
            window,
            cx,
            |style, window, cx| {
                if let Some(Ok(data)) = source.use_data(
                    self.image_cache
                        .clone()
                        .or_else(|| window.image_cache_stack.last().cloned()),
                    window,
                    cx,
                ) {
                    if data.frame_count() == 0 {
                        return;
                    }
                    let new_bounds = self
                        .style
                        .object_fit
                        .get_bounds(bounds, data.size(layout_state.frame_index));
                    let scaled_size = new_bounds.size.map(|v| v * self.style.image_scale);
                    let new_bounds = Bounds::new(
                        new_bounds.center()
                            - crate::point(scaled_size.width * 0.5, scaled_size.height * 0.5),
                        scaled_size,
                    );
                    let corner_radii = style.corner_radii.to_pixels(window.rem_size());
                    window
                        .paint_image(
                            bounds,
                            new_bounds,
                            corner_radii,
                            data,
                            layout_state.frame_index,
                            self.style.grayscale,
                        )
                        .log_err();
                } else if let Some(replacement) = &mut layout_state.replacement {
                    replacement.paint(window, cx);
                }
            },
        )
    }
}

impl Styled for Img {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.interactivity.base_style
    }
}

impl InteractiveElement for Img {
    fn interactivity(&mut self) -> &mut Interactivity {
        &mut self.interactivity
    }
}

impl IntoElement for Img {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl StatefulInteractiveElement for Img {}

impl ImageSource {
    pub(crate) fn use_data(
        &self,
        cache: Option<AnyImageCache>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        match self {
            ImageSource::Resource(resource) => {
                if let Some(cache) = cache {
                    cache.load(resource, window, cx)
                } else {
                    window.use_asset::<ImgResourceLoader>(resource, cx)
                }
            }
            ImageSource::Custom(loading_fn) => loading_fn(window, cx),
            ImageSource::Render(data) => Some(Ok(data.to_owned())),
            ImageSource::Image(data) => window.use_asset::<AssetLogger<ImageDecoder>>(data, cx),
        }
    }

    pub(crate) fn get_data(
        &self,
        cache: Option<AnyImageCache>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        match self {
            ImageSource::Resource(resource) => {
                if let Some(cache) = cache {
                    cache.load(resource, window, cx)
                } else {
                    window.get_asset::<ImgResourceLoader>(resource, cx)
                }
            }
            ImageSource::Custom(loading_fn) => loading_fn(window, cx),
            ImageSource::Render(data) => Some(Ok(data.to_owned())),
            ImageSource::Image(data) => window.get_asset::<AssetLogger<ImageDecoder>>(data, cx),
        }
    }

    /// Remove this image source from the asset system
    pub fn remove_asset(&self, cx: &mut App) {
        match self {
            ImageSource::Resource(resource) => {
                cx.remove_asset::<ImgResourceLoader>(resource);
            }
            ImageSource::Custom(_) | ImageSource::Render(_) => {}
            ImageSource::Image(data) => cx.remove_asset::<AssetLogger<ImageDecoder>>(data),
        }
    }

    /// Check whether this image source is present in the asset system (loading
    /// or loaded), without fetching it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn is_asset_cached(&self, cx: &App) -> bool {
        match self {
            ImageSource::Resource(resource) => cx.has_asset::<ImgResourceLoader>(resource),
            ImageSource::Custom(_) | ImageSource::Render(_) => false,
            ImageSource::Image(data) => cx.has_asset::<AssetLogger<ImageDecoder>>(data),
        }
    }
}

#[derive(Clone)]
enum ImageDecoder {}

impl Asset for ImageDecoder {
    type Source = Arc<Image>;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let renderer = cx.svg_renderer();
        async move { source.to_image_data(renderer).map_err(Into::into) }
    }
}

const MAX_RASTER_INPUT: usize = 128 * 1024 * 1024;

/// Decode a raster asset into bounded BGRA frames, preserving orientation and
/// total animation plays. An optional maximum side thumbnails display frames.
/// Input and static RGBA are limited to 128MiB; animation canvases to 16MiB and
/// retained frames to 64MiB/1024 frames. Decode failure rejects the whole asset.
pub fn decode_raster_image(bytes: &[u8], max_dimension: Option<u32>) -> Option<Arc<RenderImage>> {
    if bytes.len() > MAX_RASTER_INPUT || max_dimension == Some(0) {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decode_raster_inner(bytes, max_dimension)
    }))
    .ok()
    .flatten()
}

fn raster_format(bytes: &[u8]) -> Option<image::ImageFormat> {
    // image::guess_format recognizes classic TIFF, but its TIFF decoder also
    // supports BigTIFF. Keep that clipboard/path support when sharing decoding.
    if matches!(bytes.get(..4)?, b"II+\0" | b"MM\0+") {
        Some(image::ImageFormat::Tiff)
    } else {
        image::guess_format(bytes).ok()
    }
}

fn webp_orientation(bytes: &[u8]) -> Option<image::metadata::Orientation> {
    fn chunks(bytes: &[u8], frame: bool) -> Option<image::metadata::Orientation> {
        let mut orientation = None;
        let mut offset = 0_usize;
        while offset < bytes.len() {
            let header = bytes.get(offset..offset.checked_add(8)?)?;
            let length = u32::from_le_bytes(header.get(4..8)?.try_into().ok()?) as usize;
            let start = offset.checked_add(8)?;
            let end = start.checked_add(length)?;
            let payload = bytes.get(start..end)?;
            offset = end.checked_add(length & 1)?;
            if offset > bytes.len() {
                return None;
            }
            let kind = &header[..4];
            if frame {
                // ANMF permits only encoded pixels and optional alpha. Metadata
                // here can otherwise enter the decoder's global chunk table.
                if !matches!(kind, b"VP8 " | b"VP8L" | b"ALPH") {
                    return None;
                }
            } else if kind == b"ANMF" {
                chunks(payload.get(16..)?, true)?;
            } else if kind == b"EXIF" && orientation.is_none() {
                let exif = payload.strip_prefix(b"Exif\0\0").unwrap_or(payload);
                orientation = Some(
                    image::metadata::Orientation::from_exif_chunk(exif)
                        .unwrap_or(image::metadata::Orientation::NoTransforms),
                );
            }
        }
        Some(orientation.unwrap_or(image::metadata::Orientation::NoTransforms))
    }
    // image's WebP wrapper does not forward allocation limits. Validate every
    // chunk and read only top-level EXIF ourselves. The dependency can mistake
    // an ANMF payload for metadata; never invoke its allocating EXIF reader.
    let declared = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
    if declared.checked_add(8)? != bytes.len() || bytes.get(8..12)? != b"WEBP" {
        return None;
    }
    chunks(bytes.get(12..)?, false)
}

fn png_orientation(bytes: &[u8]) -> Option<image::metadata::Orientation> {
    // A PNG eXIf chunk may appear after IDAT; the decoder's constructor stops
    // at the first IDAT. Inspect bounded chunks without copying image data.
    let mut orientation = image::metadata::Orientation::NoTransforms;
    let mut offset = 8_usize;
    loop {
        let header = bytes.get(offset..offset.checked_add(8)?)?;
        let length = u32::from_be_bytes(header.get(..4)?.try_into().ok()?) as usize;
        let start = offset.checked_add(8)?;
        let end = start.checked_add(length)?;
        let chunk = bytes.get(start..end)?;
        offset = end.checked_add(4)?;
        let crc = u32::from_be_bytes(bytes.get(end..offset)?.try_into().ok()?);
        match &header[4..8] {
            b"eXIf" if crc32fast::hash(bytes.get(start - 4..end)?) == crc => {
                orientation = image::metadata::Orientation::from_exif_chunk(chunk)
                    .unwrap_or(image::metadata::Orientation::NoTransforms);
            }
            b"IEND" => return (length == 0).then_some(orientation),
            _ => {}
        }
    }
}

// image's AVIF constructor decodes before applying Limits. Validate the entire
// primary/alpha AV1 payload with a limited decoder before its color conversion.
// ponytail: remove this extra decode when image exposes dav1d settings.
fn bounded_avif(
    bytes: &[u8],
) -> Option<(image::metadata::Orientation, image::metadata::Orientation)> {
    let context =
        mp4parse::read_avif(&mut Cursor::new(bytes), mp4parse::ParseStrictness::Normal).ok()?;
    let primary = context.primary_item_coded_data()?;
    if primary.is_empty() {
        return None;
    }
    for data in [primary, context.alpha_item_coded_data().unwrap_or_default()] {
        if data.is_empty() {
            continue;
        }
        let mut settings = dav1d::Settings::new();
        settings.set_n_threads(2);
        settings.set_max_frame_delay(1);
        settings.set_frame_size_limit(4 * 1024 * 1024);
        settings.set_all_layers(true);
        settings.set_output_invisible_frames(true);
        let mut decoder = dav1d::Decoder::with_settings(&settings).ok()?;
        let mut pending = match decoder.send_data(data.to_vec(), None, None, None) {
            Ok(()) => false,
            Err(dav1d::Error::Again) => true,
            Err(_) => return None,
        };
        let mut frames = 0;
        let mut attempts = 0;
        loop {
            attempts += 1;
            if attempts > 64 {
                return None;
            }
            match decoder.get_picture() {
                Ok(picture) => {
                    frames += 1;
                    if frames > 1 || picture.width() > 16384 || picture.height() > 16384 {
                        return None;
                    }
                }
                Err(dav1d::Error::Again) if pending => {
                    pending = match decoder.send_pending_data() {
                        Ok(()) => false,
                        Err(dav1d::Error::Again) => true,
                        Err(_) => return None,
                    };
                }
                Err(dav1d::Error::Again) => break,
                Err(_) => return None,
            }
        }
        if frames != 1 {
            return None;
        }
    }
    use image::metadata::Orientation as O;
    let rotation = match context.image_rotation().ok()? {
        mp4parse::ImageRotation::D0 => O::NoTransforms,
        mp4parse::ImageRotation::D90 => O::Rotate270,
        mp4parse::ImageRotation::D180 => O::Rotate180,
        mp4parse::ImageRotation::D270 => O::Rotate90,
    };
    let pointer = context.image_mirror_ptr().ok()?;
    // mp4parse returns a pointer to its owned item property. Context remains
    // alive and unmodified until the property has been read here.
    let mirror = match unsafe { pointer.as_ref() } {
        None => O::NoTransforms,
        Some(mp4parse::ImageMirror::TopBottom) => O::FlipVertical,
        Some(mp4parse::ImageMirror::LeftRight) => O::FlipHorizontal,
    };
    Some((rotation, mirror))
}

fn decode_raster_inner(bytes: &[u8], max_dimension: Option<u32>) -> Option<Arc<RenderImage>> {
    let format = raster_format(bytes)?;
    let avif_transform = if format == image::ImageFormat::Avif {
        Some(bounded_avif(bytes)?)
    } else {
        None
    };
    let input = Cursor::new(bytes);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    let mut pixels = match format {
        image::ImageFormat::Gif => {
            let play_count = gif_play_count(bytes)?;
            let mut decoder = image::codecs::gif::GifDecoder::new(input).ok()?;
            decoder.set_limits(limits).ok()?;
            return animation(decoder, play_count, max_dimension, None);
        }
        image::ImageFormat::WebP => {
            let orientation = webp_orientation(bytes)?;
            let mut decoder = image::codecs::webp::WebPDecoder::new(input).ok()?;
            decoder.set_limits(limits).ok()?;
            if decoder.has_animation() {
                decoder.set_background_color(Rgba([0, 0, 0, 0])).ok()?;
                let play_count = decoder.loop_count();
                return animation(decoder, play_count, max_dimension, Some(orientation));
            }
            static_pixels(decoder, Some(orientation))?
        }
        image::ImageFormat::Png => {
            let orientation = png_orientation(bytes)?;
            let decoder = image::codecs::png::PngDecoder::with_limits(input, limits).ok()?;
            if decoder.is_apng().ok()? {
                animation_dimensions(decoder.dimensions())?;
                let decoder = decoder.apng().ok()?;
                let play_count = decoder.loop_count();
                return animation_frames(
                    decoder.into_frames(),
                    play_count,
                    max_dimension,
                    orientation,
                );
            }
            static_pixels(decoder, Some(orientation))?
        }
        _ => {
            let mut reader = image::ImageReader::with_format(input, format);
            reader.limits(limits);
            static_pixels(reader.into_decoder().ok()?, None)?
        }
    };
    if let Some((rotation, mirror)) = avif_transform {
        pixels.apply_orientation(rotation);
        pixels.apply_orientation(mirror);
    }
    Some(Arc::new(RenderImage::new(vec![render_frame(
        pixels,
        image::Delay::from_numer_denom_ms(0, 1),
        max_dimension,
    )])))
}

fn static_pixels(
    mut decoder: impl image::ImageDecoder,
    orientation: Option<image::metadata::Orientation>,
) -> Option<image::DynamicImage> {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) * 4 > MAX_RASTER_INPUT as u64 {
        return None;
    }
    limits.reserve(decoder.total_bytes()).ok()?;
    decoder.set_limits(limits).ok()?;
    let orientation = match orientation {
        Some(orientation) => orientation,
        None => decoder.orientation().ok()?,
    };
    let mut pixels = image::DynamicImage::from_decoder(decoder).ok()?;
    pixels.apply_orientation(orientation);
    Some(pixels)
}
fn render_frame(
    pixels: image::DynamicImage,
    delay: image::Delay,
    max_dimension: Option<u32>,
) -> image::Frame {
    let mut pixels = match max_dimension {
        Some(side) => pixels.thumbnail(side, side).into_rgba8(),
        None => pixels.into_rgba8(),
    };
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    image::Frame::from_parts(pixels, 0, 0, delay)
}

fn animation_dimensions((width, height): (u32, u32)) -> Option<()> {
    // Decoders may retain several full-size compositing buffers. Reserve 64MiB
    // for them alongside the 64MiB of retained display frames below.
    (width > 0 && height > 0 && u64::from(width) * u64::from(height) * 4 <= 16 * 1024 * 1024)
        .then_some(())
}

fn gif_play_count(mut bytes: &[u8]) -> Option<image::metadata::LoopCount> {
    fn take<'a>(bytes: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
        let part = bytes.get(..len)?;
        *bytes = &bytes[len..];
        Some(part)
    }
    fn color_table(bytes: &mut &[u8], packed: u8) -> Option<()> {
        if packed & 0x80 != 0 {
            take(bytes, 3 * (1 << ((packed & 7) + 1)))?;
        }
        Some(())
    }
    if bytes.len() > MAX_RASTER_INPUT {
        return None;
    }
    if !matches!(take(&mut bytes, 6)?, b"GIF87a" | b"GIF89a") {
        return None;
    }
    let screen = take(&mut bytes, 7)?;
    color_table(&mut bytes, screen[4])?;
    // GIF stores additional repetitions. No extension means a single play.
    let mut plays = image::metadata::LoopCount::Finite(std::num::NonZeroU32::new(1)?);
    let mut frames = 0;
    loop {
        let loop_extension = match take(&mut bytes, 1)?[0] {
            0x3b => return Some(plays),
            0x2c => {
                frames += 1;
                if frames > 1024 {
                    return None;
                }
                let descriptor = take(&mut bytes, 9)?;
                color_table(&mut bytes, descriptor[8])?;
                take(&mut bytes, 1)?; // LZW minimum code size; pixels stay encoded.
                false
            }
            0x21 => {
                if take(&mut bytes, 1)?[0] == 0xff {
                    let len = usize::from(take(&mut bytes, 1)?[0]);
                    if len == 0 {
                        return None;
                    }
                    matches!(take(&mut bytes, len)?, b"NETSCAPE2.0" | b"ANIMEXTS1.0")
                } else {
                    false
                }
            }
            _ => return None,
        };
        // Skip length-delimited data, including compressed frame bytes. Looking
        // for identifiers inside pixels would mistake image data for metadata.
        loop {
            let len = usize::from(take(&mut bytes, 1)?[0]);
            if len == 0 {
                break;
            }
            let block = take(&mut bytes, len)?;
            if loop_extension && block.first() == Some(&1) {
                let repeats = u16::from_le_bytes(block.get(1..3)?.try_into().ok()?);
                plays = if repeats == 0 {
                    image::metadata::LoopCount::Infinite
                } else {
                    image::metadata::LoopCount::Finite(std::num::NonZeroU32::new(
                        u32::from(repeats) + 1,
                    )?)
                };
            }
        }
    }
}

fn animation<'a>(
    mut decoder: impl image::ImageDecoder + AnimationDecoder<'a>,
    play_count: image::metadata::LoopCount,
    max_dimension: Option<u32>,
    orientation: Option<image::metadata::Orientation>,
) -> Option<Arc<RenderImage>> {
    animation_dimensions(decoder.dimensions())?;
    let orientation = match orientation {
        Some(orientation) => orientation,
        None => decoder.orientation().ok()?,
    };
    animation_frames(
        decoder.into_frames(),
        play_count,
        max_dimension,
        orientation,
    )
}

fn animation_frames(
    frames: image::Frames<'_>,
    play_count: image::metadata::LoopCount,
    max_dimension: Option<u32>,
    orientation: image::metadata::Orientation,
) -> Option<Arc<RenderImage>> {
    let mut rendered = SmallVec::<[image::Frame; 1]>::new();
    let mut retained = 0_usize;
    for frame in frames {
        if rendered.len() == 1024 {
            return None;
        }
        let frame = frame.ok()?;
        let delay = frame.delay();
        // Match browser handling of sub-10ms frames rather than spinning at 0ms.
        let delay = if Duration::from(delay) < Duration::from_millis(10) {
            image::Delay::from_numer_denom_ms(100, 1)
        } else {
            delay
        };
        let mut pixels = image::DynamicImage::ImageRgba8(frame.into_buffer());
        pixels.apply_orientation(orientation);
        let frame = render_frame(pixels, delay, max_dimension);
        retained = retained.checked_add(frame.buffer().len())?;
        if retained > 64 * 1024 * 1024 {
            return None;
        }
        rendered.push(frame);
    }
    (!rendered.is_empty()).then(|| Arc::new(RenderImage::new(rendered).with_play_count(play_count)))
}

/// An image loader for the GPUI asset system
#[derive(Clone)]
pub enum ImageAssetLoader {}

impl Asset for ImageAssetLoader {
    type Source = Resource;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let client = cx.http_client();
        // TODO: Can we make SVGs always rescale?
        // let scale_factor = cx.scale_factor();
        let svg_renderer = cx.svg_renderer();
        let asset_source = cx.asset_source().clone();
        async move {
            let bytes = match source.clone() {
                Resource::Path(uri) => {
                    use std::io::Read as _;
                    let mut bytes = Vec::new();
                    fs::File::open(uri.as_ref())?
                        .take((MAX_RASTER_INPUT + 1) as u64)
                        .read_to_end(&mut bytes)?;
                    bytes
                }
                Resource::Uri(uri) => {
                    use anyhow::Context as _;
                    use futures::AsyncReadExt as _;

                    let mut response = client
                        .get(uri.as_ref(), ().into(), true)
                        .await
                        .with_context(|| format!("loading image asset from {uri:?}"))?;
                    let mut body = Vec::new();
                    response
                        .body_mut()
                        .take((MAX_RASTER_INPUT + 1) as u64)
                        .read_to_end(&mut body)
                        .await?;
                    if !response.status().is_success() {
                        let mut body = String::from_utf8_lossy(&body).into_owned();
                        let first_line = body.lines().next().unwrap_or("").trim_end();
                        body.truncate(first_line.len());
                        return Err(ImageCacheError::BadStatus {
                            uri,
                            status: response.status(),
                            body,
                        });
                    }
                    body
                }
                Resource::Embedded(path) => {
                    let data = asset_source.load(&path).ok().flatten();
                    if let Some(data) = data {
                        if data.len() > MAX_RASTER_INPUT {
                            return Err(ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                                "Image input exceeds 128MiB"
                            ))));
                        }
                        data.to_vec()
                    } else {
                        return Err(ImageCacheError::Asset(
                            format!("Embedded resource not found: {}", path).into(),
                        ));
                    }
                }
            };

            if bytes.len() > MAX_RASTER_INPUT {
                return Err(ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                    "Image input exceeds 128MiB"
                ))));
            }
            if raster_format(&bytes).is_some() {
                decode_raster_image(&bytes, None).ok_or_else(|| {
                    ImageCacheError::Other(Arc::new(anyhow::anyhow!(
                        "Image could not be decoded within resource limits"
                    )))
                })
            } else {
                svg_renderer
                    .render_inline_image(&bytes, None)
                    .map_err(Into::into)
            }
        }
    }
}

/// An error that can occur when interacting with the image cache.
#[derive(Debug, Error, Clone)]
pub enum ImageCacheError {
    /// Some other kind of error occurred
    #[error("error: {0}")]
    Other(#[from] Arc<anyhow::Error>),
    /// An error that occurred while reading the image from disk.
    #[error("IO error: {0}")]
    Io(Arc<std::io::Error>),
    /// An error that occurred while processing an image.
    #[error("unexpected http status for {uri}: {status}, body: {body}")]
    BadStatus {
        /// The URI of the image.
        uri: SharedUri,
        /// The HTTP status code.
        status: http_client::StatusCode,
        /// The HTTP response body.
        body: String,
    },
    /// An error that occurred while processing an asset.
    #[error("asset error: {0}")]
    Asset(SharedString),
    /// An error that occurred while processing an image.
    #[error("image error: {0}")]
    Image(Arc<ImageError>),
    /// An error that occurred while processing an SVG.
    #[error("svg error: {0}")]
    Usvg(Arc<usvg::Error>),
}

impl From<anyhow::Error> for ImageCacheError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(Arc::new(value))
    }
}

impl From<io::Error> for ImageCacheError {
    fn from(value: io::Error) -> Self {
        Self::Io(Arc::new(value))
    }
}

impl From<usvg::Error> for ImageCacheError {
    fn from(value: usvg::Error) -> Self {
        Self::Usvg(Arc::new(value))
    }
}

impl From<image::ImageError> for ImageCacheError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(Arc::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ParentElement as _, TestAppContext, canvas, div, point, px, size};
    use image::{Frame, ImageBuffer, Rgba};

    const TEST_IMG_ID: &str = "test-img";

    fn test_image(frame_count: usize) -> Arc<RenderImage> {
        let frame = Frame::new(ImageBuffer::from_pixel(1, 1, Rgba([0, 0, 0, 0])));
        Arc::new(RenderImage::new(SmallVec::from_iter(
            (0..frame_count).map(|_| frame.clone()),
        )))
    }

    fn test_image_with_size(width: u32, height: u32) -> Arc<RenderImage> {
        let frame = Frame::new(ImageBuffer::from_pixel(width, height, Rgba([0, 0, 0, 0])));
        Arc::new(RenderImage::new(SmallVec::from_elem(frame, 1)))
    }

    /// Overwrites the cached `frame_index` of the sibling `img` during paint.
    fn seed_frame_index(frame_index: usize) -> impl IntoElement {
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.with_global_id(TEST_IMG_ID.into(), |id, window| {
                    window.with_element_state::<ImgState, _>(id, |state, _| {
                        let mut state = state.expect("img state should be initialized");
                        state.frame_index = frame_index;
                        ((), state)
                    });
                });
            },
        )
    }

    #[gpui::test]
    fn zero_frame_image_does_not_panic_on_paint(cx: &mut TestAppContext) {
        cx.add_empty_window()
            .draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
                img(ImageSource::Render(test_image(0))).into_any_element()
            });
    }

    #[gpui::test]
    fn image_object_fit_cover_crops_to_element_bounds(cx: &mut TestAppContext) {
        let window = cx.add_empty_window();
        let image = test_image_with_size(200, 100);
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            img(ImageSource::Render(image.clone()))
                .size_full()
                .object_fit(ObjectFit::Fill)
                .into_any_element()
        });
        let full_tile_bounds = window.update(|window, _| {
            window
                .rendered_frame
                .scene
                .polychrome_sprites
                .last()
                .expect("fill image should paint a sprite")
                .tile
                .bounds
        });

        window.draw(point(px(10.), px(20.)), size(px(100.), px(100.)), |_, _| {
            img(ImageSource::Render(image))
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        });

        let (rendered_bounds, rendered_tile_bounds, scale_factor) = window.update(|window, _| {
            let sprite = window
                .rendered_frame
                .scene
                .polychrome_sprites
                .last()
                .expect("cover image should paint a sprite");
            (sprite.bounds, sprite.tile.bounds, window.scale_factor())
        });
        assert_eq!(
            rendered_bounds,
            Bounds {
                origin: point(px(10.).scale(scale_factor), px(20.).scale(scale_factor)),
                size: size(px(100.).scale(scale_factor), px(100.).scale(scale_factor)),
            }
        );
        assert_eq!(
            (
                rendered_tile_bounds.origin.x.0 - full_tile_bounds.origin.x.0,
                rendered_tile_bounds.origin.y.0 - full_tile_bounds.origin.y.0,
                rendered_tile_bounds.size.width.0,
                rendered_tile_bounds.size.height.0,
            ),
            (50, 0, 100, 100),
        );
    }

    #[gpui::test]
    fn explicit_aspect_ratio_is_not_overridden_by_intrinsic_ratio(cx: &mut TestAppContext) {
        let window = cx.add_empty_window();

        // A portrait image in a square container
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            div()
                .size(px(100.))
                .overflow_hidden()
                .child(
                    img(ImageSource::Render(test_image_with_size(100, 200)))
                        .size_full()
                        .aspect_square()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element()
        });

        let (rendered_bounds, scale_factor) = window.update(|window, _| {
            let sprite = window
                .rendered_frame
                .scene
                .polychrome_sprites
                .last()
                .expect("contained image should paint a sprite");
            (sprite.bounds, window.scale_factor())
        });

        // The element stays 100x100, so the image is letterboxed horizontally
        assert_eq!(
            rendered_bounds,
            Bounds {
                origin: point(px(25.).scale(scale_factor), px(0.).scale(scale_factor)),
                size: size(px(50.).scale(scale_factor), px(100.).scale(scale_factor)),
            }
        );
    }

    #[gpui::test]
    fn image_object_fit_cover_clamps_corner_radii_to_visible_bounds(cx: &mut TestAppContext) {
        let window = cx.add_empty_window();
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            img(ImageSource::Render(test_image_with_size(200, 100)))
                .size_full()
                .rounded(px(100.))
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        });

        let (corner_radius, expected_corner_radius) = window.update(|window, _| {
            (
                window
                    .rendered_frame
                    .scene
                    .polychrome_sprites
                    .last()
                    .map(|sprite| sprite.corner_radii.top_left),
                px(50.).scale(window.scale_factor()),
            )
        });
        assert_eq!(corner_radius, Some(expected_corner_radius));
    }

    #[gpui::test]
    fn stale_frame_index_is_clamped_when_image_changes(cx: &mut TestAppContext) {
        let window = cx.add_empty_window();

        // Assert that a cached frame_index from a previous multi-frame image
        // does not cause an out-of-bounds panic when the image is replaced
        // with one that has fewer frames.
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            div()
                .child(img(ImageSource::Render(test_image(5))).id(TEST_IMG_ID))
                .child(seed_frame_index(4))
                .into_any_element()
        });
        window.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
            img(ImageSource::Render(test_image(1)))
                .id(TEST_IMG_ID)
                .into_any_element()
        });
    }
}
