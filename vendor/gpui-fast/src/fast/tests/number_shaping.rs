// Modified by Longbridge for gpui-fast.
//! Tests of numbers put together from their glyphs rather than shaped by the
//! platform.

use std::{
    borrow::Cow,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::{
    Bounds, DevicePixels, Font, FontId, FontMetrics, FontRun, GlyphId, LineLayout, NoopTextSystem,
    Pixels, PlatformTextSystem, RenderGlyphParams, Result, ShapedGlyph, ShapedRun, Size,
    TextRenderingMode,
    fast::number_shaping::{CHECKED_FIRST, NumberShaping},
    point, px,
};

const FONT: FontId = FontId(3);

/// A text system shaping as fonts do: each character is a glyph with its own
/// advance, some pairs are kerned, `%%` is one ligature glyph, and, when
/// `in_context` is set, a `-` between two digits becomes another glyph.
#[derive(Default)]
struct Shaper {
    lines: AtomicUsize,
    in_context: AtomicBool,
}

impl Shaper {
    fn advance(ch: char) -> f64 {
        match ch {
            '0'..='9' => 7.25,
            '.' | ',' => 3.5,
            _ => 6.125,
        }
    }

    fn kerning(left: char, right: char) -> f64 {
        match (left, right) {
            ('7', '4') => -0.75,
            ('1', '1') => -0.5,
            ('T', '.') => -1.25,
            _ => 0.,
        }
    }

    fn lines(&self) -> usize {
        self.lines.load(Ordering::Relaxed)
    }
}

impl PlatformTextSystem for Shaper {
    fn add_fonts(&self, _: Vec<Cow<'static, [u8]>>) -> Result<()> {
        Ok(())
    }

    fn all_font_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn font_id(&self, _: &Font) -> Result<FontId> {
        Ok(FONT)
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        NoopTextSystem.font_metrics(font_id)
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        NoopTextSystem.typographic_bounds(font_id, glyph_id)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        NoopTextSystem.advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, _: FontId, ch: char) -> Option<GlyphId> {
        Some(GlyphId(ch as u32))
    }

    fn glyph_raster_bounds(&self, _: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        Ok(Default::default())
    }

    fn rasterize_glyph(
        &self,
        _: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        Ok((raster_bounds.size, Vec::new()))
    }

    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        self.lines.fetch_add(1, Ordering::Relaxed);
        let chars: Vec<char> = text.chars().collect();
        let in_context = self.in_context.load(Ordering::Relaxed);
        let mut glyphs = Vec::new();
        let mut x = 0f64;
        let mut ix = 0;
        while ix < chars.len() {
            let ch = chars[ix];
            if ch == '%' && chars.get(ix + 1) == Some(&'%') {
                glyphs.push(glyph(1000, x, ix));
                x += 9.;
                ix += 2;
                continue;
            }
            let between_digits = ix > 0
                && chars[ix - 1].is_ascii_digit()
                && chars.get(ix + 1).is_some_and(char::is_ascii_digit);
            let id = if in_context && ch == '-' && between_digits {
                2000
            } else {
                ch as u32
            };
            glyphs.push(glyph(id, x, ix));
            x += Self::advance(ch);
            if let Some(&next) = chars.get(ix + 1) {
                x += Self::kerning(ch, next);
            }
            ix += 1;
        }
        LineLayout {
            font_size,
            width: px(x as f32),
            ascent: px(9.),
            descent: px(3.),
            runs: vec![ShapedRun {
                font_id: runs[0].font_id,
                glyphs,
            }],
            len: text.len(),
        }
    }

    fn recommended_rendering_mode(&self, _: FontId, _: Pixels) -> TextRenderingMode {
        TextRenderingMode::Grayscale
    }
}

fn glyph(id: u32, x: f64, index: usize) -> ShapedGlyph {
    ShapedGlyph {
        id: GlyphId(id),
        position: point(px(x as f32), px(0.)),
        index,
        is_emoji: false,
    }
}

fn run(text: &str) -> [FontRun; 1] {
    [FontRun {
        len: text.len(),
        font_id: FONT,
    }]
}

/// Asserts that a line put together is the one the platform shapes.
fn assert_shaped_alike(ours: &LineLayout, theirs: &LineLayout, text: &str) {
    let close = |a: Pixels, b: Pixels| (a.0 - b.0).abs() <= 1e-3;
    assert!(close(ours.width, theirs.width), "{text}: width");
    assert_eq!((ours.ascent, ours.descent), (theirs.ascent, theirs.descent));
    assert_eq!(ours.len, theirs.len);
    let [ours] = ours.runs.as_slice() else {
        panic!("{text}: one run");
    };
    let [theirs] = theirs.runs.as_slice() else {
        panic!("{text}: one run");
    };
    assert_eq!(ours.font_id, theirs.font_id);
    assert_eq!(ours.glyphs.len(), theirs.glyphs.len(), "{text}: glyphs");
    for (ours, theirs) in ours.glyphs.iter().zip(&theirs.glyphs) {
        assert_eq!((ours.id, ours.index), (theirs.id, theirs.index), "{text}");
        assert!(
            close(ours.position.x, theirs.position.x),
            "{text}: position"
        );
    }
}

/// A number is put together as the platform shapes it, kerning included,
/// and once its characters and pairs are measured and the font has been
/// checked, a new number is put together without the platform.
#[test]
fn numbers_are_put_together_as_the_platform_shapes_them() {
    let shaper = Shaper::default();
    let mut shaping = NumberShaping::default();
    let size = px(13.);
    let numbers: Vec<String> = (0..CHECKED_FIRST as u64 + 40)
        .map(|n| match n % 4 {
            0 => format!("{:.2}", 7400.0 + n as f64 * 1.11),
            1 => format!("{:+.2}%", n as f64 * 0.7 - 11.0),
            2 => format!("{:.2}T", 1.0 + n as f64),
            _ => format!("${}", 11_747 + n),
        })
        .collect();

    for text in &numbers {
        let ours = shaping.shape(&shaper, text, size, &run(text)).unwrap();
        let theirs = shaper.layout_line(text, size, &run(text));
        assert_shaped_alike(&ours, &theirs, text);
    }

    let before = shaper.lines();
    let ours = shaping.shape(&shaper, "7411.47", size, &run("7411.47"));
    assert_eq!(shaper.lines(), before, "a checked font needs no platform");
    assert_shaped_alike(
        &ours.unwrap(),
        &shaper.layout_line("7411.47", size, &run("7411.47")),
        "7411.47",
    );
}

/// A pair the platform shapes as one glyph is never put together.
#[test]
fn a_pair_the_platform_joins_is_left_to_it() {
    let shaper = Shaper::default();
    let mut shaping = NumberShaping::default();
    assert!(
        shaping
            .shape(&shaper, "5%%", px(13.), &run("5%%"))
            .is_none()
    );
    // Numbers without the pair still are.
    assert!(shaping.shape(&shaper, "5%", px(13.), &run("5%")).is_some());
}

/// A font that shapes a character by more than its neighbours is found out
/// by the check, answered with what the platform shaped, and left to the
/// platform from then on.
#[test]
fn a_font_shaping_in_context_is_left_to_the_platform() {
    let shaper = Shaper::default();
    shaper.in_context.store(true, Ordering::Relaxed);
    let mut shaping = NumberShaping::default();
    let size = px(13.);

    let shaped = shaping.shape(&shaper, "1-2", size, &run("1-2")).unwrap();
    assert_eq!(shaped.runs[0].glyphs[1].id, GlyphId(2000));
    assert!(shaping.shape(&shaper, "12", size, &run("12")).is_none());
    // Another size of the font is measured and checked on its own.
    assert!(shaping.shape(&shaper, "12", px(14.), &run("12")).is_some());
}

/// Only single runs of the characters numbers are written with are put
/// together.
#[test]
fn lines_other_than_numbers_are_left_to_the_platform() {
    let shaper = Shaper::default();
    let mut shaping = NumberShaping::default();
    let size = px(13.);
    for text in ["", "12:30", "abc", "1 234", "½", &"1".repeat(65)] {
        assert!(
            shaping.shape(&shaper, text, size, &run(text)).is_none(),
            "{text:?}"
        );
    }
    let two_runs = [
        FontRun {
            len: 1,
            font_id: FONT,
        },
        FontRun {
            len: 1,
            font_id: FONT,
        },
    ];
    assert!(shaping.shape(&shaper, "12", size, &two_runs).is_none());
}
