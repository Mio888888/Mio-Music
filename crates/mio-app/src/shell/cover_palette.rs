use super::*;

pub(super) fn decode_image(bytes: Vec<u8>) -> Option<image::DynamicImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().ok()
}

pub(super) fn vinyl_pixels(image: &image::DynamicImage) -> image::RgbaImage {
    let mut pixels = image
        .resize_to_fill(512, 512, image::imageops::FilterType::Triangle)
        .to_rgba8();
    // Original CSS: brightness(.85), contrast(1.15), saturate(1.1).
    for pixel in pixels.pixels_mut() {
        let values = [pixel[0], pixel[1], pixel[2]]
            .map(|value| ((value as f32 / 255. * 0.85 - 0.5) * 1.15 + 0.5).clamp(0., 1.));
        let luminance = values[0] * 0.213 + values[1] * 0.715 + values[2] * 0.072;
        for channel in 0..3 {
            pixel[channel] = ((luminance + (values[channel] - luminance) * 1.1).clamp(0., 1.)
                * 255.)
                .round() as u8;
        }
    }
    pixels
}

// GPUI representation of the shared artwork result.
#[derive(Clone, Copy)]
pub(super) struct CoverPalette {
    pub main: Hsla,
    pub artist: Hsla,
    pub text: Hsla,
    pub hover: Hsla,
    pub use_black_text: bool,
}
pub(super) fn cover_palette(image: &image::DynamicImage) -> CoverPalette {
    let palette = mio_core::cover_palette::cover_palette(image);
    let color = |c: mio_core::cover_palette::Hsla| Hsla {
        h: c.h,
        s: c.s,
        l: c.l,
        a: c.a,
    };
    CoverPalette {
        main: color(palette.main),
        artist: color(palette.artist),
        text: color(palette.text),
        hover: color(palette.hover),
        use_black_text: palette.use_black_text,
    }
}
