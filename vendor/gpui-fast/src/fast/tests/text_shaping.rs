// Modified by Longbridge for gpui-fast.
//! Tests of what text layout and painting remember: the text styles the
//! window's text style stack resolves, the lines shaped lately, and the
//! glyphs' raster bounds.

use crate::{
    Bounds, DecorationRun, DevicePixels, FontId, FontRun, FontWeight, GlyphId, LineLayout,
    RenderGlyphParams, ShapedGlyph, ShapedRun, TextAlign, TextStyle, TextStyleRefinement,
    fast::glyphs::{GlyphBoundsCache, has_no_background, may_reach},
    fast::text::{RECENT_GLYPHS_PER_GENERATION, RecentShapes, fonts_changed},
    fast::text_style::TextStyleStack,
    hsla, point, px, size,
};
use refineable::Refineable;

fn refinement(font_size: f32) -> TextStyleRefinement {
    TextStyleRefinement {
        font_size: Some(px(font_size).into()),
        ..Default::default()
    }
}

/// The text style a stack of refinements should resolve to.
fn refined(refinements: &[TextStyleRefinement]) -> TextStyle {
    let mut style = TextStyle::default();
    for refinement in refinements {
        style.refine(refinement);
    }
    style
}

#[test]
fn text_style_stack_resolves_as_refining_does() {
    let bold = TextStyleRefinement {
        font_weight: Some(FontWeight::BOLD),
        color: Some(hsla(0.5, 0.5, 0.5, 1.)),
        ..Default::default()
    };
    let mut stack = TextStyleStack::default();
    assert_eq!(*stack.resolve(), TextStyle::default());

    stack.push(refinement(10.));
    stack.push(bold.clone());
    assert_eq!(*stack.resolve(), refined(&[refinement(10.), bold.clone()]));

    // A style resolved lower in the stack is built on, and one resolved
    // above a pop is forgotten.
    stack.pop();
    assert_eq!(*stack.resolve(), refined(&[refinement(10.)]));
    stack.push(refinement(12.));
    stack.push(bold.clone());
    assert_eq!(
        *stack.resolve(),
        refined(&[refinement(10.), refinement(12.), bold])
    );

    // Asking twice hands out the same style.
    assert!(std::rc::Rc::ptr_eq(&stack.resolve(), &stack.resolve()));

    let mut copy = TextStyleStack::default();
    copy.push(refinement(20.));
    assert_eq!(*copy.resolve(), refined(&[refinement(20.)]));
    copy.clone_from(&stack);
    assert_eq!(*copy.resolve(), *stack.resolve());
    copy.pop();
    assert_eq!(
        *copy.resolve(),
        refined(&[refinement(10.), refinement(12.)])
    );

    stack.clear();
    assert_eq!(*stack.resolve(), TextStyle::default());
}

#[test]
fn text_style_stack_aligns_as_it_resolves() {
    let aligned = |text_align| TextStyleRefinement {
        text_align: Some(text_align),
        ..Default::default()
    };
    let mut stack = TextStyleStack::default();
    assert_eq!(stack.text_align(), TextStyle::default().text_align);

    stack.push(aligned(TextAlign::Right));
    stack.push(refinement(10.));
    assert_eq!(stack.text_align(), TextAlign::Right);
    stack.push(aligned(TextAlign::Center));
    assert_eq!(stack.text_align(), TextAlign::Center);
    assert_eq!(stack.text_align(), stack.resolve().text_align);
    stack.pop();
    assert_eq!(stack.text_align(), TextAlign::Right);
    stack.clear();
    stack.push(refinement(10.));
    assert_eq!(stack.text_align(), TextStyle::default().text_align);
}

fn line(len: usize) -> LineLayout {
    LineLayout {
        font_size: px(14.),
        width: px(len as f32 * 7.),
        ascent: px(10.),
        descent: px(3.),
        runs: vec![ShapedRun {
            font_id: FontId(0),
            glyphs: (0..len)
                .map(|index| ShapedGlyph {
                    id: GlyphId(index as u32),
                    position: point(px(index as f32 * 7.), px(0.)),
                    index,
                    is_emoji: false,
                })
                .collect(),
        }],
        len,
    }
}

fn get(shapes: &mut RecentShapes, text: &str, runs: &[FontRun]) -> Option<LineLayout> {
    let hash = RecentShapes::hash(text, px(14.), runs);
    shapes.get(hash, text, px(14.), runs)
}

fn insert(shapes: &mut RecentShapes, text: &str, runs: &[FontRun]) {
    let hash = RecentShapes::hash(text, px(14.), runs);
    shapes.insert(hash, text, px(14.), runs, line(text.len()));
}

#[test]
fn recent_shapes_answer_only_the_line_they_were_shaped_from() {
    let runs = [FontRun {
        len: 5,
        font_id: FontId(0),
    }];
    let other_font = [FontRun {
        len: 5,
        font_id: FontId(1),
    }];
    let mut shapes = RecentShapes::default();
    // The first lookup takes in the fonts generation.
    assert!(get(&mut shapes, "12.34", &runs).is_none());
    insert(&mut shapes, "12.34", &runs);

    let found = get(&mut shapes, "12.34", &runs).unwrap();
    assert_eq!(found.len, 5);
    assert_eq!(found.width, px(35.));
    assert_eq!(found.runs[0].glyphs.len(), 5);
    assert!(get(&mut shapes, "12.35", &runs).is_none());
    assert!(get(&mut shapes, "12.34", &other_font).is_none());
    let hash = RecentShapes::hash("12.34", px(14.), &runs);
    assert!(shapes.get(hash, "12.34", px(15.), &runs).is_none());

    // Adding fonts can change how a line shapes, so nothing is kept.
    fonts_changed();
    assert!(get(&mut shapes, "12.34", &runs).is_none());
    assert!(get(&mut shapes, "12.34", &runs).is_none());

    // Part of the same test, as `fonts_changed` is felt by every cache.
    recent_shapes_keep_the_lines_used_lately();
}

fn recent_shapes_keep_the_lines_used_lately() {
    let runs = |len| {
        [FontRun {
            len,
            font_id: FontId(0),
        }]
    };
    let mut shapes = RecentShapes::default();
    get(&mut shapes, "", &runs(0));
    insert(&mut shapes, "kept", &runs(4));
    insert(&mut shapes, "dropped", &runs(7));
    // Fill two generations and a half with other lines, using "kept" as it
    // goes.
    let per_line = 8;
    let lines = RECENT_GLYPHS_PER_GENERATION / per_line * 5 / 2;
    for n in 0..lines {
        insert(&mut shapes, &format!("{n:08}"), &runs(per_line));
        if n % 100 == 0 {
            assert!(get(&mut shapes, "kept", &runs(4)).is_some());
        }
    }
    assert!(get(&mut shapes, "kept", &runs(4)).is_some());
    assert!(get(&mut shapes, "dropped", &runs(7)).is_none());
    assert!(get(&mut shapes, &format!("{:08}", lines - 1), &runs(per_line)).is_some());
}

fn glyph(glyph_id: u32, subpixel_x: u8) -> RenderGlyphParams {
    RenderGlyphParams {
        font_id: FontId(3),
        glyph_id: GlyphId(glyph_id),
        font_size: px(14.),
        subpixel_variant: point(subpixel_x, 0),
        scale_factor: 2.,
        is_emoji: false,
        subpixel_rendering: false,
        dilation: 0,
    }
}

fn device_bounds(width: i32) -> Bounds<DevicePixels> {
    Bounds {
        origin: point(DevicePixels(0), DevicePixels(-10)),
        size: size(DevicePixels(width), DevicePixels(12)),
    }
}

#[test]
fn glyph_bounds_cache_answers_only_the_glyph_it_holds() {
    let mut cache = GlyphBoundsCache::default();
    assert_eq!(cache.get(&glyph(7, 0)), None);
    cache.insert(&glyph(7, 0), device_bounds(5));
    assert_eq!(cache.get(&glyph(7, 0)), Some(device_bounds(5)));
    assert_eq!(cache.get(&glyph(7, 1)), None);
    assert_eq!(
        cache.get(&RenderGlyphParams {
            scale_factor: 1.,
            ..glyph(7, 0)
        }),
        None
    );
    assert_eq!(
        cache.get(&RenderGlyphParams {
            dilation: 1,
            ..glyph(7, 0)
        }),
        None
    );

    // Glyphs sharing a slot take it over from each other.
    for glyph_id in 0..10_000 {
        cache.insert(&glyph(glyph_id, 2), device_bounds(glyph_id as i32));
    }
    for glyph_id in 0..10_000 {
        let found = cache.get(&glyph(glyph_id, 2));
        assert!(found.is_none() || found == Some(device_bounds(glyph_id as i32)));
    }
    assert_eq!(cache.get(&glyph(9_999, 2)), Some(device_bounds(9_999)));
}

/// A glyph's tile in the sprite atlas is kept for the frame it was looked up
/// in only, since the atlas may be cleared between frames; its raster bounds
/// and its tile's size are kept on.
#[test]
fn glyph_bounds_cache_keeps_a_tile_for_its_frame() {
    let tile = crate::AtlasTile {
        texture_id: crate::AtlasTextureId {
            index: 1,
            kind: crate::AtlasTextureKind::Monochrome,
        },
        tile_id: crate::TileId(3),
        padding: 0,
        bounds: device_bounds(5),
    };
    let mut cache = GlyphBoundsCache::default();
    cache.insert_tile(&glyph(7, 0), tile);
    assert_eq!(cache.lookup(&glyph(7, 0)), None, "no glyph, no tile");

    cache.insert(&glyph(7, 0), device_bounds(5));
    assert_eq!(
        cache.lookup(&glyph(7, 0)),
        Some((device_bounds(5), None, None))
    );
    cache.insert_tile(&glyph(7, 0), tile);
    cache.insert_tile(&glyph(7, 1), tile);
    assert_eq!(
        cache.lookup(&glyph(7, 0)),
        Some((device_bounds(5), Some(tile), Some(tile.bounds.size)))
    );
    assert_eq!(cache.lookup(&glyph(7, 1)), None);

    cache.finish_frame();
    assert_eq!(
        cache.lookup(&glyph(7, 0)),
        Some((device_bounds(5), None, Some(tile.bounds.size))),
        "the tile's size outlives its frame"
    );
}

/// A line tests its glyphs against its mask pushed out by their reach once a
/// run; that is the box a glyph may draw in, the bounding box's height above
/// and below the baseline and before the origin and its width after it, each
/// plus a 2px margin, overlapping the mask.
#[test]
fn a_glyph_may_reach_the_mask_where_its_box_overlaps_it() {
    let mask = Bounds::new(point(px(40.), px(30.)), size(px(100.), px(50.)));
    let (width, height) = (px(6.), px(12.));
    for x in -40..200 {
        for y in -40..140 {
            let (x, y) = (px(x as f32), px(y as f32));
            let line_glyph = Bounds::new(point(x, y - px(9.)), size(width, height));
            let reach = Bounds::from_corners(
                point(x - height - px(2.), y - height - px(2.)),
                point(x + width + px(2.), y + height + px(2.)),
            );
            assert_eq!(
                may_reach(line_glyph, px(9.), &mask),
                reach.intersects(&mask),
                "a glyph at {x:?}, {y:?}"
            );
        }
    }
}

#[test]
fn a_line_without_backgrounds_has_no_background_to_paint() {
    let run = |background_color| DecorationRun {
        len: 3,
        color: hsla(0., 0., 0., 1.),
        background_color,
        underline: None,
        strikethrough: None,
    };
    assert!(has_no_background(&[]));
    assert!(has_no_background(&[run(None), run(None)]));
    assert!(!has_no_background(&[
        run(None),
        run(Some(hsla(0.1, 0.5, 0.5, 1.)))
    ]));
}
