// Modified by Longbridge for gpui-fast.
//! A scene orders its primitives with `fast/bounds_tree.rs`'s grid, which must
//! give every primitive the ordering upstream's tree does, so that a scene
//! batches and draws exactly what it did.
//!
//! Random scenes of overlapping quads, text layers and glyph sprites, some
//! reaching past the window or covering much of it, are painted over several
//! frames — the same frame again, which the tree replays, and new ones. Every
//! primitive's ordering must be the one worked out by comparing it with
//! everything painted before it, and every two primitives that overlap must be
//! drawn in the order they were painted, as read from the batches the
//! renderer would draw: the batch each is in, then its place in the batch.

use rand::{Rng as _, SeedableRng as _, rngs::StdRng};

use crate::{
    AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, ContentMask, Corners, DevicePixels,
    DrawOrder, Hsla, MonochromeSprite, PolychromeSprite, PrimitiveBatch, Quad, ScaledPixels, Scene,
    SubpixelSprite, TileId, TransformationMatrix, hsla, point, size,
};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Quad,
    Monochrome,
    Subpixel,
    Polychrome,
}

struct Painted {
    kind: Kind,
    /// The primitive's bounds clipped to its content mask.
    bounds: Bounds<ScaledPixels>,
    /// The layer the primitive was painted in, whose primitives all share the
    /// layer's ordering and are not ordered among themselves.
    layer: Option<usize>,
    /// The ordering it must be given.
    expected_order: DrawOrder,
}

/// Orders bounds the way the bounds tree must: one past the greatest
/// ordering among the bounds before them that they intersect.
#[derive(Default)]
struct Reference(Vec<(Bounds<ScaledPixels>, DrawOrder)>);

impl Reference {
    fn insert(&mut self, bounds: Bounds<ScaledPixels>) -> DrawOrder {
        let order = self
            .0
            .iter()
            .filter(|(other, _)| other.intersects(&bounds))
            .map(|(_, order)| *order)
            .max()
            .unwrap_or(0)
            + 1;
        self.0.push((bounds, order));
        order
    }
}

fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
    Bounds {
        origin: point(ScaledPixels(x), ScaledPixels(y)),
        size: size(ScaledPixels(w), ScaledPixels(h)),
    }
}

fn tile(id: usize, kind: AtlasTextureKind, texture: u32) -> AtlasTile {
    AtlasTile {
        texture_id: AtlasTextureId {
            index: texture,
            kind,
        },
        tile_id: TileId(id as u32),
        padding: 0,
        bounds: Bounds {
            origin: point(DevicePixels(0), DevicePixels(0)),
            size: size(DevicePixels(1), DevicePixels(1)),
        },
    }
}

const WINDOW: f32 = 1200.;

/// Paints a random scene into `scene`, and returns what it painted, in order.
fn paint(scene: &mut Scene, rng: &mut StdRng) -> Vec<Painted> {
    const COLORS: [Hsla; 3] = [
        hsla(0.0, 0.0, 0.1, 1.0),
        hsla(0.6, 0.7, 0.5, 1.0),
        hsla(0.3, 0.6, 0.4, 0.5),
    ];
    let window_mask = ContentMask {
        bounds: bounds(0., 0., WINDOW, WINDOW),
    };
    let mut reference = Reference::default();
    let mut content_mask = window_mask;
    let mut painted = Vec::new();
    let mut layers = 0;
    let mut layer: Option<(usize, DrawOrder)> = None;
    for _ in 0..rng.random_range(1..120) {
        match rng.random_range(0..10) {
            0..3 => {
                let id = painted.len();
                // Mostly small, sometimes a panel covering much of the window,
                // sometimes on a cell's edge.
                let b = match rng.random_range(0..6) {
                    0 => bounds(
                        rng.random_range(-100.0..WINDOW),
                        rng.random_range(-100.0..WINDOW),
                        rng.random_range(100.0..1500.),
                        rng.random_range(100.0..1500.),
                    ),
                    1 => bounds(
                        rng.random_range(0..20) as f32 * 64.,
                        rng.random_range(0..20) as f32 * 64.,
                        rng.random_range(1..5) as f32 * 64.,
                        rng.random_range(1..5) as f32 * 64.,
                    ),
                    _ => bounds(
                        rng.random_range(-20.0..WINDOW),
                        rng.random_range(-20.0..WINDOW),
                        rng.random_range(1.0..150.),
                        rng.random_range(1.0..60.),
                    ),
                };
                let clipped = b.intersect(&content_mask.bounds);
                if clipped.is_empty() {
                    continue;
                }
                scene.insert_primitive(Quad {
                    bounds: b,
                    content_mask,
                    corner_radii: Corners::all(ScaledPixels(id as f32)),
                    ..Default::default()
                });
                painted.push(Painted {
                    kind: Kind::Quad,
                    bounds: clipped,
                    layer: layer.map(|(layer, _)| layer),
                    expected_order: match layer {
                        Some((_, order)) => order,
                        None => reference.insert(clipped),
                    },
                });
            }
            3 => {
                if layer.is_some() {
                    scene.pop_layer();
                    layer = None;
                    content_mask = window_mask;
                } else {
                    // A text line's layer, which may reach past the window.
                    content_mask.bounds = bounds(
                        rng.random_range(-200.0..WINDOW),
                        rng.random_range(-200.0..WINDOW),
                        rng.random_range(0.0..600.),
                        rng.random_range(0.0..40.),
                    );
                    scene.push_layer(content_mask.bounds);
                    layer = Some((layers, reference.insert(content_mask.bounds)));
                    layers += 1;
                }
            }
            _ => {
                // A line of glyphs, each overlapping the ones beside it.
                let mut x = rng.random_range(-10.0..WINDOW);
                let y = rng.random_range(-10.0..WINDOW);
                let mut color = COLORS[rng.random_range(0..COLORS.len())];
                let mut kind = Kind::Monochrome;
                for _ in 0..rng.random_range(1..30) {
                    if rng.random_bool(0.2) {
                        color = COLORS[rng.random_range(0..COLORS.len())];
                    }
                    if rng.random_bool(0.1) {
                        kind = [Kind::Monochrome, Kind::Subpixel, Kind::Polychrome]
                            [rng.random_range(0..3)];
                    }
                    let id = painted.len();
                    let b = bounds(
                        x,
                        y + rng.random_range(-3.0..3.),
                        rng.random_range(2.0..12.),
                        rng.random_range(8.0..16.),
                    );
                    x += rng.random_range(-2.0..9.);
                    let texture = rng.random_range(0..2);
                    let clipped = b.intersect(&content_mask.bounds);
                    if clipped.is_empty() {
                        continue;
                    }
                    match kind {
                        Kind::Monochrome => scene.insert_primitive(MonochromeSprite {
                            order: 0,
                            pad: 0,
                            bounds: b,
                            content_mask,
                            color,
                            tile: tile(id, AtlasTextureKind::Monochrome, texture),
                            transformation: TransformationMatrix::unit(),
                        }),
                        Kind::Subpixel => scene.insert_primitive(SubpixelSprite {
                            order: 0,
                            pad: 0,
                            bounds: b,
                            content_mask,
                            color,
                            tile: tile(id, AtlasTextureKind::Subpixel, texture),
                            transformation: TransformationMatrix::unit(),
                        }),
                        Kind::Polychrome => scene.insert_primitive(PolychromeSprite {
                            order: 0,
                            pad: 0,
                            grayscale: false.into(),
                            opacity: 1.,
                            bounds: b,
                            content_mask,
                            corner_radii: Corners::default(),
                            tile: tile(id, AtlasTextureKind::Polychrome, texture),
                        }),
                        Kind::Quad => unreachable!(),
                    }
                    painted.push(Painted {
                        kind,
                        bounds: clipped,
                        layer: layer.map(|(layer, _)| layer),
                        expected_order: match layer {
                            Some((_, order)) => order,
                            None => reference.insert(clipped),
                        },
                    });
                }
            }
        }
    }
    if layer.is_some() {
        scene.pop_layer();
    }
    painted
}

/// Where each painted primitive is drawn, by its index in painting order —
/// its batch's position among the batches, then its own in the batch — and
/// the ordering it was given.
fn draw_positions(scene: &Scene, count: usize) -> Vec<((usize, usize), DrawOrder)> {
    let mut positions = vec![((usize::MAX, usize::MAX), 0); count];
    for (batch_ix, batch) in scene.batches().enumerate() {
        match batch {
            PrimitiveBatch::Quads(range) => {
                for ix in range {
                    let quad = &scene.quads[ix];
                    positions[quad.corner_radii.top_left.0 as usize] = ((batch_ix, ix), quad.order);
                }
            }
            PrimitiveBatch::MonochromeSprites { range, .. } => {
                for ix in range {
                    let sprite = &scene.monochrome_sprites[ix];
                    positions[sprite.tile.tile_id.0 as usize] = ((batch_ix, ix), sprite.order);
                }
            }
            PrimitiveBatch::SubpixelSprites { range, .. } => {
                for ix in range {
                    let sprite = &scene.subpixel_sprites[ix];
                    positions[sprite.tile.tile_id.0 as usize] = ((batch_ix, ix), sprite.order);
                }
            }
            PrimitiveBatch::PolychromeSprites { range, .. } => {
                for ix in range {
                    let sprite = &scene.polychrome_sprites[ix];
                    positions[sprite.tile.tile_id.0 as usize] = ((batch_ix, ix), sprite.order);
                }
            }
            _ => unreachable!(),
        }
    }
    positions
}

/// Checks every primitive's ordering, and that every two overlapping
/// primitives outside a shared layer are drawn in the order they were
/// painted.
fn check(scene: &Scene, painted: &[Painted], seed: u64, frame: u64) {
    let positions = draw_positions(scene, painted.len());
    for (later_ix, later) in painted.iter().enumerate() {
        let (later_position, order) = positions[later_ix];
        assert_eq!(
            order, later.expected_order,
            "seed {seed} frame {frame}: {later_ix} ({:?} {:?}) ordered {order}, \
             where it intersects what makes it {}",
            later.kind, later.bounds, later.expected_order
        );
        for (earlier_ix, earlier) in painted[..later_ix].iter().enumerate() {
            let same_layer = earlier.layer.is_some() && earlier.layer == later.layer;
            if same_layer || !earlier.bounds.intersects(&later.bounds) {
                continue;
            }
            assert!(
                positions[earlier_ix].0 < later_position,
                "seed {seed} frame {frame}: {earlier_ix} ({:?}) is painted before \
                 {later_ix} ({:?}) and overlaps it, but is drawn at {:?}, after {:?}",
                earlier.kind,
                later.kind,
                positions[earlier_ix].0,
                later_position,
            );
        }
    }
}

#[test]
fn scenes_are_ordered_and_drawn_in_painting_order() {
    for seed in 0..400 {
        let mut scene = Scene::default();
        // Frames 0 and 1 are the same scene, which the second replays; the
        // rest are each new.
        for frame in 0..5 {
            let mut rng = StdRng::seed_from_u64(seed * 16 + frame.max(1));
            scene.clear();
            let painted = paint(&mut scene, &mut rng);
            scene.finish();
            check(&scene, &painted, seed, frame);
        }
    }
}
