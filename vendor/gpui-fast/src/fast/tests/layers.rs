// Modified by Longbridge for gpui-fast.
//! Tests of scroll layers.

use crate::AppContext as _;
use crate::{
    AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, ContentMask, DevicePixels, Hsla,
    LayerFrame, LayerKey, MonochromeSprite, Path, Pixels, Point, PolychromeSprite, Quad,
    ScaledPixels, Scene, Shadow, SubpixelSprite, TileCoord, TileId, TransformationMatrix,
    Underline, decode_layer_tile, fast::layers::scene::translate_primitive, layer_tile_id,
    layer_tile_texture_id, point, px, scene::Primitive, size,
};

#[test]
fn layer_tile_ids_round_trip_and_never_collide_with_the_atlas() {
    for layer in [0, 1, 77, 0x00FF_FFFF] {
        for tile in [(0, 0), (-1, 3), (2047, -2048), (-2048, 2047)] {
            let key = LayerKey(layer);
            let coord = TileCoord {
                x: tile.0,
                y: tile.1,
            };
            let texture = layer_tile_texture_id(key);
            assert_eq!(texture.kind, AtlasTextureKind::Polychrome);
            assert!(texture.index >= crate::LAYER_TILE_TEXTURE_BASE);
            assert_eq!(
                decode_layer_tile(texture, layer_tile_id(coord)),
                Some((key, coord))
            );
        }
    }
    let atlas = AtlasTextureId {
        index: 3,
        kind: AtlasTextureKind::Polychrome,
    };
    assert_eq!(
        decode_layer_tile(atlas, layer_tile_id(TileCoord { x: 0, y: 0 })),
        None
    );
    let mono = AtlasTextureId {
        index: crate::LAYER_TILE_TEXTURE_BASE,
        kind: AtlasTextureKind::Monochrome,
    };
    assert_eq!(
        decode_layer_tile(mono, layer_tile_id(TileCoord { x: 0, y: 0 })),
        None
    );
}

fn sp(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
    Bounds {
        origin: point(ScaledPixels(x), ScaledPixels(y)),
        size: size(ScaledPixels(w), ScaledPixels(h)),
    }
}

fn wide_mask() -> ContentMask<ScaledPixels> {
    ContentMask {
        bounds: sp(-10_000., -10_000., 20_000., 20_000.),
    }
}

fn quad(bounds: Bounds<ScaledPixels>) -> Quad {
    Quad {
        bounds,
        content_mask: wide_mask(),
        background: Hsla::red().into(),
        ..Default::default()
    }
}

fn layer(content: Scene) -> LayerFrame {
    LayerFrame {
        key: LayerKey(1),
        generation: 1,
        background: crate::rgba(0xffffffff),
        tile_size: 512,
        content: content.into(),
        dirty_tiles: Vec::new(),
    }
}

#[test]
fn a_tile_scene_holds_the_primitives_over_the_tile_in_its_own_space() {
    let mut content = Scene::default();
    content.insert_primitive(quad(sp(10., 10., 20., 20.))); // tile (0,0) only
    content.insert_primitive(quad(sp(500., 100., 40., 10.))); // tiles (0,0) and (1,0)
    content.insert_primitive(quad(sp(600., 700., 10., 10.))); // tile (1,1) only
    content.insert_primitive(quad(sp(-30., -30., 10., 10.))); // tile (-1,-1) only
    content.finish();
    let layer = layer(content);

    assert_eq!(
        layer.tile_bounds(TileCoord { x: -1, y: 2 }),
        sp(-512., 1024., 512., 512.)
    );

    let t10 = layer.tile_scene(TileCoord { x: 1, y: 0 });
    assert_eq!(t10.quads.len(), 1);
    assert_eq!(t10.quads[0].bounds, sp(500. - 512., 100., 40., 10.));
    assert_eq!(
        t10.quads[0].content_mask.bounds.origin,
        point(ScaledPixels(-10_000. - 512.), ScaledPixels(-10_000.))
    );

    let t00 = layer.tile_scene(TileCoord { x: 0, y: 0 });
    assert_eq!(t00.quads.len(), 2);
    // The two do not overlap, so they may share a draw order; the first
    // painted still sorts first.
    assert!(
        t00.quads[0].order <= t00.quads[1].order,
        "drawing order kept"
    );
    assert_eq!(t00.quads[0].bounds, sp(10., 10., 20., 20.));
    assert_eq!(t00.quads[1].bounds, sp(500., 100., 40., 10.));

    let t11 = layer.tile_scene(TileCoord { x: 1, y: 1 });
    assert_eq!(t11.quads.len(), 1);
    assert_eq!(t11.quads[0].bounds, sp(600. - 512., 700. - 512., 10., 10.));

    let tneg = layer.tile_scene(TileCoord { x: -1, y: -1 });
    assert_eq!(tneg.quads.len(), 1);
    assert_eq!(tneg.quads[0].bounds, sp(-30. + 512., -30. + 512., 10., 10.));

    assert!(layer.tile_scene(TileCoord { x: 3, y: 3 }).quads.is_empty());
}

#[test]
fn a_tile_scene_leaves_out_primitives_masked_off_the_tile() {
    let mut content = Scene::default();
    // Spans tiles (0,0) and (1,0), but its mask keeps it inside tile (0,0).
    content.insert_primitive(Quad {
        content_mask: ContentMask {
            bounds: sp(0., 0., 512., 512.),
        },
        ..quad(sp(500., 100., 40., 10.))
    });
    content.finish();
    let layer = layer(content);
    assert_eq!(layer.tile_scene(TileCoord { x: 0, y: 0 }).quads.len(), 1);
    assert!(layer.tile_scene(TileCoord { x: 1, y: 0 }).quads.is_empty());
}

#[test]
fn a_tile_scene_keeps_layers_sharing_one_draw_order() {
    let mut content = Scene::default();
    content.push_layer(sp(0., 0., 1024., 100.));
    content.insert_primitive(quad(sp(10., 10., 10., 10.)));
    content.insert_primitive(quad(sp(600., 10., 10., 10.)));
    content.pop_layer();
    content.insert_primitive(quad(sp(20., 20., 10., 10.)));
    content.finish();
    let layer = layer(content);

    let t00 = layer.tile_scene(TileCoord { x: 0, y: 0 });
    assert_eq!(t00.quads.len(), 2);
    assert!(t00.quads[0].order < t00.quads[1].order);
    let t10 = layer.tile_scene(TileCoord { x: 1, y: 0 });
    assert_eq!(t10.quads.len(), 1);
    assert_eq!(t10.quads[0].bounds, sp(600. - 512., 10., 10., 10.));
}

/// Content handed to the renderer in parts, as a virtual list's rows are,
/// draws over each tile what it draws handed over as one scene: a part
/// reaching no pixel of a tile is left out of the tile's scene.
#[test]
fn content_in_parts_draws_each_tile_as_one_scene_does() {
    use crate::fast::layers::{
        scene::{LayerContent, LayerPart},
        tiles::part_tile_hashes,
    };
    use crate::scene::PaintOperation;
    use std::rc::Rc;

    let color = |row: usize| crate::hsla(row as f32 / 7., 0.5, 0.5, 1.);
    // Six rows, 200 px tall and 700 px wide: a background each; in every
    // other row a layer around two overlapping quads; in row 3 a shadow
    // blurred into the rows beside it.
    let rows: Vec<Vec<PaintOperation>> = (0..6)
        .map(|row| {
            let y = row as f32 * 200.;
            let mut operations = vec![PaintOperation::Primitive(
                Quad {
                    background: color(row).into(),
                    ..quad(sp(0., y, 700., 200.))
                }
                .into(),
            )];
            if row % 2 == 0 {
                operations.push(PaintOperation::StartLayer(sp(0., y, 700., 200.)));
                operations.push(PaintOperation::Primitive(
                    quad(sp(480., y + 50., 60., 60.)).into(),
                ));
                operations.push(PaintOperation::Primitive(
                    Quad {
                        background: color(row + 1).into(),
                        ..quad(sp(500., y + 70., 60., 60.))
                    }
                    .into(),
                ));
                operations.push(PaintOperation::EndLayer);
            }
            if row == 3 {
                operations.push(PaintOperation::Primitive(
                    Shadow {
                        order: 0,
                        blur_radius: ScaledPixels(20.),
                        bounds: sp(100., y + 150., 300., 40.),
                        corner_radii: Default::default(),
                        content_mask: wide_mask(),
                        color: Hsla::black(),
                        element_bounds: sp(100., y + 150., 300., 40.),
                        element_corner_radii: Default::default(),
                        inset: 0,
                        pad: 0,
                    }
                    .into(),
                ));
            }
            operations
        })
        .collect();

    let mut whole = Scene::default();
    for operation in rows.iter().flatten() {
        match operation {
            PaintOperation::Primitive(primitive) => whole.insert_primitive(primitive.clone()),
            PaintOperation::StartLayer(bounds) => whole.push_layer(*bounds),
            PaintOperation::EndLayer => whole.pop_layer(),
        }
    }
    whole.finish();
    let parts = rows.iter().map(|operations| {
        let (_, reach) = part_tile_hashes(operations, 512);
        let mut scene = Scene::default();
        scene.paint_operations = operations
            .iter()
            .map(|operation| match operation {
                PaintOperation::Primitive(primitive) => {
                    PaintOperation::Primitive(primitive.clone())
                }
                PaintOperation::StartLayer(bounds) => PaintOperation::StartLayer(*bounds),
                PaintOperation::EndLayer => PaintOperation::EndLayer,
            })
            .collect();
        LayerPart {
            bounds: Some(reach.unwrap_or_default()),
            scene: Rc::new(scene),
        }
    });
    let whole = layer(whole);
    let in_parts = LayerFrame {
        content: LayerContent::from_parts(parts),
        ..whole.clone()
    };
    for y in -1..4 {
        for x in -1..3 {
            let tile = TileCoord { x, y };
            let drawn =
                |frame: &LayerFrame| crate::fast::layers::verify::drawn(&frame.tile_scene(tile));
            assert_eq!(drawn(&in_parts), drawn(&whole), "tile {tile:?}");
        }
    }
}

fn atlas_tile() -> AtlasTile {
    AtlasTile {
        texture_id: AtlasTextureId {
            index: 0,
            kind: AtlasTextureKind::Monochrome,
        },
        tile_id: TileId(9),
        padding: 0,
        bounds: Bounds {
            origin: point(DevicePixels(0), DevicePixels(0)),
            size: size(DevicePixels(4), DevicePixels(4)),
        },
    }
}

#[test]
fn translation_moves_every_position_a_primitive_carries() {
    let delta = point(ScaledPixels(7.), ScaledPixels(-3.));
    let mask = ContentMask {
        bounds: sp(0., 0., 100., 100.),
    };
    let moved_mask = sp(7., -3., 100., 100.);
    let translate = |primitive: Primitive| translate_primitive(&primitive, delta);

    let shadow = Shadow {
        order: 0,
        blur_radius: ScaledPixels(2.),
        bounds: sp(1., 2., 3., 4.),
        corner_radii: Default::default(),
        content_mask: mask,
        color: Hsla::red(),
        element_bounds: sp(5., 6., 7., 8.),
        element_corner_radii: Default::default(),
        inset: 0,
        pad: 0,
    };
    let Primitive::Shadow(moved) = translate(shadow.into()) else {
        unreachable!()
    };
    assert_eq!(moved.bounds, sp(8., -1., 3., 4.));
    assert_eq!(moved.element_bounds, sp(12., 3., 7., 8.));
    assert_eq!(moved.content_mask.bounds, moved_mask);
    assert_eq!(moved.blur_radius, ScaledPixels(2.));

    let Primitive::Quad(moved) = translate(
        Quad {
            content_mask: mask,
            ..quad(sp(1., 2., 3., 4.))
        }
        .into(),
    ) else {
        unreachable!()
    };
    assert_eq!(moved.bounds, sp(8., -1., 3., 4.));
    assert_eq!(moved.content_mask.bounds, moved_mask);

    let mut path = Path::new(point(px(10.), px(20.)));
    path.push_triangle(
        (
            point(px(10.), px(20.)),
            point(px(30.), px(20.)),
            point(px(10.), px(40.)),
        ),
        (point(0., 1.), point(0., 1.), point(0., 1.)),
    );
    let mut path = path.scale(1.);
    path.content_mask = mask;
    for vertex in &mut path.vertices {
        vertex.content_mask = mask;
    }
    let bounds = path.bounds;
    let Primitive::Path(moved) = translate(path.into()) else {
        unreachable!()
    };
    assert_eq!(moved.bounds.origin, bounds.origin + delta);
    assert_eq!(moved.bounds.size, bounds.size);
    assert_eq!(moved.content_mask.bounds, moved_mask);
    let positions: Vec<_> = moved.vertices.iter().map(|v| v.xy_position).collect();
    assert_eq!(
        positions,
        vec![
            point(ScaledPixels(17.), ScaledPixels(17.)),
            point(ScaledPixels(37.), ScaledPixels(17.)),
            point(ScaledPixels(17.), ScaledPixels(37.)),
        ]
    );
    assert!(
        moved
            .vertices
            .iter()
            .all(|v| v.content_mask.bounds == moved_mask)
    );
    assert!(
        moved
            .vertices
            .iter()
            .all(|v| v.st_position == point(0., 1.))
    );

    let underline = Underline {
        order: 0,
        pad: 0,
        bounds: sp(1., 2., 3., 4.),
        content_mask: mask,
        color: Hsla::red(),
        thickness: ScaledPixels(1.),
        wavy: true.into(),
    };
    let Primitive::Underline(moved) = translate(underline.into()) else {
        unreachable!()
    };
    assert_eq!(moved.bounds, sp(8., -1., 3., 4.));
    assert_eq!(moved.content_mask.bounds, moved_mask);

    // A transformation applies to window positions, `R·p + t`, so moving the
    // sprite by `delta` moves `t` to `t + (I − R)·delta`.
    let rotation = TransformationMatrix {
        rotation_scale: [[0., -1.], [1., 0.]],
        translation: [5., 6.],
    };
    let expected = TransformationMatrix {
        rotation_scale: [[0., -1.], [1., 0.]],
        // (I − R)·(7, −3) = (7, −3) − (3, 7) = (4, −10)
        translation: [5. + 4., 6. - 10.],
    };
    let mono = MonochromeSprite {
        order: 0,
        pad: 0,
        bounds: sp(1., 2., 3., 4.),
        content_mask: mask,
        color: Hsla::red(),
        tile: atlas_tile(),
        transformation: rotation,
    };
    let Primitive::MonochromeSprite(moved) = translate(mono.into()) else {
        unreachable!()
    };
    assert_eq!(moved.bounds, sp(8., -1., 3., 4.));
    assert_eq!(moved.content_mask.bounds, moved_mask);
    assert_eq!(moved.transformation, expected);
    assert_eq!(moved.tile, atlas_tile());
    let window_position = |m: &TransformationMatrix, p: Point<Pixels>| m.apply(p);
    assert_eq!(
        window_position(&moved.transformation, point(px(8.), px(-1.))),
        window_position(&rotation, point(px(1.), px(2.))) + point(px(7.), px(-3.)),
    );

    let unit = MonochromeSprite {
        transformation: TransformationMatrix::unit(),
        ..mono
    };
    let Primitive::MonochromeSprite(moved) = translate(unit.into()) else {
        unreachable!()
    };
    assert_eq!(moved.transformation, TransformationMatrix::unit());

    let subpixel = SubpixelSprite {
        order: 0,
        pad: 0,
        bounds: sp(1., 2., 3., 4.),
        content_mask: mask,
        color: Hsla::red(),
        tile: atlas_tile(),
        transformation: rotation,
    };
    let Primitive::SubpixelSprite(moved) = translate(subpixel.into()) else {
        unreachable!()
    };
    assert_eq!(moved.bounds, sp(8., -1., 3., 4.));
    assert_eq!(moved.content_mask.bounds, moved_mask);
    assert_eq!(moved.transformation, expected);

    let poly = PolychromeSprite {
        order: 0,
        pad: 0,
        grayscale: false.into(),
        opacity: 1.,
        bounds: sp(1., 2., 3., 4.),
        content_mask: mask,
        corner_radii: Default::default(),
        tile: atlas_tile(),
    };
    let Primitive::PolychromeSprite(moved) = translate(poly.into()) else {
        unreachable!()
    };
    assert_eq!(moved.bounds, sp(8., -1., 3., 4.));
    assert_eq!(moved.content_mask.bounds, moved_mask);
    assert_eq!(moved.tile, atlas_tile());
}

#[test]
fn a_tile_scene_keeps_overlapping_primitives_in_drawing_order() {
    let mut content = Scene::default();
    content.insert_primitive(quad(sp(10., 10., 20., 20.)));
    content.insert_primitive(quad(sp(20., 20., 20., 20.)));
    content.finish();
    let layer = layer(content);
    let t00 = layer.tile_scene(TileCoord { x: 0, y: 0 });
    assert_eq!(t00.quads.len(), 2);
    assert!(t00.quads[0].order < t00.quads[1].order);
    assert_eq!(t00.quads[1].bounds, sp(20., 20., 20., 20.));
}

/// Upstream's glyph quantization, as `Window::paint_glyph` had it.
fn old_quantize(x: f32, y: f32) -> (f32, f32, u8) {
    use crate::{SUBPIXEL_VARIANTS_X as VX, SUBPIXEL_VARIANTS_Y as VY};
    let qx = crate::util::round_half_toward_zero(x * VX as f32) / VX as f32;
    let qy = crate::util::round_half_toward_zero(y * VY as f32) / VY as f32;
    (qx.trunc(), qy.trunc(), (qx.fract() * VX as f32) as u8)
}

#[test]
fn glyph_quantization_is_unchanged_on_screen() {
    for i in 0..20_000 {
        let x = i as f32 * 0.0137;
        let y = i as f32 * 0.0291;
        let (origin, variant) =
            crate::fast::glyphs::quantize_origin(point(ScaledPixels(x), ScaledPixels(y)));
        let (ox, oy, v) = old_quantize(x, y);
        assert_eq!(
            (origin.x.0, origin.y.0, variant.x, variant.y),
            (ox, oy, v, 0),
            "at ({x}, {y})"
        );
        let emoji =
            crate::fast::glyphs::quantize_emoji_origin(point(ScaledPixels(x), ScaledPixels(y)));
        assert_eq!(
            (emoji.x.0, emoji.y.0),
            (
                crate::util::round_half_toward_zero(x),
                crate::util::round_half_toward_zero(y)
            ),
            "emoji at ({x}, {y})"
        );
    }
}

#[test]
fn glyph_quantization_moves_with_whole_pixel_shifts() {
    for i in 0..5_000 {
        let x = -300. + i as f32 * 0.0731;
        let y = -300. + i as f32 * 0.0519;
        let (a, va) = crate::fast::glyphs::quantize_origin(point(ScaledPixels(x), ScaledPixels(y)));
        let (b, vb) = crate::fast::glyphs::quantize_origin(point(
            ScaledPixels(x + 1024.),
            ScaledPixels(y + 1024.),
        ));
        assert_eq!(va, vb, "variant at ({x}, {y})");
        assert_eq!(
            (b.x.0 - a.x.0, b.y.0 - a.y.0),
            (1024., 1024.),
            "origin at ({x}, {y})"
        );
        let ea =
            crate::fast::glyphs::quantize_emoji_origin(point(ScaledPixels(x), ScaledPixels(y)));
        let eb = crate::fast::glyphs::quantize_emoji_origin(point(
            ScaledPixels(x + 1024.),
            ScaledPixels(y + 1024.),
        ));
        assert_eq!(
            (eb.x.0 - ea.x.0, eb.y.0 - ea.y.0),
            (1024., 1024.),
            "emoji at ({x}, {y})"
        );
    }
}

struct EmptyView;

impl crate::Render for EmptyView {
    fn render(
        &mut self,
        _window: &mut crate::Window,
        _cx: &mut crate::Context<Self>,
    ) -> impl crate::IntoElement {
        crate::Empty
    }
}

#[crate::test]
fn scroll_layers_are_on_where_compiled_and_can_be_turned_off(cx: &mut crate::TestAppContext) {
    let window = cx.add_window(|_, _| EmptyView);
    cx.update_window(window.into(), |_, window, cx| {
        assert_eq!(
            window.fast_layers.enabled,
            crate::fast::layers::COMPILED
                && std::env::var("GPUI_SCROLL_LAYERS").map_or(true, |value| value != "0")
        );
        assert_eq!(
            crate::fast::layers::active(window, cx),
            window.fast_layers.enabled
        );
        window.set_scroll_layers(false);
        assert!(!window.fast_layers.enabled);
        assert!(window.fast_layers.layers.is_empty());
        assert!(
            window.refreshing,
            "the switch redraws the window from scratch"
        );
        window.draw(cx).clear(cx);
        assert!(!crate::fast::layers::active(window, cx));

        window.set_scroll_layers(true);
        assert!(
            !crate::fast::layers::active(window, cx),
            "not while the window is refreshing"
        );
        window.draw(cx).clear(cx);
        assert_eq!(
            crate::fast::layers::active(window, cx),
            crate::fast::layers::COMPILED
        );

        window.set_view_retention(false);
        window.draw(cx).clear(cx);
        assert!(!crate::fast::layers::active(window, cx));
    })
    .unwrap();
}

#[test]
fn layout_stats_count_scroll_layer_work() {
    let stats = crate::LayoutStats::default();
    assert_eq!(
        (
            stats.layer_frames_composited,
            stats.layer_frames_repainted,
            stats.tiles_dirtied,
            stats.layer_rebuilds_for_input,
            stats.layers_demoted,
        ),
        (0, 0, 0, 0, 0)
    );
}

/// The paint stream (M3): painting a scroll container's content into a
/// layer, tile diffing, snapping, background baking and compositing.
mod paint {
    use crate::AppContext as _;
    use crate::fast::layers::tiles::{dirty_tiles, tile_hashes};
    use crate::{Bounds, ContentMask, Hsla, Quad, ScaledPixels, Scene, TileCoord, point, size};

    fn sp(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
        Bounds {
            origin: point(ScaledPixels(x), ScaledPixels(y)),
            size: size(ScaledPixels(w), ScaledPixels(h)),
        }
    }

    fn quad(bounds: Bounds<ScaledPixels>, color: Hsla) -> Quad {
        Quad {
            bounds,
            content_mask: ContentMask {
                bounds: sp(-10_000., -10_000., 20_000., 20_000.),
            },
            background: color.into(),
            ..Default::default()
        }
    }

    fn scene(quads: &[Quad]) -> Scene {
        let mut scene = Scene::default();
        for quad in quads {
            scene.insert_primitive(*quad);
        }
        scene.finish();
        scene
    }

    fn region() -> Bounds<ScaledPixels> {
        sp(0., 0., 1024., 1024.)
    }

    fn dirty(old: &Scene, new: &Scene) -> Vec<TileCoord> {
        dirty_tiles(
            &tile_hashes(old, 512, region()),
            &tile_hashes(new, 512, region()),
        )
    }

    fn tiles(coords: &[(i32, i32)]) -> Vec<TileCoord> {
        coords.iter().map(|&(x, y)| TileCoord { x, y }).collect()
    }

    #[test]
    fn every_tile_of_the_region_gets_a_hash_even_an_empty_one() {
        let hashes = tile_hashes(
            &scene(&[quad(sp(10., 10., 5., 5.), Hsla::red())]),
            512,
            region(),
        );
        let mut keys: Vec<_> = hashes.keys().copied().collect();
        keys.sort();
        assert_eq!(keys, tiles(&[(0, 0), (0, 1), (1, 0), (1, 1)]));
        assert_eq!(
            hashes[&TileCoord { x: 1, y: 0 }],
            hashes[&TileCoord { x: 1, y: 1 }]
        );
        assert_ne!(
            hashes[&TileCoord { x: 0, y: 0 }],
            hashes[&TileCoord { x: 1, y: 1 }]
        );
    }

    #[test]
    fn identical_scenes_dirty_no_tile() {
        let quads = [
            quad(sp(10., 10., 20., 20.), Hsla::red()),
            quad(sp(500., 600., 40., 10.), Hsla::blue()),
        ];
        assert_eq!(dirty(&scene(&quads), &scene(&quads)), Vec::new());
    }

    #[test]
    fn a_colour_change_dirties_the_tiles_the_quad_covers() {
        let before = [
            quad(sp(10., 10., 20., 20.), Hsla::red()),
            quad(sp(500., 600., 40., 10.), Hsla::blue()),
        ];
        let mut after = before;
        after[1].background = Hsla::green().into();
        assert_eq!(
            dirty(&scene(&before), &scene(&after)),
            tiles(&[(0, 1), (1, 1)])
        );
    }

    #[test]
    fn a_move_within_one_tile_dirties_that_tile() {
        let before = [
            quad(sp(10., 10., 20., 20.), Hsla::red()),
            quad(sp(600., 600., 10., 10.), Hsla::blue()),
        ];
        let mut after = before;
        after[1].bounds = sp(640., 610., 10., 10.);
        assert_eq!(dirty(&scene(&before), &scene(&after)), tiles(&[(1, 1)]));
    }

    #[test]
    fn an_added_primitive_spanning_two_tiles_dirties_both() {
        let before = [quad(sp(10., 10., 20., 20.), Hsla::red())];
        let after = [
            quad(sp(10., 10., 20., 20.), Hsla::red()),
            quad(sp(100., 500., 10., 40.), Hsla::blue()),
        ];
        assert_eq!(
            dirty(&scene(&before), &scene(&after)),
            tiles(&[(0, 0), (0, 1)])
        );
    }

    #[test]
    fn tiles_new_to_the_region_are_dirty() {
        let quads = [quad(sp(10., 10., 20., 20.), Hsla::red())];
        let old = tile_hashes(&scene(&quads), 512, sp(0., 0., 512., 512.));
        let new = tile_hashes(&scene(&quads), 512, sp(0., 0., 512., 1024.));
        assert_eq!(dirty_tiles(&old, &new), tiles(&[(0, 1)]));
    }

    #[test]
    fn a_primitive_moved_by_whole_tiles_hashes_like_its_old_tile() {
        // Its mask moves with it, so it draws the same pixels in its tile.
        let at = |bounds: Bounds<ScaledPixels>| Quad {
            content_mask: ContentMask { bounds },
            ..quad(bounds, Hsla::red())
        };
        let a = tile_hashes(&scene(&[at(sp(10., 10., 20., 20.))]), 512, region());
        let b = tile_hashes(&scene(&[at(sp(522., 522., 20., 20.))]), 512, region());
        assert_eq!(a[&TileCoord { x: 0, y: 0 }], b[&TileCoord { x: 1, y: 1 }]);
    }

    #[test]
    fn a_mask_change_outside_a_tile_leaves_the_tile_clean() {
        // The painted region, every primitive's mask while a layer paints,
        // moves with the scroll offset: only the tiles it crosses change.
        let masked = |mask: Bounds<ScaledPixels>| {
            [
                Quad {
                    content_mask: ContentMask { bounds: mask },
                    ..quad(sp(10., 10., 20., 900.), Hsla::red())
                },
                Quad {
                    content_mask: ContentMask { bounds: mask },
                    ..quad(sp(600., 10., 20., 900.), Hsla::blue())
                },
            ]
        };
        let before = masked(sp(0., -100., 1024., 1000.));
        let after = masked(sp(0., -90., 1024., 1000.));
        assert_eq!(
            dirty(&scene(&before), &scene(&after)),
            tiles(&[(0, 1), (1, 1)])
        );
    }

    fn row_color(row: usize) -> Hsla {
        crate::hsla(row as f32 / 64., 0.5, 0.5, 1.)
    }

    /// A panel, white at first, holding a 100 px scroll container of `rows` rows of
    /// 20 px, each its own colour.
    struct Rows {
        rows: usize,
        panel: Hsla,
        scroll: crate::ScrollHandle,
        height: crate::Pixels,
    }

    impl crate::Render for Rows {
        fn render(
            &mut self,
            _window: &mut crate::Window,
            _cx: &mut crate::Context<Self>,
        ) -> impl crate::IntoElement {
            use crate::{
                InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
                Styled as _,
            };
            crate::div().size_full().bg(self.panel).child(
                crate::div()
                    .id("s")
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .h(self.height)
                    .children(
                        (0..self.rows).map(|i| crate::div().h(crate::px(20.)).bg(row_color(i))),
                    ),
            )
        }
    }

    fn rows_window(cx: &mut crate::TestAppContext, rows: usize) -> crate::WindowHandle<Rows> {
        rows_window_of_height(cx, rows, crate::px(100.))
    }

    fn rows_window_of_height(
        cx: &mut crate::TestAppContext,
        rows: usize,
        height: crate::Pixels,
    ) -> crate::WindowHandle<Rows> {
        let window = cx.add_window(|_, _| Rows {
            rows,
            panel: crate::white(),
            scroll: crate::ScrollHandle::new(),
            height,
        });
        // The first frame redraws everything, which never uses layers.
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        window
    }

    /// Draws `window` with its view rebuilt and every scroll container made
    /// to decide `decision`.
    fn draw_deciding(
        cx: &mut crate::TestAppContext,
        window: crate::WindowHandle<Rows>,
        decision: crate::fast::layers::policy::Decision,
    ) {
        window
            .update(cx, |_, window, cx| {
                window.fast_layers.forced_decision = Some(decision);
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
    }

    fn with_record<R>(
        cx: &mut crate::TestAppContext,
        window: crate::WindowHandle<Rows>,
        f: impl FnOnce(&crate::fast::layers::record::LayerRecord, &crate::Window) -> R,
    ) -> R {
        cx.update_window(window.into(), |_, window, _| {
            assert_eq!(window.fast_layers.layers.len(), 1, "one layer");
            let layer = window.fast_layers.layers.values().next().unwrap();
            f(
                layer.record.as_ref().expect("the layer was painted"),
                window,
            )
        })
        .unwrap()
    }

    fn row_quads(scene: &Scene, rows: usize) -> Vec<(usize, Bounds<ScaledPixels>)> {
        scene
            .quads
            .iter()
            .filter_map(|quad| {
                (0..rows)
                    .find(|&row| quad.background == row_color(row).into())
                    .map(|row| (row, quad.bounds))
            })
            .collect()
    }

    #[crate::test]
    fn a_repainted_layer_holds_the_content_in_content_space(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = rows_window(cx, 40);
        draw_deciding(cx, window, crate::fast::layers::policy::Decision::Repaint);

        let main_rows = cx
            .update_window(window.into(), |_, window, _| {
                row_quads(&window.rendered_frame.scene, 40)
            })
            .unwrap();
        assert_eq!(main_rows, Vec::new(), "the rows are painted into the layer");

        let scale = cx
            .update_window(window.into(), |_, window, _| window.scale_factor())
            .unwrap();
        with_record(cx, window, |record, _| {
            // The viewport, 100 px, and two viewports of overscan below it;
            // nothing above, at the top.
            assert_eq!(record.viewport.size.height, crate::px(100.));
            assert_eq!(record.painted_region.origin.y, crate::px(0.));
            assert_eq!(record.painted_region.size.height, crate::px(300.));
            let rows = row_quads(record.content.scene().unwrap(), 40);
            assert_eq!(
                rows.iter().map(|(row, _)| *row).collect::<Vec<_>>(),
                (0..15).collect::<Vec<_>>()
            );
            for (row, bounds) in rows {
                assert_eq!(bounds.origin.y, ScaledPixels(row as f32 * 20. * scale));
                assert_eq!(bounds.size.height, ScaledPixels(20. * scale));
            }
            assert_eq!(record.generation, 1);
            let mut tiles: Vec<_> = record.tile_hashes.keys().copied().collect();
            tiles.sort();
            // 300 px at the test window's scale of 2 is 600 device px: two
            // tiles high.
            assert!(tiles.iter().all(|tile| tile.y <= 1), "{tiles:?}");
            assert!(tiles.iter().any(|tile| tile.y == 1), "{tiles:?}");
            assert_eq!(
                record.dirty_tiles, tiles,
                "a first paint dirties every tile"
            );
        });
    }

    #[crate::test]
    fn scrolled_content_is_stored_where_it_was_before_the_scroll(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = rows_window(cx, 40);
        window
            .update(cx, |view, _, _| {
                view.scroll
                    .set_offset(crate::point(crate::px(0.), crate::px(-300.)))
            })
            .unwrap();
        draw_deciding(cx, window, crate::fast::layers::policy::Decision::Repaint);
        let scale = cx
            .update_window(window.into(), |_, window, _| window.scale_factor())
            .unwrap();
        with_record(cx, window, |record, _| {
            // Overscan of two viewports above and below the one at 300..400.
            assert_eq!(record.painted_region.origin.y, crate::px(-200.));
            assert_eq!(record.painted_region.size.height, crate::px(500.));
            let rows = row_quads(record.content.scene().unwrap(), 40);
            assert_eq!(
                rows.iter().map(|(row, _)| *row).collect::<Vec<_>>(),
                (5..30).collect::<Vec<_>>()
            );
            for (row, bounds) in rows {
                assert_eq!(bounds.origin.y, ScaledPixels(row as f32 * 20. * scale));
            }
        });
    }

    /// A renderer keeps a layer's tiles by key and generation. A record
    /// dropped (content that could not be composited, a demotion, a
    /// background that could not be baked) and painted again must not come
    /// back at a generation the renderer holds tiles of, or those stale
    /// tiles would be shown.
    #[crate::test]
    fn a_layer_painted_again_after_its_record_was_dropped_gets_a_new_generation(
        cx: &mut crate::TestAppContext,
    ) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        use crate::fast::layers::policy::Decision;
        let window = rows_window(cx, 40);
        draw_deciding(cx, window, Decision::Repaint);
        let first = with_record(cx, window, |record, _| record.generation);
        cx.update_window(window.into(), |_, window, _| {
            for layer in window.fast_layers.layers.values_mut() {
                layer.record = None;
            }
        })
        .unwrap();
        draw_deciding(cx, window, Decision::Repaint);
        let (second, dirty, tiles) = with_record(cx, window, |record, _| {
            (
                record.generation,
                record.dirty_tiles.len(),
                record.tile_hashes.len(),
            )
        });
        assert!(second > first, "generation {second} after {first}");
        assert_eq!(dirty, tiles, "every tile is dirty");
        let framed = cx
            .update_window(window.into(), |_, window, _| {
                window
                    .rendered_frame
                    .scene
                    .layers
                    .frames
                    .iter()
                    .map(|frame| frame.generation)
                    .collect::<Vec<_>>()
            })
            .unwrap();
        assert_eq!(
            framed,
            vec![second],
            "the frame hands the renderer the new generation"
        );
    }

    #[crate::test]
    fn a_repaint_at_a_shifted_offset_dirties_only_the_edge_tiles(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        use crate::fast::layers::policy::Decision;
        // Enough viewport for the painted region to span many tiles.
        let window = rows_window_of_height(cx, 400, crate::px(1000.));
        let scroll_to = |cx: &mut crate::TestAppContext, y: f32| {
            window
                .update(cx, |view, _, _| {
                    view.scroll
                        .set_offset(crate::point(crate::px(0.), crate::px(y)))
                })
                .unwrap();
            draw_deciding(cx, window, Decision::Repaint);
        };
        scroll_to(cx, -3000.);
        scroll_to(cx, -3010.);
        with_record(cx, window, |record, _| {
            let rows: Vec<i32> = record.tile_hashes.keys().map(|tile| tile.y).collect();
            let (top, bottom) = (*rows.iter().min().unwrap(), *rows.iter().max().unwrap());
            assert!(bottom - top >= 4, "the region spans many tile rows");
            assert!(!record.dirty_tiles.is_empty(), "the region's edges moved");
            for tile in &record.dirty_tiles {
                assert!(
                    tile.y == top || tile.y == bottom,
                    "tile {tile:?} inside the region ({top}..={bottom}) is dirty"
                );
            }
        });
        // The same content at the same offset dirties nothing.
        scroll_to(cx, -3010.);
        with_record(cx, window, |record, _| {
            assert_eq!(record.dirty_tiles, Vec::new());
        });
    }

    #[crate::test]
    fn overscan_is_clamped_to_content(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        // 60 px of rows in the 100 px viewport: nothing to scroll to.
        let window = rows_window(cx, 3);
        draw_deciding(cx, window, crate::fast::layers::policy::Decision::Repaint);
        with_record(cx, window, |record, window| {
            assert_eq!(
                record.painted_region, record.viewport,
                "no overscan past the content"
            );
            assert_eq!(row_quads(record.content.scene().unwrap(), 3).len(), 3);
            let region = record.painted_region.scale(window.scale_factor());
            for tile in record.tile_hashes.keys() {
                let top = tile.y as f32 * 512.;
                assert!(
                    top < region.bottom_right().y.0,
                    "tile {tile:?} below the content"
                );
            }
        });
    }

    /// A scroll container whose content records the element offset it is
    /// prepainted at.
    struct Offsets {
        scroll: crate::ScrollHandle,
        seen: std::rc::Rc<std::cell::Cell<crate::Point<crate::Pixels>>>,
    }

    impl crate::Render for Offsets {
        fn render(
            &mut self,
            _window: &mut crate::Window,
            _cx: &mut crate::Context<Self>,
        ) -> impl crate::IntoElement {
            use crate::{
                InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
                Styled as _,
            };
            let seen = self.seen.clone();
            crate::div().size_full().child(
                crate::div()
                    .id("s")
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .h(crate::px(100.))
                    .child(
                        crate::canvas(
                            move |_, window, _| seen.set(window.element_offset()),
                            |_, _, _, _| {},
                        )
                        .h(crate::px(1000.))
                        .w_full(),
                    ),
            )
        }
    }

    #[crate::test]
    fn scroll_offsets_land_on_device_pixels(cx: &mut crate::TestAppContext) {
        let seen = std::rc::Rc::new(std::cell::Cell::new(crate::Point::default()));
        let window = cx.add_window({
            let seen = seen.clone();
            |_, _| Offsets {
                scroll: crate::ScrollHandle::new(),
                seen,
            }
        });
        cx.test_window(window.into())
            .simulate_scale_factor_change(1.25);
        window
            .update(cx, |view, _, cx| {
                view.scroll
                    .set_offset(crate::point(crate::px(0.), crate::px(-10.37)));
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let offset = seen.get();
        if crate::fast::layers::COMPILED {
            // -10.37 px is -12.9625 device px; the content goes to -13.
            assert_eq!(offset.y * 1.25, crate::px(-13.));
        } else {
            assert_eq!(offset.y, crate::px(-10.37));
        }

        let snap = crate::fast::layers::paint::snap_offset;
        let fractional = crate::point(crate::px(0.37), crate::px(-10.37));
        assert_eq!(snap(fractional, 1.25, false), fractional);
        let snapped = snap(fractional, 1.25, true);
        assert_eq!(
            (snapped.x * 1.25, snapped.y * 1.25),
            (crate::px(0.), crate::px(-13.))
        );
        let whole = crate::point(crate::px(-8.), crate::px(-12.8));
        assert_eq!(snap(whole, 1.25, true), whole, "whole device pixels stay");
    }

    fn bake(quads: &[Quad], window_opaque: bool) -> Option<crate::Rgba> {
        crate::fast::layers::background::bake(
            &scene(quads),
            sp(100., 100., 200., 300.),
            window_opaque,
        )
    }

    #[test]
    fn an_opaque_panel_under_the_viewport_is_baked() {
        let panel = quad(sp(0., 0., 1000., 1000.), Hsla::blue());
        assert_eq!(bake(&[panel], true), Some(Hsla::blue().into()));
        // Something painted later away from the viewport changes nothing.
        let elsewhere = quad(sp(500., 500., 10., 10.), Hsla::red());
        assert_eq!(bake(&[panel, elsewhere], true), Some(Hsla::blue().into()));
        // The topmost panel is the one baked.
        let above = quad(sp(50., 50., 400., 400.), Hsla::green());
        assert_eq!(bake(&[panel, above], true), Some(Hsla::green().into()));
    }

    #[test]
    fn only_a_solid_opaque_panel_covering_the_viewport_is_baked() {
        let panel = quad(sp(0., 0., 1000., 1000.), Hsla::blue());
        assert_eq!(bake(&[panel], false), None, "transparent window");

        let gradient = Quad {
            background: crate::linear_gradient(
                0.,
                crate::linear_color_stop(Hsla::red(), 0.),
                crate::linear_color_stop(Hsla::blue(), 1.),
            ),
            ..panel
        };
        assert_eq!(bake(&[gradient], true), None, "gradient");

        let translucent = quad(sp(0., 0., 1000., 1000.), Hsla::blue().opacity(0.5));
        assert_eq!(bake(&[translucent], true), None, "translucent");

        let partly_inside = quad(sp(250., 250., 100., 100.), Hsla::red());
        assert_eq!(bake(&[panel, partly_inside], true), None, "partly covered");

        let short = quad(sp(0., 0., 1000., 350.), Hsla::blue());
        assert_eq!(bake(&[short], true), None, "does not cover the viewport");

        let clipped = Quad {
            content_mask: ContentMask {
                bounds: sp(0., 0., 1000., 200.),
            },
            ..panel
        };
        assert_eq!(
            bake(&[clipped], true),
            None,
            "clipped short of the viewport"
        );

        let rounded_inside = Quad {
            bounds: sp(90., 90., 500., 500.),
            corner_radii: crate::Corners::all(ScaledPixels(20.)),
            ..panel
        };
        assert_eq!(bake(&[rounded_inside], true), None, "corner inside");
        let rounded_outside = Quad {
            bounds: sp(0., 0., 1000., 1000.),
            corner_radii: crate::Corners::all(ScaledPixels(20.)),
            ..panel
        };
        assert_eq!(
            bake(&[rounded_outside], true),
            Some(Hsla::blue().into()),
            "corners away from the viewport"
        );

        let bordered = Quad {
            bounds: sp(95., 0., 1000., 1000.),
            border_widths: crate::Edges::all(ScaledPixels(10.)),
            border_color: Hsla::red(),
            ..panel
        };
        assert_eq!(bake(&[bordered], true), None, "border inside");
        let transparent_border = Quad {
            border_color: Hsla::transparent_black(),
            ..bordered
        };
        assert_eq!(
            bake(&[transparent_border], true),
            Some(Hsla::blue().into()),
            "an invisible border"
        );
    }

    /// Draws `window` over a panel of colour `panel`, every scroll container
    /// made to decide `decision`.
    fn draw_over(
        cx: &mut crate::TestAppContext,
        window: crate::WindowHandle<Rows>,
        panel: Hsla,
        decision: crate::fast::layers::policy::Decision,
    ) {
        window
            .update(cx, |view, window, cx| {
                view.panel = panel;
                window.fast_layers.forced_decision = Some(decision);
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
    }

    #[crate::test]
    fn a_layer_bakes_the_panel_under_it(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = rows_window(cx, 40);
        draw_deciding(cx, window, crate::fast::layers::policy::Decision::Repaint);
        with_record(cx, window, |record, _| {
            assert_eq!(record.background, crate::white().into());
        });
    }

    #[crate::test]
    fn background_change_repaints_layer(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        use crate::fast::layers::policy::Decision;
        let window = rows_window(cx, 40);
        draw_deciding(cx, window, Decision::Repaint);
        let (generation, tiles) = with_record(cx, window, |record, _| {
            let mut tiles: Vec<_> = record.tile_hashes.keys().copied().collect();
            tiles.sort();
            (record.generation, tiles)
        });

        // Painted again with nothing changed but the panel: every tile is
        // cleared with another colour, so every tile is dirty.
        draw_over(cx, window, Hsla::blue(), Decision::Repaint);
        let generation = with_record(cx, window, |record, _| {
            assert_eq!(record.background, Hsla::blue().into());
            assert_eq!(record.generation, generation + 1);
            assert_eq!(record.dirty_tiles, tiles);
            record.generation
        });

        // Composited over another panel: the content stands, the tiles are
        // cleared with the new colour.
        draw_over(cx, window, Hsla::green(), Decision::Composite);
        with_record(cx, window, |record, _| {
            assert_eq!(record.background, Hsla::green().into());
            assert_eq!(record.generation, generation + 1);
            assert_eq!(record.dirty_tiles, tiles);
        });

        // Nothing changed: the same tiles, nothing dirty.
        draw_deciding(cx, window, Decision::Repaint);
        with_record(cx, window, |record, _| {
            assert_eq!(record.background, Hsla::green().into());
            assert_eq!(record.dirty_tiles, Vec::new());
        });
    }

    #[crate::test]
    fn no_layer_is_painted_over_a_background_it_cannot_bake(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = rows_window(cx, 40);
        draw_over(
            cx,
            window,
            Hsla::blue().opacity(0.5),
            crate::fast::layers::policy::Decision::Repaint,
        );
        cx.update_window(window.into(), |_, window, _| {
            assert_eq!(
                row_quads(&window.rendered_frame.scene, 40).len(),
                5,
                "the rows are painted as without layers"
            );
            assert!(
                window
                    .fast_layers
                    .layers
                    .values()
                    .all(|layer| layer.record.is_none())
            );
        })
        .unwrap();
    }

    /// The layer tiles `scene` composites, with the layer each belongs to.
    fn tile_quads(scene: &Scene) -> Vec<(crate::LayerKey, TileCoord, crate::PolychromeSprite)> {
        scene
            .polychrome_sprites
            .iter()
            .filter_map(|sprite| {
                crate::decode_layer_tile(sprite.tile.texture_id, sprite.tile.tile_id)
                    .map(|(key, coord)| (key, coord, *sprite))
            })
            .collect()
    }

    #[crate::test]
    fn tile_quads_cover_the_viewport_at_the_current_offset(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        use crate::fast::layers::policy::Decision;
        let window = rows_window(cx, 40);
        draw_deciding(cx, window, Decision::Repaint);
        window
            .update(cx, |view, _, _| {
                view.scroll
                    .set_offset(crate::point(crate::px(0.), crate::px(-130.)))
            })
            .unwrap();
        draw_deciding(cx, window, Decision::Composite);

        cx.update_window(window.into(), |_, window, _| {
            let scale = window.scale_factor();
            let layer = window.fast_layers.layers.values().next().expect("a layer");
            let record = layer.record.as_ref().expect("painted");
            let scene = &window.rendered_frame.scene;
            assert_eq!(row_quads(scene, 40), Vec::new(), "the rows are not drawn");

            // The viewport, snapped as every content mask is.
            let viewport = record.viewport;
            let floor = |v: crate::Pixels| crate::util::floor_to_device_pixel(v.0, scale);
            let ceil = |v: crate::Pixels| crate::util::ceil_to_device_pixel(v.0, scale);
            let mask = Bounds::from_corners(
                point(
                    ScaledPixels(floor(viewport.left())),
                    ScaledPixels(floor(viewport.top())),
                ),
                point(
                    ScaledPixels(ceil(viewport.right())),
                    ScaledPixels(ceil(viewport.bottom())),
                ),
            );
            let translation = point(ScaledPixels(0.), ScaledPixels(-130. * scale));

            assert_eq!(scene.layers.frames.len(), 1);
            let frame = &scene.layers.frames[0];
            assert_eq!(frame.key, layer.key);
            assert!(frame.content.ptr_eq(&record.content));
            assert_eq!(frame.generation, record.generation);
            assert_eq!(frame.background, record.background);
            assert_eq!(frame.tile_size, 512);
            assert_eq!(frame.dirty_tiles, record.dirty_tiles);

            let quads = tile_quads(scene);
            let mut expected = Vec::new();
            let content_viewport = Bounds {
                origin: mask.origin - translation,
                size: mask.size,
            };
            for y in -4..4 {
                for x in -4..8 {
                    let tile = TileCoord { x, y };
                    if frame.tile_bounds(tile).intersects(&content_viewport) {
                        expected.push(tile);
                    }
                }
            }
            let mut coords: Vec<_> = quads.iter().map(|(_, coord, _)| *coord).collect();
            coords.sort();
            assert!(!expected.is_empty());
            assert_eq!(coords, expected);
            let order = quads[0].2.order;
            for (key, coord, sprite) in &quads {
                assert_eq!(*key, layer.key);
                let tile = frame.tile_bounds(*coord);
                assert_eq!(
                    sprite.bounds,
                    Bounds {
                        origin: tile.origin + translation,
                        size: tile.size,
                    }
                );
                assert_eq!(sprite.content_mask.bounds, mask);
                assert_eq!(sprite.order, order, "the tiles share one draw order");
                assert_eq!(sprite.opacity, 1.);
                assert_eq!(sprite.tile.bounds.size.width.0, 512);
                assert_eq!(sprite.tile.bounds.size.height.0, 512);
                assert_eq!(sprite.tile.bounds.origin, crate::Point::default());
            }
        })
        .unwrap();
    }

    #[crate::test]
    fn a_repainted_layer_is_composited_where_it_was_painted(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = rows_window(cx, 40);
        cx.update_window(window.into(), |_, window, _| window.reset_layout_stats())
            .unwrap();
        draw_deciding(cx, window, crate::fast::layers::policy::Decision::Repaint);
        cx.update_window(window.into(), |_, window, _| {
            let scene = &window.rendered_frame.scene;
            let quads = tile_quads(scene);
            assert!(!quads.is_empty());
            for (_, coord, sprite) in &quads {
                let tile = scene.layers.frames[0].tile_bounds(*coord);
                assert_eq!(sprite.bounds, tile, "no translation at offset 0");
            }
            let stats = window.layout_stats();
            assert_eq!(stats.layer_frames_repainted, 1);
            assert_eq!(stats.layer_frames_composited, 1);
            assert_eq!(
                stats.tiles_dirtied,
                scene.layers.frames[0].dirty_tiles.len() as u64
            );
        })
        .unwrap();
    }

    #[crate::test]
    fn a_reused_view_keeps_compositing_its_layer(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = rows_window(cx, 40);
        draw_deciding(cx, window, crate::fast::layers::policy::Decision::Repaint);
        let before = cx
            .update_window(window.into(), |_, window, _| {
                tile_quads(&window.rendered_frame.scene).len()
            })
            .unwrap();
        // Nothing changed: the view is reused, its scene replayed.
        cx.update_window(window.into(), |_, window, cx| {
            window.reset_layout_stats();
            window.draw(cx).clear(cx)
        })
        .unwrap();
        cx.update_window(window.into(), |_, window, _| {
            let scene = &window.rendered_frame.scene;
            assert_eq!(tile_quads(scene).len(), before);
            assert_eq!(scene.layers.frames.len(), 1, "the layer frame comes along");
            let stats = window.layout_stats();
            assert_eq!(stats.layer_frames_repainted, 0, "the view was reused");
            assert!(stats.views_reused > 0, "the view was reused");
        })
        .unwrap();
    }

    #[crate::test]
    fn a_composited_layer_over_a_background_it_cannot_bake_is_drawn_into_the_frame(
        cx: &mut crate::TestAppContext,
    ) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        use crate::fast::layers::policy::Decision;
        let window = rows_window(cx, 40);
        let direct = cx
            .update_window(window.into(), |_, window, _| {
                row_quads(&window.rendered_frame.scene, 40)
            })
            .unwrap();
        draw_deciding(cx, window, Decision::Repaint);
        draw_over(cx, window, Hsla::blue().opacity(0.5), Decision::Composite);
        cx.update_window(window.into(), |_, window, _| {
            let scene = &window.rendered_frame.scene;
            assert!(tile_quads(scene).is_empty());
            assert!(scene.layers.frames.is_empty());
            assert_eq!(row_quads(scene, 40), direct, "the rows as without a layer");
            let layer = window.fast_layers.layers.values().next().expect("a layer");
            assert!(layer.record.is_none(), "painted afresh next time");
        })
        .unwrap();
    }

    #[crate::test]
    fn a_layer_painted_while_scrolled_is_drawn_where_it_shows_over_a_background_it_cannot_bake(
        cx: &mut crate::TestAppContext,
    ) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        use crate::fast::layers::policy::Decision;
        let window = rows_window(cx, 40);
        let scroll_to = |cx: &mut crate::TestAppContext, y: f32| {
            window
                .update(cx, |view, _, _| {
                    view.scroll.set_offset(point(crate::px(0.), crate::px(y)));
                })
                .unwrap();
        };
        scroll_to(cx, -60.);
        draw_deciding(cx, window, Decision::Bypass);
        let direct = cx
            .update_window(window.into(), |_, window, _| {
                row_quads(&window.rendered_frame.scene, 40)
            })
            .unwrap();

        scroll_to(cx, -40.);
        draw_deciding(cx, window, Decision::Repaint);
        scroll_to(cx, -60.);
        draw_over(cx, window, Hsla::blue().opacity(0.5), Decision::Composite);
        cx.update_window(window.into(), |_, window, _| {
            let scene = &window.rendered_frame.scene;
            assert!(tile_quads(scene).is_empty());
            let drawn: Vec<_> = row_quads(scene, 40)
                .into_iter()
                .filter(|(row, _)| direct.iter().any(|(direct, _)| direct == row))
                .collect();
            assert_eq!(drawn, direct, "the rows where they show at the new offset");
        })
        .unwrap();
    }

    /// A scroll container whose content paints a path under a row.
    struct PathRows;

    impl crate::Render for PathRows {
        fn render(
            &mut self,
            _window: &mut crate::Window,
            _cx: &mut crate::Context<Self>,
        ) -> impl crate::IntoElement {
            use crate::{
                InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
                Styled as _,
            };
            crate::div().size_full().bg(crate::white()).child(
                crate::div()
                    .id("s")
                    .overflow_y_scroll()
                    .h(crate::px(100.))
                    .child(
                        crate::canvas(
                            |_, _, _| {},
                            |bounds, _, window, _| {
                                let origin = bounds.origin;
                                let mut path = crate::Path::new(origin);
                                path.line_to(origin + crate::point(crate::px(30.), crate::px(0.)));
                                path.line_to(origin + crate::point(crate::px(0.), crate::px(30.)));
                                window.paint_path(path, crate::black());
                            },
                        )
                        .h(crate::px(40.))
                        .w_full(),
                    )
                    .children((0..20).map(|i| crate::div().h(crate::px(20.)).bg(row_color(i)))),
            )
        }
    }

    /// Paths are never composited from tiles (spec §5.6), but a path nothing
    /// in the content draws over is drawn into the frame over the layer's
    /// tiles, where it lands on the same pixels as drawn without a layer.
    #[crate::test]
    fn paths_nothing_draws_over_are_drawn_over_the_tiles(cx: &mut crate::TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = cx.add_window(|_, _| PathRows);
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let (direct_paths, direct) = cx
            .update_window(window.into(), |_, window, _| {
                let scene = &window.rendered_frame.scene;
                (scene.paths.len(), crate::fast::layers::verify::drawn(scene))
            })
            .unwrap();
        assert_eq!(direct_paths, 1);

        window
            .update(cx, |_, window, cx| {
                window.fast_layers.forced_decision =
                    Some(crate::fast::layers::policy::Decision::Repaint);
                window.reset_layout_stats();
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        cx.update_window(window.into(), |_, window, _| {
            let stats = window.layout_stats();
            assert_eq!(stats.layer_frames_repainted, 1);
            assert_eq!(stats.layer_frames_composited, 1);
            let layer = window.fast_layers.layers.values().next().expect("a layer");
            let record = layer.record.as_ref().expect("painted");
            assert!(!record.has_paths);
            assert!(
                record.content.scene().unwrap().paths.is_empty(),
                "the tiles hold no path"
            );
            assert_eq!(record.paths.len(), 1, "the layer keeps the path apart");
            let scene = &window.rendered_frame.scene;
            assert!(!tile_quads(scene).is_empty(), "the tiles are composited");
            assert_eq!(scene.layers.frames.len(), 1);
            assert_eq!(scene.paths.len(), 1, "the path is in the frame");
            assert!(
                scene.paths[0].order > tile_quads(scene)[0].2.order,
                "the path is drawn over the tiles"
            );
            assert!(row_quads(scene, 20).is_empty(), "the rows are in the tiles");
            assert_eq!(
                crate::fast::layers::verify::drawn(scene),
                direct,
                "the frame draws what it drew without a layer"
            );
        })
        .unwrap();
    }
}

/// Primitives that draw outside their bounds: a shadow's blur and a
/// transformed sprite reach the tiles they draw over, not just the tiles
/// their bounds cover.
mod footprints {
    use super::{atlas_tile, layer, sp, wide_mask};
    use crate::fast::layers::tiles::{dirty_tiles, tile_hashes};
    use crate::{
        Hsla, MonochromeSprite, Radians, Scene, Shadow, TileCoord, TransformationMatrix, point,
    };

    fn shadow(color: Hsla) -> Shadow {
        // Its blur reaches x = 500 + 3 * 10 = 530, inside tile (1, 0).
        Shadow {
            order: 0,
            blur_radius: crate::ScaledPixels(10.),
            bounds: sp(400., 100., 100., 100.),
            corner_radii: Default::default(),
            content_mask: wide_mask(),
            color,
            element_bounds: sp(400., 100., 100., 100.),
            element_corner_radii: Default::default(),
            inset: 0,
            pad: 0,
        }
    }

    fn rotated_sprite(color: Hsla) -> MonochromeSprite {
        // A 70 px square ending 2 px short of tile (1, 0), turned an eighth
        // around its centre (475, 135): its corners reach x = 524.5.
        MonochromeSprite {
            order: 0,
            pad: 0,
            bounds: sp(440., 100., 70., 70.),
            content_mask: wide_mask(),
            color,
            tile: atlas_tile(),
            transformation: TransformationMatrix::unit()
                .translate(point(crate::ScaledPixels(475.), crate::ScaledPixels(135.)))
                .rotate(Radians(std::f32::consts::FRAC_PI_4))
                .translate(point(
                    crate::ScaledPixels(-475.),
                    crate::ScaledPixels(-135.),
                )),
        }
    }

    fn scene_of(primitive: impl Into<crate::scene::Primitive>) -> Scene {
        let mut scene = Scene::default();
        scene.insert_primitive(primitive);
        scene.finish();
        scene
    }

    fn dirty(old: &Scene, new: &Scene) -> Vec<TileCoord> {
        let region = sp(0., 0., 1024., 512.);
        dirty_tiles(
            &tile_hashes(old, 512, region),
            &tile_hashes(new, 512, region),
        )
    }

    const BOTH: [TileCoord; 2] = [TileCoord { x: 0, y: 0 }, TileCoord { x: 1, y: 0 }];

    #[test]
    fn a_shadow_change_dirties_every_tile_its_blur_reaches() {
        assert_eq!(
            dirty(
                &scene_of(shadow(Hsla::red())),
                &scene_of(shadow(Hsla::blue()))
            ),
            BOTH
        );
    }

    #[test]
    fn a_tile_scene_holds_a_shadow_whose_blur_reaches_the_tile() {
        let frame = layer(scene_of(shadow(Hsla::red())));
        assert_eq!(frame.tile_scene(TileCoord { x: 1, y: 0 }).shadows.len(), 1);
        assert!(
            frame
                .tile_scene(TileCoord { x: 1, y: 1 })
                .shadows
                .is_empty()
        );
    }

    #[test]
    fn a_shadow_masked_off_a_tile_leaves_it_alone() {
        let masked = |color| Shadow {
            content_mask: crate::ContentMask {
                bounds: sp(0., 0., 512., 512.),
            },
            ..shadow(color)
        };
        assert_eq!(
            dirty(
                &scene_of(masked(Hsla::red())),
                &scene_of(masked(Hsla::blue()))
            ),
            [TileCoord { x: 0, y: 0 }]
        );
    }

    #[test]
    fn a_rotated_sprite_change_dirties_every_tile_it_turns_into() {
        assert_eq!(
            dirty(
                &scene_of(rotated_sprite(Hsla::red())),
                &scene_of(rotated_sprite(Hsla::blue()))
            ),
            BOTH
        );
        let frame = layer(scene_of(rotated_sprite(Hsla::red())));
        assert_eq!(
            frame
                .tile_scene(TileCoord { x: 1, y: 0 })
                .monochrome_sprites
                .len(),
            1
        );
    }
}

/// Tests of telling scrolls apart from other changes and of deciding what a
/// scroll container's layer does each frame (M4).
mod invalidation {
    use crate::fast::layers::invalidate::{ScrollSource, render_read_offset, scrolled};
    use crate::{
        AnyWindowHandle, App, AppContext as _, Context, Entity, GlobalElementId,
        InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollDelta,
        ScrollHandle, ScrollWheelEvent, StatefulInteractiveElement as _, Styled as _,
        TestAppContext, TouchPhase, Window, div, point, px, rgb,
    };
    use std::{cell::Cell, rc::Rc};

    /// A page scrolled by a wheel: a 100 px tall scroll container of forty
    /// 20 px rows at the top left of the window.
    struct Page {
        handle: ScrollHandle,
        read_offset_in_render: bool,
        reader: Option<Entity<Reader>>,
    }

    impl Render for Page {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            if self.read_offset_in_render {
                let _ = self.handle.offset();
            }
            div()
                .size_full()
                .child(
                    div()
                        .id("scroller")
                        .overflow_y_scroll()
                        .track_scroll(&self.handle)
                        .w(px(200.))
                        .h(px(100.))
                        .children(
                            (0..40).map(|row| div().h(px(20.)).bg(rgb(0x100000 + row * 0x10))),
                        ),
                )
                .children(self.reader.clone())
        }
    }

    /// A view outside the scroll container that shows where it is scrolled.
    struct Reader {
        handle: ScrollHandle,
        renders: Rc<Cell<usize>>,
    }

    impl Render for Reader {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let offset = self.handle.offset();
            div().w(px(10.)).h(px(10.) - offset.y / 100.)
        }
    }

    fn page(handle: ScrollHandle, read_offset_in_render: bool) -> Page {
        Page {
            handle,
            read_offset_in_render,
            reader: None,
        }
    }

    pub(super) fn with_window<R>(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> R {
        cx.update_window(window, |_, window, cx| f(window, cx))
            .unwrap()
    }

    pub(super) fn draw(cx: &mut TestAppContext, window: AnyWindowHandle) {
        with_window(cx, window, |window, cx| window.draw(cx).clear(cx));
    }

    /// Scrolls the page by `dy` with the wheel, returning the scroll
    /// containers noted as scrolled before the frame that follows is drawn.
    pub(super) fn wheel(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        dy: f32,
    ) -> Vec<GlobalElementId> {
        with_window(cx, window, |window, cx| {
            window.dispatch_event(
                crate::PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position: point(px(20.), px(20.)),
                    delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                    modifiers: Default::default(),
                    touch_phase: TouchPhase::Moved,
                }),
                cx,
            );
            window
                .fast_layers
                .scrolls
                .scrolled
                .iter()
                .cloned()
                .collect()
        })
    }

    /// The page's scroll container, as last painted.
    fn scroller(cx: &mut TestAppContext, window: AnyWindowHandle) -> GlobalElementId {
        with_window(cx, window, |window, _| {
            window
                .fast_layers
                .scrolls
                .containers()
                .find(|id| id.last() == Some(&"scroller".into()))
                .cloned()
                .expect("the scroll container was painted")
        })
    }

    fn is_scrolled(cx: &mut TestAppContext, window: AnyWindowHandle, id: &GlobalElementId) -> bool {
        with_window(cx, window, |window, _| scrolled(window, id))
    }

    #[crate::test]
    fn a_wheel_scroll_is_noted_for_its_container(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window: AnyWindowHandle = cx
            .add_window(|_, _| page(ScrollHandle::new(), false))
            .into();
        draw(cx, window);
        let scroller = scroller(cx, window);
        assert!(!is_scrolled(cx, window, &scroller));

        assert_eq!(wheel(cx, window, -30.), vec![scroller.clone()]);
        draw(cx, window);
        assert!(
            !is_scrolled(cx, window, &scroller),
            "a frame takes in the scrolls before it"
        );
    }

    #[crate::test]
    fn programmatic_scrolls_are_noted(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = ScrollHandle::new();
        let window: AnyWindowHandle = cx
            .add_window({
                let handle = handle.clone();
                move |_, _| page(handle, false)
            })
            .into();
        draw(cx, window);
        let scroller = scroller(cx, window);
        assert!(!is_scrolled(cx, window, &scroller));
        handle.set_offset(point(px(0.), px(-40.)));
        assert!(is_scrolled(cx, window, &scroller));
        draw(cx, window);
        assert!(!is_scrolled(cx, window, &scroller));
        handle.scroll_to_item(30);
        assert!(is_scrolled(cx, window, &scroller));
        draw(cx, window);
        assert!(!is_scrolled(cx, window, &scroller));
        handle.scroll_to_bottom();
        assert!(is_scrolled(cx, window, &scroller));
    }

    /// Whether the root view's own reading, last frame, included the offset
    /// of its scroll container.
    fn root_view_read_offset(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
        let scroller = scroller(cx, window);
        with_window(cx, window, |window, _| {
            let source = window
                .fast_layers
                .scrolls
                .source(&scroller)
                .expect("the scroll container was painted");
            assert!(matches!(source, ScrollSource::Handle(_)));
            let record = window
                .rendered_frame
                .retained
                .records
                .first()
                .expect("the root view is retained");
            render_read_offset(&record.own_dependencies, &source)
        })
    }

    #[crate::test]
    fn a_render_that_reads_the_offset_depends_on_it(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window: AnyWindowHandle = cx.add_window(|_, _| page(ScrollHandle::new(), true)).into();
        draw(cx, window);
        assert!(root_view_read_offset(cx, window));
    }

    #[crate::test]
    fn one_that_does_not_does_not(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window: AnyWindowHandle = cx
            .add_window(|_, _| page(ScrollHandle::new(), false))
            .into();
        draw(cx, window);
        assert!(!root_view_read_offset(cx, window));
    }

    #[crate::test]
    fn a_view_that_read_the_offset_is_built_again_when_it_scrolls(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let renders = Rc::new(Cell::new(0));
        let window: AnyWindowHandle = cx
            .add_window({
                let renders = renders.clone();
                move |_, cx| {
                    let handle = ScrollHandle::new();
                    let reader = cx.new(|_| Reader {
                        handle: handle.clone(),
                        renders,
                    });
                    Page {
                        handle,
                        read_offset_in_render: false,
                        reader: Some(reader),
                    }
                }
            })
            .into();
        draw(cx, window);
        draw(cx, window);
        let before = renders.get();
        wheel(cx, window, -30.);
        draw(cx, window);
        assert_eq!(renders.get(), before + 1, "the reader shows the new offset");
        draw(cx, window);
        assert_eq!(renders.get(), before + 1, "and is reused once it has");
    }

    /// A list whose view shows whether it is scrolled to its end.
    struct EndIndicator {
        state: crate::ListState,
    }

    impl Render for EndIndicator {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let at_end = self.state.is_scrolled_to_end() == Some(true);
            div()
                .size_full()
                .child(
                    crate::list(self.state.clone(), |index, _, _| {
                        div()
                            .h(px(20.))
                            .bg(rgb(0x100000 + index as u32))
                            .into_any_element()
                    })
                    .w(px(200.))
                    .h(px(100.)),
                )
                .child(div().w(px(10.)).h(px(if at_end { 20. } else { 10. })))
        }
    }

    #[crate::test]
    fn a_render_that_asks_whether_a_list_is_scrolled_to_its_end_depends_on_its_offset(
        cx: &mut TestAppContext,
    ) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let state = crate::ListState::new(40, crate::ListAlignment::Top, px(100.));
        let window: AnyWindowHandle = cx
            .add_window({
                let state = state.clone();
                move |_, _| EndIndicator { state }
            })
            .into();
        draw(cx, window);
        draw(cx, window);
        let read = with_window(cx, window, |window, _| {
            let record = window
                .rendered_frame
                .retained
                .records
                .first()
                .expect("the root view is retained");
            let source = ScrollSource::of_state(&state.0.borrow().version);
            render_read_offset(&record.own_dependencies, &source)
        });
        assert!(read);
    }
}

/// Tests of what a scroll container decides to do with its layer each frame
/// (M4), through the paint stream's hook around a container's children,
/// which asks [`decide`] and paints the rows into the layer when told to.
mod decisions {
    use super::invalidation::{draw, with_window};
    use crate::fast::layers::policy::{Decision, last_decision};
    use crate::{
        AnyElement, AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity,
        GlobalElementId, Hsla, InteractiveElement as _, IntoElement, MouseMoveEvent,
        ParentElement as _, Pixels, Render, ScrollDelta, ScrollHandle, ScrollWheelEvent,
        StatefulInteractiveElement as _, Styled as _, TestAppContext, TouchPhase, Window,
        WindowHandle, canvas, div, point, px, rgb, size,
    };
    use std::{cell::Cell, rc::Rc};

    pub(super) const ROWS: usize = 40;
    pub(super) const ROW_HEIGHT: f32 = 20.;

    pub(super) fn viewport() -> Bounds<Pixels> {
        Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(200.), px(100.)),
        }
    }

    /// A row, with a hover style when `hover` is set, `width` wide.
    fn row(index: usize, hover: bool, width: f32) -> AnyElement {
        let row = div()
            .w(px(width))
            .h(px(ROW_HEIGHT))
            .bg(rgb(0x100000 + index as u32 * 0x10));
        if hover {
            row.id(("row", index))
                .hover(|style| style.bg(rgb(0x00ff00)))
                .into_any_element()
        } else {
            row.into_any_element()
        }
    }

    /// The rows in a view of their own, for a page whose scroll container
    /// holds a child view (pattern A).
    pub(super) struct Rows {
        pub(super) tint: u32,
    }

    impl Render for Rows {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().children((0..ROWS).map(|index| {
                div()
                    .h(px(ROW_HEIGHT))
                    .bg(rgb(self.tint + index as u32 * 0x10))
            }))
        }
    }

    /// A page with a 100 px tall scroll container of forty 20 px rows at the
    /// top left of the window: in the page's own view (pattern B), or in a
    /// child view (pattern A), after `extra`, if any. The rows are as wide
    /// as the container, or twice as wide when `wide` is set, which the
    /// container, scrolling on y only, clips.
    pub(super) struct LayerPage {
        pub(super) handle: ScrollHandle,
        pub(super) rows: Option<Entity<Rows>>,
        pub(super) hover: bool,
        pub(super) read_offset_in_render: bool,
        pub(super) extra: Option<Rc<dyn Fn() -> AnyElement>>,
        pub(super) wide: bool,
        /// Whether its render asks for an animation frame.
        pub(super) animate: bool,
        /// Whether it draws a scrollbar beside the scroll container, whose
        /// prepaint and paint read the offset, as GPUI Kit's does.
        pub(super) scrollbar: bool,
        /// Whether the page's background is translucent, which no layer can
        /// bake into its tiles.
        pub(super) translucent: bool,
    }

    impl Render for LayerPage {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            if self.read_offset_in_render {
                let _ = self.handle.offset();
            }
            if self.animate {
                window.request_animation_frame();
            }
            let content_width = viewport().size.width.0 * if self.wide { 2. } else { 1. };
            let mut scroller = div()
                .id("scroller")
                .overflow_y_scroll()
                .track_scroll(&self.handle)
                .w(viewport().size.width)
                .h(viewport().size.height);
            if let Some(extra) = &self.extra {
                scroller = scroller.child(extra());
            }
            scroller = match &self.rows {
                Some(rows) => scroller.child(rows.clone()),
                None => {
                    scroller.children((0..ROWS).map(|index| row(index, self.hover, content_width)))
                }
            };
            let background = if self.translucent {
                crate::rgba(0xffffff80)
            } else {
                rgb(0xffffff)
            };
            let page = div().size_full().bg(background).child(scroller);
            if !self.scrollbar {
                return page;
            }
            let prepaint_handle = self.handle.clone();
            let paint_handle = self.handle.clone();
            page.child(
                canvas(
                    move |_, _, _| prepaint_handle.offset().y,
                    move |bounds, _, window, _| {
                        let offset = paint_handle.offset().y;
                        let thumb = crate::Bounds::new(
                            point(bounds.origin.x, bounds.origin.y - offset / 8.),
                            size(px(6.), px(20.)),
                        );
                        window.paint_quad(crate::fill(thumb, rgb(0x888888)));
                    },
                )
                .absolute()
                .left(viewport().size.width)
                .top(px(0.))
                .w(px(6.))
                .h(viewport().size.height),
            )
        }
    }

    pub(super) fn new_page(child_view: bool, cx: &mut App) -> LayerPage {
        LayerPage {
            handle: ScrollHandle::new(),
            rows: child_view.then(|| cx.new(|_| Rows { tint: 0x100000 })),
            hover: false,
            read_offset_in_render: false,
            extra: None,
            wide: false,
            animate: false,
            scrollbar: false,
            translucent: false,
        }
    }

    pub(super) fn page(cx: &mut TestAppContext, child_view: bool) -> WindowHandle<LayerPage> {
        let window = cx.add_window(move |_, cx| new_page(child_view, cx));
        draw(cx, window.into());
        draw(cx, window.into());
        window
    }

    pub(super) fn scroller_id(cx: &mut TestAppContext, window: AnyWindowHandle) -> GlobalElementId {
        with_window(cx, window, |window, _| {
            window
                .fast_layers
                .scrolls
                .containers()
                .find(|id| id.last() == Some(&"scroller".into()))
                .cloned()
                .expect("the scroll container was painted")
        })
    }

    /// Draws the frame that follows `change`, unless the change drew one.
    pub(super) fn frame_after(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        change: impl FnOnce(&mut TestAppContext),
    ) {
        let frame = with_window(cx, window, |window, _| window.fast_layers.frame);
        change(cx);
        if with_window(cx, window, |window, _| window.fast_layers.frame) == frame {
            draw(cx, window);
        }
    }

    /// Scrolls by `dy` with the wheel, returning what the scroll container
    /// decided in the frame that followed.
    pub(super) fn scroll(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        dy: f32,
    ) -> Option<Decision> {
        scroll_and(cx, window, dy, |_| {})
    }

    /// Scrolls by `dy` with the wheel after `change`, in one frame,
    /// returning what the scroll container decided in it.
    pub(super) fn scroll_and(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        dy: f32,
        change: impl FnOnce(&mut App),
    ) -> Option<Decision> {
        // In one update, for the change and the scroll to be taken in by the
        // same frame.
        frame_after(cx, window, |cx| {
            with_window(cx, window, |window, cx| {
                change(cx);
                window.dispatch_event(
                    crate::PlatformInput::ScrollWheel(ScrollWheelEvent {
                        position: point(px(20.), px(20.)),
                        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                        modifiers: Default::default(),
                        touch_phase: TouchPhase::Moved,
                    }),
                    cx,
                );
            })
        });
        decision(cx, window)
    }

    /// What the scroll container decided in the last frame drawn, if it
    /// decided anything in it.
    pub(super) fn decision(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<Decision> {
        let id = scroller_id(cx, window);
        with_window(cx, window, |window, _| last_decision(window, &id))
    }

    /// Scrolls until the container has a layer, checking it is promoted on
    /// the second scrolled frame.
    pub(super) fn promote(cx: &mut TestAppContext, window: AnyWindowHandle) {
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Repaint));
    }

    /// A background no tile can be cleared with keeps the container on
    /// today's path for a while, rather than painting a layer every
    /// scrolled frame that it can never composite.
    #[crate::test]
    fn a_background_that_cannot_be_baked_stops_repainting(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        handle
            .update(cx, |page, _, cx| {
                page.translucent = true;
                cx.notify();
            })
            .unwrap();
        draw(cx, window);
        promote(cx, window);
        for _ in 0..20 {
            assert_eq!(scroll(cx, window, -5.), Some(Decision::Bypass));
        }
    }

    #[crate::test]
    fn a_wheel_scroll_composites(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = page(cx, false).into();
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert_eq!(scroll(cx, window, 20.), Some(Decision::Composite));
    }

    /// A scrollbar being dragged sets the offset itself and notifies the view
    /// holding the container, as a wheel listener would: that notification
    /// is the scroll's, and the layer composites.
    #[crate::test]
    fn a_scrollbar_drag_composites(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        promote(cx, window);
        for step in 0..6 {
            frame_after(cx, window, |cx| {
                handle
                    .update(cx, |page, _, cx| {
                        let offset = page.handle.offset();
                        page.handle.set_offset(point(offset.x, offset.y - px(10.)));
                        cx.notify();
                    })
                    .unwrap();
            });
            assert_eq!(
                decision(cx, window),
                Some(Decision::Composite),
                "step {step}"
            );
        }
    }

    #[crate::test]
    fn a_child_view_page_composites(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = page(cx, true).into();
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
    }

    #[crate::test]
    fn a_notified_content_view_repaints(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, true);
        let window = handle.into();
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        let rows = handle
            .read_with(cx, |page, _| page.rows.clone().unwrap())
            .unwrap();
        // Notified in the frame a scroll draws, so that the page is built
        // again for the scroll and the rows' view is found changed.
        // Notified without being updated first: the view may have changed
        // what it renders in a way nothing it read shows.
        assert_eq!(
            scroll_and(cx, window, -20., |cx| cx.notify(rows.entity_id())),
            Some(Decision::Repaint)
        );
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert_eq!(
            scroll_and(cx, window, -20., |cx| rows.update(cx, |rows, cx| {
                rows.tint = 0x200000;
                cx.notify();
            })),
            Some(Decision::Repaint)
        );
        // Repeated broad repaints now fall back rather than keeping the
        // expensive overscan alive.
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
    }

    #[crate::test]
    fn an_owner_notified_for_another_reason_while_scrolling_repaints(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        // What the owner renders changes through state nothing records a
        // read of, which only its notification tells.
        let height = Rc::new(Cell::new(10.));
        handle
            .update(cx, |page, _, cx| {
                let height = height.clone();
                page.extra = Some(Rc::new(move || {
                    div().h(px(height.get())).into_any_element()
                }));
                cx.notify();
            })
            .unwrap();
        draw(cx, window);
        let owner = handle.update(cx, |_, _, cx| cx.entity_id()).unwrap();
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        // Notified before the scroll.
        assert_eq!(
            scroll_and(cx, window, -20., |cx| {
                height.set(30.);
                cx.notify(owner);
            }),
            Some(Decision::Repaint)
        );
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        // Notified after it.
        frame_after(cx, window, |cx| {
            with_window(cx, window, |window, cx| {
                window.dispatch_event(
                    crate::PlatformInput::ScrollWheel(ScrollWheelEvent {
                        position: point(px(20.), px(20.)),
                        delta: ScrollDelta::Pixels(point(px(0.), px(-20.))),
                        modifiers: Default::default(),
                        touch_phase: TouchPhase::Moved,
                    }),
                    cx,
                );
                height.set(10.);
                cx.notify(owner);
            })
        });
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
        // Repeated broad repaints now fall back rather than keeping the
        // expensive overscan alive.
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
    }

    #[crate::test]
    fn content_wider_than_a_container_scrolling_on_y_composites(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = cx.add_window(|_, cx| LayerPage {
            wide: true,
            ..new_page(false, cx)
        });
        let window = handle.into();
        draw(cx, window);
        draw(cx, window);
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
    }

    /// A scrollbar beside the container reads the offset while it is
    /// prepainted and painted, outside the content: the content is
    /// composited, and the scrollbar drawn afresh around it.
    #[crate::test]
    fn an_owner_that_reads_the_offset_outside_the_content_composites(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        handle
            .update(cx, |page, _, cx| {
                page.scrollbar = true;
                cx.notify();
            })
            .unwrap();
        draw(cx, window);
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
    }

    #[crate::test]
    fn an_owner_that_reads_the_offset_in_render_repaints(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        handle
            .update(cx, |page, _, cx| {
                page.read_offset_in_render = true;
                cx.notify();
            })
            .unwrap();
        draw(cx, window);
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Repaint));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Repaint));
    }

    #[crate::test]
    fn a_hover_change_in_the_content_repaints(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        handle
            .update(cx, |page, _, cx| {
                page.hover = true;
                cx.notify();
            })
            .unwrap();
        draw(cx, window);
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        frame_after(cx, window, |cx| {
            with_window(cx, window, |window, cx| {
                window.dispatch_event(
                    crate::PlatformInput::MouseMove(MouseMoveEvent {
                        position: point(px(20.), px(70.)),
                        pressed_button: None,
                        modifiers: Default::default(),
                    }),
                    cx,
                );
            })
        });
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
    }

    /// A view around the page that sets the text colour the page inherits.
    struct Themed {
        color: Hsla,
        page: Entity<LayerPage>,
    }

    impl Render for Themed {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .text_color(self.color)
                .child(self.page.clone())
        }
    }

    #[crate::test]
    fn a_style_change_of_the_scroll_div_repaints(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        // The scroll div's own style is set by the view that holds it, whose
        // notification repaints the layer anyway; what it inherits changes
        // without it, as the text colour of a view around it does.
        let handle = cx.add_window(|_, cx| Themed {
            color: crate::black(),
            page: cx.new(|cx| new_page(false, cx)),
        });
        let window: AnyWindowHandle = handle.into();
        draw(cx, window);
        draw(cx, window);
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        frame_after(cx, window, |cx| {
            handle
                .update(cx, |themed, _, cx| {
                    themed.color = crate::white();
                    cx.notify();
                })
                .unwrap();
        });
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
    }

    #[crate::test]
    fn exposing_past_the_margin_repaints(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = page(cx, false).into();
        promote(cx, window);
        // Painted at the offset it was promoted at, -40 px, two viewports
        // (200 px) beyond each edge as far as the content goes. The margin is
        // a quarter of a viewport: the ninth 20 px scroll leaves less than
        // 25 px painted below the viewport.
        for _ in 0..8 {
            assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        }
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Repaint));
        let id = scroller_id(cx, window);
        let painted_at = with_window(cx, window, |window, _| {
            let record = window.fast_layers.layers[&id].record.as_ref().unwrap();
            (record.scroll_offset, record.painted_region)
        });
        assert_eq!(
            painted_at,
            (
                point(px(0.), px(-220.)),
                Bounds::from_corners(point(px(0.), px(-200.)), point(px(200.), px(300.)))
            ),
            "re-centred on the viewport"
        );
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
    }

    /// Puts a 200 px square container scrolling on x, holding an 800 px wide
    /// child, at the top of the page's scroll container, painted by the
    /// page's own view.
    fn with_inner_scroller(
        cx: &mut TestAppContext,
        handle: WindowHandle<LayerPage>,
    ) -> ScrollHandle {
        let inner = ScrollHandle::new();
        let tracked = inner.clone();
        handle
            .update(cx, |page, _, cx| {
                page.extra = Some(Rc::new(move || {
                    div()
                        .id("inner")
                        .overflow_x_scroll()
                        .track_scroll(&tracked)
                        .w(px(200.))
                        .h(px(200.))
                        .child(div().w(px(800.)).h(px(200.)).bg(rgb(0x123456)))
                        .into_any_element()
                }));
                cx.notify();
            })
            .unwrap();
        draw(cx, handle.into());
        inner
    }

    #[crate::test]
    fn a_wheel_scroll_of_a_container_inside_the_content_repaints(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        // The inner container is painted by the view holding the outer one,
        // which the inner one's wheel listener notifies: a change of the
        // outer layer's content, not a scroll of it (spec §6.6).
        let handle = page(cx, false);
        let window = handle.into();
        let inner = with_inner_scroller(cx, handle);
        promote(cx, window);
        let before = inner.offset();
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Repaint));
        assert_ne!(
            inner.offset(),
            before,
            "the wheel scrolled the inner container"
        );
    }

    #[crate::test]
    fn a_programmatic_scroll_of_a_container_inside_the_content_repaints(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        let inner = with_inner_scroller(cx, handle);
        promote(cx, window);
        assert_eq!(
            scroll_and(cx, window, -20., |_| inner
                .set_offset(point(px(-100.), px(0.)))),
            Some(Decision::Repaint)
        );
    }
}

/// Tests of which scroll containers get a layer and for how long (M4).
mod policies {
    use super::decisions::{
        LayerPage, decision, frame_after, page, promote, scroll, scroll_and, scroller_id,
    };
    use super::invalidation::{draw, with_window};
    use crate::fast::layers::policy::Decision;
    use crate::{
        AnyElement, AnyWindowHandle, GlobalElementId, InteractiveElement as _, IntoElement,
        ParentElement as _, Path, Styled as _, TestAppContext, WindowHandle, anchored, canvas,
        deferred, div, point, px, size,
    };
    use std::rc::Rc;

    /// Puts `extra` at the top of the page's scroll container.
    fn with_extra(
        cx: &mut TestAppContext,
        handle: WindowHandle<LayerPage>,
        extra: impl Fn() -> AnyElement + 'static,
    ) {
        handle
            .update(cx, |page, _, cx| {
                page.extra = Some(Rc::new(extra));
                cx.notify();
            })
            .unwrap();
        draw(cx, handle.into());
    }

    fn has_layer(cx: &mut TestAppContext, window: AnyWindowHandle, id: &GlobalElementId) -> bool {
        with_window(cx, window, |window, _| {
            window.fast_layers.layers.contains_key(id)
        })
    }

    fn has_record(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
        let id = scroller_id(cx, window);
        with_window(cx, window, |window, _| {
            window
                .fast_layers
                .layers
                .get(&id)
                .is_some_and(|layer| layer.record.is_some())
        })
    }

    fn layers_demoted(cx: &mut TestAppContext, window: AnyWindowHandle) -> u64 {
        with_window(cx, window, |window, _| window.layout_stats().layers_demoted)
    }

    #[crate::test]
    fn a_container_is_promoted_after_two_scrolled_frames(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert!(!has_record(cx, window));
        // A frame drawn in between for something else breaks the streak.
        frame_after(cx, window, |cx| {
            handle.update(cx, |_, _, cx| cx.notify()).unwrap();
        });
        assert_eq!(decision(cx, window), Some(Decision::Bypass));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert!(!has_record(cx, window));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Repaint));
        assert!(has_record(cx, window));
    }

    #[crate::test]
    fn deferred_draws_inside_make_it_ineligible(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        with_extra(cx, handle, || {
            div()
                .h(px(10.))
                .child(deferred(
                    anchored().child(div().w(px(50.)).h(px(50.)).bg(crate::red())),
                ))
                .into_any_element()
        });
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert!(!has_record(cx, window));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
    }

    #[crate::test]
    fn anchored_elements_inside_make_it_ineligible(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        // Positioned against the window's edges when prepainted, which a
        // composited layer would move with the content.
        with_extra(cx, handle, || {
            div()
                .h(px(10.))
                .child(anchored().child(div().w(px(50.)).h(px(50.)).bg(crate::red())))
                .into_any_element()
        });
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        // On today's path while it holds one, but not demoted (spec §6.5).
        assert_eq!(layers_demoted(cx, window), 0);
        assert!(!has_record(cx, window));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
    }

    #[crate::test]
    fn an_animation_frame_requested_by_the_owner_makes_it_ineligible(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        frame_after(cx, window, |cx| {
            handle
                .update(cx, |page, _, cx| {
                    page.animate = true;
                    cx.notify();
                })
                .unwrap();
        });
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert!(!has_record(cx, window));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
    }

    #[crate::test]
    fn a_focused_input_inside_makes_it_ineligible(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        let focus = with_window(cx, window, |_, cx| cx.focus_handle());
        let input_focus = focus.clone();
        with_extra(cx, handle, move || {
            div()
                .h(px(10.))
                .track_focus(&input_focus)
                .child(super::super::retained::text_input(
                    input_focus.clone(),
                    "inside",
                ))
                .into_any_element()
        });
        with_window(cx, window, |window, cx| window.focus(&focus, cx));
        draw(cx, window);
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert!(!has_record(cx, window));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
    }

    #[crate::test]
    fn a_layer_whose_input_loses_focus_is_composited_again_soon(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        // A focused input keeps the container on today's path only while
        // it is focused: it is not demoted, which would keep the container
        // waiting for 60 stable frames (spec §6.5).
        let handle = page(cx, false);
        let window = handle.into();
        let focus = with_window(cx, window, |_, cx| cx.focus_handle());
        let input_focus = focus.clone();
        with_extra(cx, handle, move || {
            div()
                .h(px(10.))
                .track_focus(&input_focus)
                .child(super::super::retained::text_input(
                    input_focus.clone(),
                    "inside",
                ))
                .into_any_element()
        });
        with_window(cx, window, |window, cx| window.focus(&focus, cx));
        draw(cx, window);
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        frame_after(cx, window, |cx| {
            with_window(cx, window, |window, cx| window.blur(cx));
        });
        let mut decisions = Vec::new();
        // Down, where there is room to scroll: at the top the wheel would
        // move nothing, which is no scroll.
        for _ in 0..12 {
            decisions.push(scroll(cx, window, -10.).unwrap());
        }
        assert!(
            decisions.contains(&Decision::Composite),
            "composited again within 12 frames: {decisions:?}"
        );
        assert_eq!(layers_demoted(cx, window), 0);
    }

    /// Content painting a path, drawn over by `cover` when set.
    fn with_path(cx: &mut TestAppContext, handle: WindowHandle<LayerPage>, cover: bool) {
        with_extra(cx, handle, move || {
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    // Where it shows once scrolled by the 40 px promotion
                    // takes; out of view it is culled.
                    let origin = bounds.origin + point(px(0.), px(60.));
                    let mut path = Path::new(origin);
                    path.line_to(origin + point(px(10.), px(0.)));
                    path.line_to(origin + point(px(10.), px(10.)));
                    path.line_to(origin);
                    window.paint_path(path, crate::red());
                    if cover {
                        window.paint_quad(crate::fill(
                            crate::Bounds::new(origin, size(px(5.), px(5.))),
                            crate::blue().opacity(0.5),
                        ));
                    }
                },
            )
            .w(px(10.))
            .h(px(100.))
            .into_any_element()
        });
    }

    #[crate::test]
    fn content_with_paths_nothing_draws_over_composites(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        with_path(cx, handle, false);
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert_eq!(layers_demoted(cx, window), 0);
    }

    #[crate::test]
    fn content_drawing_over_a_path_is_demoted(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, false);
        let window = handle.into();
        with_path(cx, handle, true);
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert_eq!(layers_demoted(cx, window), 1);
        assert!(!has_record(cx, window));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
    }

    #[crate::test]
    fn churning_content_is_demoted_and_repromoted_after_60_stable_frames(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, true);
        let window = handle.into();
        let rows = handle
            .read_with(cx, |page, _| page.rows.clone().unwrap())
            .unwrap();
        let change = |tint: u32| {
            let rows = rows.clone();
            move |cx: &mut crate::App| {
                rows.update(cx, |rows, cx| {
                    rows.tint = tint;
                    cx.notify();
                })
            }
        };
        promote(cx, window);
        // Frequency and work guards both apply; expensive overscan may
        // demote the layer before the eighth changed frame.
        for tint in 1..=8 {
            if scroll_and(cx, window, -10., change(0x100000 + tint)) == Some(Decision::Bypass) {
                break;
            }
        }
        assert_eq!(layers_demoted(cx, window), 1);
        assert!(!has_record(cx, window));
        assert_eq!(scroll(cx, window, -10.), Some(Decision::Bypass));
        assert_eq!(scroll(cx, window, -10.), Some(Decision::Bypass));
        // A change while it is demoted starts the wait again.
        assert_eq!(
            scroll_and(cx, window, -10., change(0x300000)),
            Some(Decision::Bypass)
        );
        for _ in 0..59 {
            draw(cx, window);
        }
        assert_eq!(scroll(cx, window, -10.), Some(Decision::Bypass));
        assert_eq!(scroll(cx, window, -10.), Some(Decision::Repaint));
        assert_eq!(scroll(cx, window, -10.), Some(Decision::Composite));
    }

    #[crate::test]
    fn content_changing_every_other_frame_is_demoted(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, true);
        let window = handle.into();
        let rows = handle
            .read_with(cx, |page, _| page.rows.clone().unwrap())
            .unwrap();
        promote(cx, window);
        // A 60 Hz feed in a 120 Hz window must not keep repainting the
        // layer's overscan indefinitely at the eight-of-sixteen boundary.
        for tint in 1..=8 {
            if scroll(cx, window, -10.) == Some(Decision::Bypass) {
                break;
            }
            if scroll_and(cx, window, -10., |cx| {
                rows.update(cx, |rows, cx| {
                    rows.tint = 0x100000 + tint;
                    cx.notify();
                });
            }) == Some(Decision::Bypass)
            {
                break;
            }
        }
        assert_eq!(layers_demoted(cx, window), 1);
        assert!(!has_record(cx, window));
    }

    #[crate::test]
    fn overscan_repaints_below_the_frequency_threshold_are_demoted(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, true);
        let window = handle.into();
        let rows = handle
            .read_with(cx, |page, _| page.rows.clone().unwrap())
            .unwrap();
        promote(cx, window);
        for frame in 0..64 {
            scroll_and(cx, window, -5., |cx| {
                if frame % 4 == 0 {
                    rows.update(cx, |rows, cx| {
                        rows.tint += 1;
                        cx.notify();
                    });
                }
            });
        }
        assert_eq!(decision(cx, window), Some(Decision::Bypass));
        assert_eq!(layers_demoted(cx, window), 1);
        assert!(!has_record(cx, window));
    }

    #[crate::test]
    fn repeatedly_unprofitable_layers_extend_their_cooldown(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = page(cx, true);
        let window = handle.into();
        let rows = handle
            .read_with(cx, |page, _| page.rows.clone().unwrap())
            .unwrap();
        promote(cx, window);
        for cooldown in [60, 120, 240, 480, 960, 1920] {
            for _ in 0..8 {
                if scroll_and(cx, window, -1., |cx| {
                    rows.update(cx, |rows, cx| {
                        rows.tint += 1;
                        cx.notify();
                    });
                }) == Some(Decision::Bypass)
                {
                    break;
                }
            }
            let id = scroller_id(cx, window);
            let remaining = with_window(cx, window, |window, _| {
                window.fast_layers.layers[&id].policy.demoted_until.unwrap()
                    - (window.fast_layers.frame - 1)
            });
            assert_eq!(remaining, cooldown);
            assert!(!has_record(cx, window));
            for _ in 0..cooldown - 1 {
                draw(cx, window);
            }
            assert!(has_layer(cx, window, &id), "cooldown retains backoff");
            // A retained ancestor may skip the first resumed prepaint.
            // Give the wheel enough frames to promote the container again.
            for _ in 0..4 {
                scroll(cx, window, -1.);
                if has_record(cx, window) {
                    break;
                }
            }
            assert!(has_record(cx, window), "stable content is promoted again");
            assert_eq!(scroll(cx, window, -1.), Some(Decision::Composite));
        }
        assert_eq!(layers_demoted(cx, window), 6);
        // The last recovery followed 1920 quiet frames, so a new period
        // of expensive refreshes starts with the initial cooldown again.
        for _ in 0..8 {
            if scroll_and(cx, window, -1., |cx| {
                rows.update(cx, |rows, cx| {
                    rows.tint += 1;
                    cx.notify();
                });
            }) == Some(Decision::Bypass)
            {
                break;
            }
        }
        let id = scroller_id(cx, window);
        assert_eq!(
            with_window(cx, window, |window, _| {
                window.fast_layers.layers[&id].policy.demoted_until.unwrap()
                    - (window.fast_layers.frame - 1)
            }),
            60
        );
        assert_eq!(layers_demoted(cx, window), 7);
    }

    #[crate::test]
    fn a_layer_not_composited_for_120_frames_is_dropped(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = page(cx, false).into();
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        let id = scroller_id(cx, window);
        for _ in 0..119 {
            draw(cx, window);
        }
        assert!(has_layer(cx, window, &id));
        draw(cx, window);
        assert!(!has_layer(cx, window, &id));
    }

    #[crate::test]
    fn resize_drops_layers(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let window = page(cx, false).into();
        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        let id = scroller_id(cx, window);
        cx.simulate_window_resize(window, size(px(800.), px(500.)));
        draw(cx, window);
        assert!(!has_layer(cx, window, &id));

        promote(cx, window);
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        cx.simulate_window_scale_factor_change(window, 1.5);
        draw(cx, window);
        assert!(!has_layer(cx, window, &id));
    }
}

/// Tests of input into scroll layers (M5): the content's hitboxes and
/// listeners carried through composited frames, the content brought up to
/// date before it sees input, and hit testing and scroll handle bounds kept
/// true.
mod input {
    use super::decisions::{decision, frame_after};
    use super::invalidation::{draw, with_window};
    use crate::fast::layers::policy::Decision;
    use crate::{
        AnyWindowHandle, AppContext as _, Bounds, Context, DispatchPhase, FocusHandle,
        HitboxBehavior, InteractiveElement as _, IntoElement, Modifiers, MouseButton,
        MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Render,
        ScrollDelta, ScrollHandle, ScrollWheelEvent, StatefulInteractiveElement as _, Styled as _,
        TestAppContext, TouchPhase, Window, WindowHandle, canvas, div, point, px, rgb,
    };
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    const ROWS: usize = 40;
    const ROW_HEIGHT: f32 = 20.;

    /// What a listener inside a row saw: the row, the bounds of the hitbox
    /// it captured, and the event's position.
    type Seen = (usize, Bounds<Pixels>, Point<Pixels>);

    /// A 200 × 100 px scroll container of forty 20 px rows, 100 px wide, that
    /// can be clicked and dragged, at the top left of the window. Each row
    /// holds a canvas whose mouse-down listener notes the bounds of the
    /// hitbox it captured when painted. Row 2 can take focus and counts the
    /// keys it sees. The wheel scrolls it from beside the rows, where it
    /// hovers none of them. With `inner`, the rows are in a 200 px wide div
    /// that `inner` tracks, with a tooltip if `tooltip` is set.
    struct InputPage {
        handle: ScrollHandle,
        clicks: Rc<RefCell<Vec<usize>>>,
        keys: Rc<Cell<usize>>,
        focus: FocusHandle,
        seen: Rc<RefCell<Vec<Seen>>>,
        drags: Rc<RefCell<Vec<Seen>>>,
        inner: Option<ScrollHandle>,
        tooltip: bool,
        /// Paints the page's background translucent, which a layer's tiles
        /// cannot be cleared with.
        translucent: bool,
    }

    /// What a row of an [`InputPage`] is dragged as.
    struct DraggedRow;

    /// A canvas filling a row, whose mouse-down listener notes into `seen`
    /// the bounds of the hitbox it captured when painted.
    fn noting_canvas(index: usize, seen: Rc<RefCell<Vec<Seen>>>) -> impl IntoElement {
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
                    if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                        seen.borrow_mut()
                            .push((index, hitbox.bounds, event.position));
                    }
                });
            },
        )
        .size_full()
    }

    impl Render for InputPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rows = (0..ROWS).map(|index| {
                let clicks = self.clicks.clone();
                let drags = self.drags.clone();
                let row = div()
                    .id(("row", index))
                    .w(px(100.))
                    .h(px(ROW_HEIGHT))
                    .bg(rgb(0x100000 + index as u32 * 0x10))
                    .on_click(move |_, _, _| clicks.borrow_mut().push(index))
                    .on_drag(DraggedRow, |_, _, _, cx| cx.new(|_| super::EmptyView))
                    .on_drag_move::<DraggedRow>(move |event, _, _| {
                        drags
                            .borrow_mut()
                            .push((index, event.bounds, event.event.position))
                    })
                    .child(noting_canvas(index, self.seen.clone()));
                if index == 2 {
                    let keys = self.keys.clone();
                    row.track_focus(&self.focus)
                        .on_key_down(move |_, _, _| keys.set(keys.get() + 1))
                } else {
                    row
                }
            });
            let scroller = div()
                .id("scroller")
                .overflow_y_scroll()
                .track_scroll(&self.handle)
                .w(px(200.))
                .h(px(100.));
            let scroller = match &self.inner {
                Some(inner) => {
                    let inner = div()
                        .id("inner")
                        .track_scroll(inner)
                        .w(px(200.))
                        .children(rows);
                    if self.tooltip {
                        scroller.child(inner.tooltip(|_, cx| cx.new(|_| super::EmptyView).into()))
                    } else {
                        scroller.child(inner)
                    }
                }
                None => scroller.children(rows),
            };
            let background = if self.translucent {
                crate::rgba(0xffffff80)
            } else {
                rgb(0xffffff)
            };
            div().size_full().bg(background).child(scroller)
        }
    }

    fn input_page(cx: &mut TestAppContext) -> WindowHandle<InputPage> {
        input_page_with(cx, None, false)
    }

    fn input_page_with(
        cx: &mut TestAppContext,
        inner: Option<ScrollHandle>,
        tooltip: bool,
    ) -> WindowHandle<InputPage> {
        let window = cx.add_window(move |_, cx| InputPage {
            handle: ScrollHandle::new(),
            clicks: Rc::default(),
            keys: Rc::default(),
            focus: cx.focus_handle(),
            seen: Rc::default(),
            drags: Rc::default(),
            inner,
            tooltip,
            translucent: false,
        });
        draw(cx, window.into());
        draw(cx, window.into());
        window
    }

    /// Scrolls by `dy` with the wheel beside the rows, returning what the
    /// scroll container decided in the frame that followed.
    fn scroll(cx: &mut TestAppContext, window: AnyWindowHandle, dy: f32) -> Option<Decision> {
        frame_after(cx, window, |cx| {
            with_window(cx, window, |window, cx| {
                window.dispatch_event(
                    crate::PlatformInput::ScrollWheel(ScrollWheelEvent {
                        position: point(px(150.), px(20.)),
                        delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
                        modifiers: Default::default(),
                        touch_phase: TouchPhase::Moved,
                    }),
                    cx,
                );
            })
        });
        decision(cx, window)
    }

    fn click(cx: &mut TestAppContext, window: AnyWindowHandle, position: Point<Pixels>) {
        dispatch(cx, window, [mouse_down(position), mouse_up(position)]);
    }

    fn mouse_down(position: Point<Pixels>) -> crate::PlatformInput {
        crate::PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        })
    }

    fn mouse_up(position: Point<Pixels>) -> crate::PlatformInput {
        crate::PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        })
    }

    fn mouse_move(position: Point<Pixels>, pressed: bool) -> crate::PlatformInput {
        crate::PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: pressed.then_some(MouseButton::Left),
            modifiers: Modifiers::default(),
        })
    }

    /// Dispatches `events` to `window` in one update, with no frame drawn
    /// between them unless dispatching one draws it.
    fn dispatch(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        events: impl IntoIterator<Item = crate::PlatformInput>,
    ) {
        with_window(cx, window, |window, cx| {
            for event in events {
                window.dispatch_event(event, cx);
            }
        });
    }

    fn rebuilds(cx: &mut TestAppContext, window: AnyWindowHandle) -> u64 {
        with_window(cx, window, |window, _| {
            window.layout_stats().layer_rebuilds_for_input
        })
    }

    /// A page with layers, and one without, for the same history to be
    /// played in both.
    fn page_with_and_without_layers(
        cx: &mut TestAppContext,
    ) -> (WindowHandle<InputPage>, WindowHandle<InputPage>) {
        pages_with_and_without_layers(cx, None, None)
    }

    /// [`page_with_and_without_layers`], the rows of each tracked by the
    /// handle given for it.
    fn pages_with_and_without_layers(
        cx: &mut TestAppContext,
        with_inner: Option<ScrollHandle>,
        without_inner: Option<ScrollHandle>,
    ) -> (WindowHandle<InputPage>, WindowHandle<InputPage>) {
        let with = input_page_with(cx, with_inner, false);
        let without = input_page_with(cx, without_inner, false);
        without
            .update(cx, |_, window, _| window.set_scroll_layers(false))
            .unwrap();
        draw(cx, without.into());
        (with, without)
    }

    /// Scrolls the page five times by -20 px: promoted on the second
    /// scroll, painted at -40 px, and composited from the third on, at
    /// -100 px after the fifth. Returns the decisions of the last three.
    fn scroll_five_times(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
    ) -> Vec<Option<Decision>> {
        (0..5)
            .map(|_| scroll(cx, window, -20.))
            .collect::<Vec<_>>()
            .split_off(2)
    }

    fn clicks(cx: &mut TestAppContext, window: WindowHandle<InputPage>) -> Vec<usize> {
        window
            .update(cx, |page, _, _| page.clicks.borrow().clone())
            .unwrap()
    }

    /// Promotes the page's container, painting its layer at -40 px, and
    /// composites it twice, at -60 and -80 px.
    fn composite_twice(cx: &mut TestAppContext, window: AnyWindowHandle) {
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Repaint));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
    }

    #[crate::test]
    fn composited_frames_keep_the_content_interactive(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = input_page(cx);
        let window = handle.into();
        composite_twice(cx, window);
        // Row 6 lies at 120 px in the content, at 40 px once scrolled by -80.
        let row_6 = with_window(cx, window, |window, _| {
            window.rendered_frame.hitboxes.iter().any(|hitbox| {
                hitbox.bounds.origin == point(px(0.), px(40.))
                    && hitbox.bounds.size.height == px(ROW_HEIGHT)
            })
        });
        assert!(row_6, "the row's hitbox is carried, moved by the scroll");
        click(cx, window, point(px(20.), px(50.)));
        assert_eq!(clicks(cx, handle), vec![6]);
    }

    #[crate::test]
    fn keyboard_focus_inside_survives_composite_frames(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = input_page(cx);
        let window: AnyWindowHandle = handle.into();
        handle
            .update(cx, |page, window, cx| window.focus(&page.focus, cx))
            .unwrap();
        draw(cx, window);
        composite_twice(cx, window);
        let focused = handle
            .update(cx, |page, window, _| {
                page.focus.is_focused(window)
                    && window
                        .rendered_frame
                        .dispatch_tree
                        .focusable_node_id(page.focus.id)
                        .is_some()
            })
            .unwrap();
        assert!(focused, "the focused row's dispatch node is carried");
        cx.simulate_keystrokes(window, "a");
        let keys = handle.update(cx, |page, _, _| page.keys.get()).unwrap();
        assert_eq!(keys, 1);
    }

    #[crate::test]
    fn click_after_scroll_hits_current_element(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (with, without) = page_with_and_without_layers(cx);
        assert_eq!(
            scroll_five_times(cx, with.into()),
            vec![Some(Decision::Composite); 3]
        );
        scroll_five_times(cx, without.into());
        // Row 7 lies at 140 px in the content, at 40 px once scrolled by -100.
        for window in [with, without] {
            click(cx, window.into(), point(px(20.), px(50.)));
            assert_eq!(clicks(cx, window), vec![7]);
        }
        let seen = |cx: &mut TestAppContext, window: WindowHandle<InputPage>| {
            window
                .update(cx, |page, _, _| page.seen.borrow().clone())
                .unwrap()
        };
        let seen_with = seen(cx, with);
        assert_eq!(
            seen_with,
            vec![(
                7,
                Bounds::new(point(px(0.), px(40.)), crate::size(px(100.), px(20.))),
                point(px(20.), px(50.))
            )]
        );
        assert_eq!(seen_with, seen(cx, without));
        assert_eq!(rebuilds(cx, with.into()), 1);
    }

    #[crate::test]
    fn drag_after_scroll_sees_current_bounds(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (with, without) = page_with_and_without_layers(cx);
        assert_eq!(
            scroll_five_times(cx, with.into()),
            vec![Some(Decision::Composite); 3]
        );
        scroll_five_times(cx, without.into());
        let drags = |cx: &mut TestAppContext, window: WindowHandle<InputPage>| {
            // The drag is the app's: it ends with the mouse up, before the
            // other window is dragged in.
            dispatch(
                cx,
                window.into(),
                [
                    mouse_down(point(px(20.), px(50.))),
                    mouse_move(point(px(20.), px(60.)), true),
                    mouse_move(point(px(20.), px(70.)), true),
                    mouse_up(point(px(20.), px(70.))),
                ],
            );
            window
                .update(cx, |page, _, _| page.drags.borrow().clone())
                .unwrap()
        };
        let drags_with = drags(cx, with);
        assert!(!drags_with.is_empty(), "the row was dragged");
        assert!(
            drags_with
                .iter()
                .all(|(index, bounds, _)| *index != 7 || bounds.origin.y == px(40.)),
            "{drags_with:?}"
        );
        assert_eq!(drags_with, drags(cx, without));
    }

    #[crate::test]
    fn wheel_events_do_not_rebuild(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = input_page(cx);
        let window = handle.into();
        scroll_five_times(cx, window);
        for _ in 0..10 {
            assert_eq!(scroll(cx, window, 5.), Some(Decision::Composite));
        }
        assert_eq!(rebuilds(cx, window), 0);
    }

    #[crate::test]
    fn a_mouse_move_after_scroll_rebuilds_once(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = input_page(cx);
        let window = handle.into();
        scroll_five_times(cx, window);
        dispatch(cx, window, [mouse_move(point(px(20.), px(50.)), false)]);
        assert_eq!(rebuilds(cx, window), 1);
        dispatch(cx, window, [mouse_move(point(px(30.), px(60.)), false)]);
        draw(cx, window);
        dispatch(cx, window, [mouse_move(point(px(40.), px(70.)), false)]);
        assert_eq!(rebuilds(cx, window), 1);
        // Outside the viewport, a move after a scroll does not rebuild. (The
        // first scroll moves the pointer off the rows, a hover change.)
        scroll(cx, window, -20.);
        // Repeated broad repaints now fall back rather than keeping the
        // expensive overscan alive.
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        dispatch(cx, window, [mouse_move(point(px(300.), px(300.)), false)]);
        assert_eq!(rebuilds(cx, window), 1);
    }

    /// Whether a row's hitbox takes the pointer at `position`.
    fn row_hit_at(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        position: Point<Pixels>,
    ) -> bool {
        with_window(cx, window, |window, _| {
            window.rendered_frame.hitboxes.iter().any(|hitbox| {
                hitbox.bounds.size.height == px(ROW_HEIGHT)
                    && hitbox
                        .bounds
                        .intersect(&hitbox.content_mask.bounds)
                        .contains(&position)
            })
        })
    }

    #[crate::test]
    fn hitboxes_in_overscan_do_not_hit(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = input_page(cx);
        let window = handle.into();
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Bypass));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Repaint));
        // Painted at -40 px, over the viewport and 100 px of overscan below
        // it: row 9 is painted at 140 px, below the container.
        assert!(!row_hit_at(cx, window, point(px(20.), px(150.))));
        assert!(row_hit_at(cx, window, point(px(20.), px(50.))));
        // Composited at -80 px, row 7, painted in the overscan at 100 px,
        // shows at 60 px, and takes the pointer there.
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert_eq!(scroll(cx, window, -20.), Some(Decision::Composite));
        assert!(row_hit_at(cx, window, point(px(20.), px(70.))));
        assert!(!row_hit_at(cx, window, point(px(20.), px(110.))));
    }

    fn tooltip_requested(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
        with_window(cx, window, |window, _| {
            window
                .rendered_frame
                .tooltip_requests
                .iter()
                .any(|request| request.is_some())
        })
    }

    #[crate::test]
    fn tooltips_requested_inside_are_dropped_on_composite(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let page = input_page_with(cx, Some(ScrollHandle::new()), true);
        let window: AnyWindowHandle = page.into();
        scroll_five_times(cx, window);
        // Hover the div around the rows until its tooltip shows.
        dispatch(cx, window, [mouse_move(point(px(150.), px(30.)), false)]);
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        draw(cx, window);
        draw(cx, window);
        assert!(tooltip_requested(cx, window));
        // Scrolled from outside, which keeps the tooltip up on today's path.
        let handle = page.update(cx, |page, _, _| page.handle.clone()).unwrap();
        let mut decisions = Vec::new();
        for offset in [-110., -120.] {
            handle.set_offset(point(px(0.), px(offset)));
            draw(cx, window);
            decisions.push(super::decisions::decision(cx, window));
        }
        assert_eq!(decisions.last(), Some(&Some(Decision::Composite)));
        assert!(!tooltip_requested(cx, window));
    }

    #[crate::test]
    fn bounds_for_item_is_current_after_composited_scrolls(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (with_inner, without_inner) = (ScrollHandle::new(), ScrollHandle::new());
        let (with, without) = pages_with_and_without_layers(
            cx,
            Some(with_inner.clone()),
            Some(without_inner.clone()),
        );
        assert_eq!(
            scroll_five_times(cx, with.into()),
            vec![Some(Decision::Composite); 3]
        );
        scroll_five_times(cx, without.into());
        // Row 7 lies at 140 px in the content, at 40 px once scrolled by -100.
        assert_eq!(
            with_inner.bounds_for_item(7).map(|bounds| bounds.origin),
            Some(point(px(0.), px(40.)))
        );
        for row in 0..ROWS {
            assert_eq!(
                with_inner.bounds_for_item(row),
                without_inner.bounds_for_item(row)
            );
        }
        // The container's own handle holds its children's bounds before
        // scrolling, which prepainting the container keeps current.
        let outer = |cx: &mut TestAppContext, window: WindowHandle<InputPage>| {
            window
                .update(cx, |page, _, _| page.handle.bounds_for_item(0))
                .unwrap()
        };
        assert_eq!(outer(cx, with), outer(cx, without));
    }

    /// How often observers of the page's view were told it changed, from
    /// now on.
    fn count_observations(
        cx: &mut TestAppContext,
        window: WindowHandle<InputPage>,
    ) -> Rc<Cell<usize>> {
        let count = Rc::new(Cell::new(0));
        let entity = window.update(cx, |_, _, cx| cx.entity()).unwrap();
        let counter = count.clone();
        cx.update(|cx| {
            cx.observe(&entity, move |_, _| counter.set(counter.get() + 1))
                .detach()
        });
        count
    }

    #[crate::test]
    fn a_rebuild_for_input_is_not_seen_by_observers(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (with, without) = page_with_and_without_layers(cx);
        let mut counts = Vec::new();
        for window in [with, without] {
            let count = count_observations(cx, window);
            scroll_five_times(cx, window.into());
            dispatch(
                cx,
                window.into(),
                [mouse_move(point(px(20.), px(50.)), false)],
            );
            cx.run_until_parked();
            counts.push(count.get());
        }
        assert_eq!(rebuilds(cx, with.into()), 1);
        assert_eq!(counts[0], counts[1]);
    }

    fn repaints(cx: &mut TestAppContext, window: AnyWindowHandle) -> u64 {
        with_window(cx, window, |window, _| {
            window.layout_stats().layer_frames_repainted
        })
    }

    /// A move that lands beside the content — on a scrollbar being dragged,
    /// say — does not rebuild a scrolled layer: only the content's own
    /// elements compare pointer positions with bounds they hold. A move onto
    /// the content does.
    #[crate::test]
    fn only_a_move_onto_the_content_rebuilds_it(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = input_page(cx);
        let window: AnyWindowHandle = handle.into();
        scroll_five_times(cx, window);
        for step in 0..6 {
            let y = px(50. + step as f32);
            dispatch(cx, window, [mouse_move(point(px(150.), y), true)]);
        }
        assert_eq!(rebuilds(cx, window), 0, "moves beside the rows");
        dispatch(cx, window, [mouse_move(point(px(20.), px(50.)), false)]);
        assert_eq!(rebuilds(cx, window), 1, "a move onto a row");
    }

    #[crate::test]
    fn moves_beside_the_content_neither_rebuild_nor_demote(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = input_page(cx);
        let window: AnyWindowHandle = handle.into();
        scroll_five_times(cx, window);
        // Scrolling with the pointer moving in the viewport between scrolls,
        // beside the rows, where it lands on none of the content: no move
        // rebuilds, and none changes the content.
        for step in 0..12 {
            assert_eq!(
                scroll(cx, window, 5.),
                Some(Decision::Composite),
                "step {step}"
            );
            let y = px(50. + (step % 2) as f32);
            let repainted = repaints(cx, window);
            dispatch(cx, window, [mouse_move(point(px(150.), y), false)]);
            assert_eq!(repaints(cx, window), repainted, "step {step}");
        }
        assert_eq!(rebuilds(cx, window), 0);
        let id = super::decisions::scroller_id(cx, window);
        let (demoted, changes) = with_window(cx, window, |window, _| {
            let layer = &window.fast_layers.layers[&id];
            (
                window.layout_stats().layers_demoted,
                layer.policy.change_history.count_ones(),
            )
        });
        assert_eq!((demoted, changes), (0, 0));
    }

    #[crate::test]
    fn content_painted_without_tiles_is_not_rebuilt_for_input(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = input_page(cx);
        let window: AnyWindowHandle = handle.into();
        scroll_five_times(cx, window);
        // The layer's tiles cannot be cleared with a translucent background:
        // its content is painted straight into the frame, at the offset
        // shown.
        let repainted = repaints(cx, window);
        frame_after(cx, window, |cx| {
            handle
                .update(cx, |page, _, cx| {
                    page.translucent = true;
                    cx.notify();
                })
                .unwrap();
        });
        assert_eq!(decision(cx, window), Some(Decision::Repaint));
        assert_eq!(repaints(cx, window), repainted, "painted without tiles");
        dispatch(cx, window, [mouse_move(point(px(20.), px(50.)), false)]);
        assert_eq!(rebuilds(cx, window), 0);
    }

    #[crate::test]
    fn bounds_for_item_matches_after_the_tracked_element_goes(cx: &mut TestAppContext) {
        if !crate::fast::layers::COMPILED {
            return;
        }
        let (with_inner, without_inner) = (ScrollHandle::new(), ScrollHandle::new());
        let (with, without) = pages_with_and_without_layers(
            cx,
            Some(with_inner.clone()),
            Some(without_inner.clone()),
        );
        assert_eq!(
            scroll_five_times(cx, with.into()),
            vec![Some(Decision::Composite); 3]
        );
        scroll_five_times(cx, without.into());
        // The rows' div stops being rendered; its handle, kept, holds the
        // bounds its children had when last shown, as without a layer.
        for window in [with, without] {
            window
                .update(cx, |page, _, cx| {
                    page.inner = None;
                    cx.notify();
                })
                .unwrap();
            draw(cx, window.into());
        }
        for _ in 0..3 {
            scroll(cx, with.into(), 5.);
            scroll(cx, without.into(), 5.);
        }
        for row in 0..ROWS {
            assert_eq!(
                with_inner.bounds_for_item(row),
                without_inner.bounds_for_item(row),
                "row {row}"
            );
        }
    }
}

/// Tests of the streams working together, found by measuring and verifying
/// real scroll scenarios (M8).
mod integration {
    use std::sync::Arc;

    use super::super::oracle::GlyphBoxTextSystem;
    use crate::{
        AppContext as _, Context, IntoElement, NoopTextSystem, ParentElement as _, Render,
        Styled as _, TestAppContext, Window, div, px,
    };

    /// One line of 8 px text on a 40 px line, whose top is `top`.
    struct TallLine {
        top: f32,
    }

    impl Render for TallLine {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                div()
                    .absolute()
                    .top(px(self.top))
                    .text_size(px(8.))
                    .line_height(px(40.))
                    .child("A"),
            )
        }
    }

    fn glyphs_drawn(top: f32) -> usize {
        let mut cx = TestAppContext::with_text_system(Arc::new(GlyphBoxTextSystem(NoopTextSystem)));
        let window = cx.add_window(|_, _| TallLine { top });
        cx.update_window(window.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            window.rendered_frame.scene.monochrome_sprites.len()
                + window.rendered_frame.scene.subpixel_sprites.len()
        })
        .unwrap()
    }

    /// Content whose path nothing draws over composites its tiles with the
    /// path drawn over them, and draws what it draws without a layer at every
    /// offset, odd numbers of device pixels included.
    #[crate::test]
    fn a_path_drawn_over_the_tiles_matches_drawing_without_layers(cx: &mut TestAppContext) {
        path_over_tiles_matches_drawing_without_layers(cx, 1.);
    }

    #[crate::test]
    fn a_path_drawn_over_the_tiles_matches_drawing_without_layers_at_a_fractional_scale(
        cx: &mut TestAppContext,
    ) {
        path_over_tiles_matches_drawing_without_layers(cx, 1.25);
        path_over_tiles_matches_drawing_without_layers(cx, 1.6);
    }

    fn path_over_tiles_matches_drawing_without_layers(cx: &mut TestAppContext, scale_factor: f32) {
        use super::decisions::{LayerPage, new_page};
        use crate::{Path, canvas, point, size};
        let composited = matches_drawing_without_layers(cx, scale_factor, |cx| LayerPage {
            extra: Some(std::rc::Rc::new(|| {
                canvas(
                    |_, _, _| {},
                    |bounds, _, window, _| {
                        // Two paths, the second over the first, as a progress
                        // circle draws its track and its arc.
                        for (inset, color) in [(0., crate::red()), (3., crate::green())] {
                            let origin = bounds.origin + point(px(20. + inset), px(70.));
                            let mut path = Path::new(origin);
                            path.line_to(origin + point(px(12.), px(0.)));
                            path.curve_to(
                                origin + point(px(0.), px(12.)),
                                origin + point(px(14.), px(14.)),
                            );
                            path.line_to(origin);
                            window.paint_path(path, color);
                        }
                        window.paint_quad(crate::fill(
                            crate::Bounds::new(bounds.origin, size(px(10.), px(10.))),
                            crate::blue(),
                        ));
                    },
                )
                .w(px(100.))
                .h(px(100.))
                .into_any_element()
            })),
            ..new_page(false, cx)
        });
        assert!(composited >= 6, "composited {composited} frames");
    }

    /// A scrollbar beside the container, which reads the offset where it is
    /// prepainted and painted, moves with the offset while the content is
    /// composited, as it does without layers.
    #[crate::test]
    fn a_scrollbar_beside_a_composited_layer_matches_drawing_without_layers(
        cx: &mut TestAppContext,
    ) {
        use super::decisions::{LayerPage, new_page};
        for scale_factor in [1., 1.6] {
            let composited = matches_drawing_without_layers(cx, scale_factor, |cx| LayerPage {
                scrollbar: true,
                ..new_page(false, cx)
            });
            assert!(composited >= 6, "composited {composited} frames");
        }
    }

    /// Scrolls a window with layers and one without, each holding the page
    /// `page` builds, at `scale_factor`, and checks after every scroll that
    /// they draw the same. Returns how many frames composited a layer.
    fn matches_drawing_without_layers<V: crate::Render>(
        cx: &mut TestAppContext,
        scale_factor: f32,
        page: impl Fn(&mut crate::App) -> V,
    ) -> u64 {
        use crate::point;
        if !crate::fast::layers::COMPILED {
            return u64::MAX;
        }
        let mut open = |cx: &mut TestAppContext, layers: bool| {
            let window = cx.add_window(|_, cx| page(cx));
            cx.simulate_window_scale_factor_change(window.into(), scale_factor);
            cx.update_window(window.into(), |_, window, cx| {
                window.set_scroll_layers(layers);
                window.draw(cx).clear(cx);
            })
            .unwrap();
            window
        };
        let layered = open(cx, true);
        let plain = open(cx, false);
        let drawn = |cx: &mut TestAppContext, window: crate::WindowHandle<V>| {
            cx.update_window(window.into(), |_, window, _| window.painted_primitives())
                .unwrap()
        };
        for (step, dy) in [-20., -20., -7., -3., -11., -1., 5., -13., -2.]
            .into_iter()
            .enumerate()
        {
            for window in [layered, plain] {
                cx.update_window(window.into(), |_, window, cx| {
                    window.dispatch_event(
                        crate::PlatformInput::ScrollWheel(crate::ScrollWheelEvent {
                            position: point(px(20.), px(20.)),
                            delta: crate::ScrollDelta::Pixels(point(px(0.), px(dy))),
                            modifiers: Default::default(),
                            touch_phase: crate::TouchPhase::Moved,
                        }),
                        cx,
                    );
                    window.draw(cx).clear(cx);
                })
                .unwrap();
            }
            let (actual, expected) = (drawn(cx, layered), drawn(cx, plain));
            if actual != expected {
                let only = |a: &[String], b: &[String]| -> Vec<String> {
                    a.iter().filter(|line| !b.contains(line)).cloned().collect()
                };
                panic!(
                    "scale {scale_factor}, step {step}, scrolled by {dy}: only with layers \
                     {:#?}, only without {:#?}",
                    only(&actual, &expected),
                    only(&expected, &actual)
                );
            }
        }
        cx.update_window(layered.into(), |_, window, _| {
            window.layout_stats().layer_frames_composited
        })
        .unwrap()
    }

    /// Rows in a view of their own, counting how often it renders.
    struct CountedRows {
        renders: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl Render for CountedRows {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            use crate::rgb;
            self.renders.set(self.renders.get() + 1);
            div().children(
                (0..40).map(|ix| div().h(px(20.)).bg(rgb(0x100000 + ix * 0x10)).child("row")),
            )
        }
    }

    /// A scroll container whose content is a child view (pattern A).
    struct ChildViewPage {
        rows: crate::Entity<CountedRows>,
    }

    impl Render for ChildViewPage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            use crate::{InteractiveElement as _, StatefulInteractiveElement as _, rgb};
            div().size_full().bg(rgb(0xffffff)).child(
                div()
                    .id("scroller")
                    .overflow_y_scroll()
                    .w(px(200.))
                    .h(px(100.))
                    .child(self.rows.clone()),
            )
        }
    }

    fn child_view_page(
        cx: &mut crate::App,
    ) -> (ChildViewPage, std::rc::Rc<std::cell::Cell<usize>>) {
        let renders = std::rc::Rc::new(std::cell::Cell::new(0));
        let rows = cx.new(|_| CountedRows {
            renders: renders.clone(),
        });
        (ChildViewPage { rows }, renders)
    }

    /// A frame that composites a layer whose content is a child view lays
    /// the view out as it was, without rendering it (spec §6.3, pattern A):
    /// only the view holding the container is rendered.
    #[crate::test]
    fn a_composited_child_view_is_not_rendered(cx: &mut TestAppContext) {
        use super::decisions::{promote, scroll};
        use crate::fast::layers::policy::Decision;
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = cx.add_window(|_, cx| child_view_page(cx).0);
        let renders = handle
            .read_with(cx, |page, cx| page.rows.read(cx).renders.clone())
            .unwrap();
        let window: crate::AnyWindowHandle = handle.into();
        for _ in 0..2 {
            cx.update_window(window, |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
        }
        promote(cx, window);
        assert_eq!(scroll(cx, window, -10.), Some(Decision::Composite));
        let before = renders.get();
        for step in 0..4 {
            assert_eq!(
                scroll(cx, window, -10.),
                Some(Decision::Composite),
                "step {step}"
            );
        }
        assert_eq!(renders.get(), before, "the child view was not rendered");
    }

    /// A child view laid out from what its layer kept draws as without
    /// layers, on frames that composite it and on frames that paint it
    /// afresh.
    #[crate::test]
    fn a_composited_child_view_matches_drawing_without_layers(cx: &mut TestAppContext) {
        for scale_factor in [1., 1.6] {
            let composited =
                matches_drawing_without_layers(cx, scale_factor, |cx| child_view_page(cx).0);
            assert!(composited >= 6, "composited {composited} frames");
        }
    }

    /// A scroll container around a smaller one, its content no taller than
    /// itself, as GPUI Kit's gallery holds a story's own scroll area.
    struct NestedScrollers {
        outer: crate::ScrollHandle,
        inner: crate::ScrollHandle,
    }

    impl Render for NestedScrollers {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            use crate::{InteractiveElement as _, StatefulInteractiveElement as _, rgb};
            div().size_full().bg(rgb(0xffffff)).child(
                div()
                    .id("outer")
                    .overflow_y_scroll()
                    .track_scroll(&self.outer)
                    .w(px(200.))
                    .h(px(100.))
                    .child(
                        div()
                            .id("scroller")
                            .overflow_y_scroll()
                            .track_scroll(&self.inner)
                            .w(px(200.))
                            .h(px(100.))
                            .children(
                                (0..40).map(|ix| div().h(px(20.)).bg(rgb(0x100000 + ix * 0x10))),
                            ),
                    ),
            )
        }
    }

    /// A wheel over the inner container moves the outer one's offset too,
    /// which it cannot scroll and puts back when it is prepainted: the outer
    /// container did not scroll, and the inner one gets the layer.
    #[crate::test]
    fn a_container_that_cannot_scroll_is_not_promoted_by_the_wheel(cx: &mut TestAppContext) {
        use super::decisions::{decision, promote, scroll};
        use crate::fast::layers::policy::Decision;
        if !crate::fast::layers::COMPILED {
            return;
        }
        let handle = cx.add_window(|_, _| NestedScrollers {
            outer: crate::ScrollHandle::new(),
            inner: crate::ScrollHandle::new(),
        });
        let window: crate::AnyWindowHandle = handle.into();
        for _ in 0..2 {
            cx.update_window(window, |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
        }
        promote(cx, window);
        for step in 0..4 {
            assert_eq!(
                scroll(cx, window, -10.),
                Some(Decision::Composite),
                "step {step}"
            );
        }
        let outer_painted = cx
            .update_window(window, |_, window, _| {
                window
                    .fast_layers
                    .layers
                    .iter()
                    .any(|(id, layer)| id.last() == Some(&"outer".into()) && layer.record.is_some())
            })
            .unwrap();
        assert!(!outer_painted, "the outer container has no layer");
        assert_eq!(decision(cx, window), Some(Decision::Composite));
    }

    /// A line's glyphs never reach into an empty content mask, however near
    /// it they lie: a box straddling the mask's edge does not overlap it.
    #[test]
    fn no_glyph_reaches_into_an_empty_mask() {
        use crate::{Bounds, point, size};
        let glyph = Bounds::new(point(px(10.), px(10.)), size(px(8.), px(10.)));
        let empty = Bounds::new(point(px(0.), px(15.)), size(px(100.), px(0.)));
        let thin = Bounds::new(point(px(0.), px(15.)), size(px(100.), px(1.)));
        assert!(!crate::fast::glyphs::may_reach(glyph, px(8.), &empty));
        assert!(crate::fast::glyphs::may_reach(glyph, px(8.), &thin));
    }

    /// A glyph sits on its line's baseline, below the top of a tall line: it
    /// is drawn wherever it reaches into the content mask, even when the top
    /// of its line, where upstream looks for it, is outside the mask. A
    /// scroll layer paints the line in its overscan and shows the glyph, so
    /// drawing from scratch must show it too.
    #[test]
    fn a_glyph_low_on_a_tall_line_is_drawn_where_it_reaches_into_the_mask() {
        assert_eq!(glyphs_drawn(10.), 1, "a line inside the window");
        assert_eq!(glyphs_drawn(-100.), 0, "a line far above the window");
        // The glyph lies between 15 and 24 px below the line's top, so a line
        // 20 px above the window shows its bottom 4 px.
        assert_eq!(glyphs_drawn(-20.), 1, "a glyph reaching into the window");
    }
}
