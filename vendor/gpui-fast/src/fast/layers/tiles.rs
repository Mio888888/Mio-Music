// Modified by Longbridge for gpui-fast.
//! Hashing a layer's tiles and finding the ones a repaint changed (M3).
//!
//! A tile's hash covers what [`crate::LayerFrame::tile_scene`] would draw
//! over it: the primitives whose visible bounds (a shadow's blur and a
//! transformed sprite included) reach the tile, in drawing order, moved
//! into the tile's space. Two tiles hashing alike draw the same pixels, so a
//! repaint that leaves a tile's hash alone keeps its texture.
//!
//! Content masks and layer bounds are hashed clipped to the tile: pixels
//! outside it never depend on them. While a layer paints, every mask is
//! the painted region, which moves in content space with the scroll offset;
//! hashed whole, it would dirty every tile of a repaint at a new offset.

use crate::{
    AtlasTile, Background, Bounds, ContentMask, Corners, Edges, Hsla, Point, ScaledPixels, Scene,
    TileCoord, TransformationMatrix,
    fast::layers::scene::{drawn_bounds, translate_primitive, visible_bounds},
    point,
    scene::{PaintOperation, Primitive},
    size,
};
use collections::{FxHashMap, FxHasher};
use std::hash::Hasher;

/// The hash of every tile of `region` (content space, device pixels), empty
/// tiles included: the primitives of `content` visible over each, in drawing
/// order, moved into the tile's space.
pub(crate) fn tile_hashes(
    content: &Scene,
    tile_size: u32,
    region: Bounds<ScaledPixels>,
) -> FxHashMap<TileCoord, u64> {
    let tile = tile_rect(tile_size);
    let mut hashers: FxHashMap<TileCoord, FxHasher> = FxHashMap::default();
    for coord in tiles_over(region, tile_size) {
        hashers.insert(coord, FxHasher::default());
    }
    let mut open_layers: Vec<Bounds<ScaledPixels>> = Vec::new();
    for operation in &content.paint_operations {
        match operation {
            PaintOperation::Primitive(primitive) => {
                let visible = visible_bounds(primitive);
                for_tiles(&mut hashers, visible, tile_size, |hasher, origin| {
                    hasher.write_u8(0);
                    hash_primitive(&translate_primitive(primitive, origin), &tile, hasher);
                });
            }
            PaintOperation::StartLayer(bounds) => {
                open_layers.push(*bounds);
                for_tiles(&mut hashers, *bounds, tile_size, |hasher, origin| {
                    hasher.write_u8(1);
                    hash_clipped(
                        &Bounds {
                            origin: bounds.origin + origin,
                            size: bounds.size,
                        },
                        &tile,
                        hasher,
                    );
                });
            }
            PaintOperation::EndLayer => {
                if let Some(bounds) = open_layers.pop() {
                    for_tiles(&mut hashers, bounds, tile_size, |hasher, _| {
                        hasher.write_u8(2)
                    });
                }
            }
        }
    }
    hashers
        .into_iter()
        .map(|(tile, hasher)| (tile, hasher.finish()))
        .collect()
}

/// The hash of what the paint operations `operations` (content space,
/// device pixels) draw, and the tiles they reach, each with that hash: a
/// virtual list's row, hashed once when it is painted, in one pass. See
/// [`combine_tile_hashes`].
///
/// Each primitive is hashed with its masks clipped to the part of space it
/// draws in, outside which they change no pixel: a row painted again with
/// the part of the content painted around it grown or moved hashes alike.
/// A primitive drawing nothing is left out, and so are the bounds of the
/// layers the operations push: both only raise the draw orders of what
/// follows, which keeps every overlapping pair in order.
pub(crate) fn part_tile_hashes(
    operations: &[PaintOperation],
    tile_size: u32,
) -> (Vec<(TileCoord, u64)>, Option<Bounds<ScaledPixels>>) {
    let mut hasher = FxHasher::default();
    let mut reach: Option<Bounds<ScaledPixels>> = None;
    for operation in operations {
        match operation {
            PaintOperation::Primitive(primitive) => {
                let drawn = drawn_bounds(primitive);
                let visible = drawn.intersect(&primitive.content_mask().bounds);
                if visible.size.width.0 <= 0. || visible.size.height.0 <= 0. {
                    continue;
                }
                reach = Some(reach.map_or(visible, |reach| reach.union(&visible)));
                hasher.write_u8(0);
                hash_primitive(primitive, &drawn, &mut hasher);
            }
            PaintOperation::StartLayer(_) => hasher.write_u8(1),
            PaintOperation::EndLayer => hasher.write_u8(2),
        }
    }
    let hash = hasher.finish();
    let tiles = reach
        .map(|reach| {
            tiles_over(reach, tile_size)
                .map(|tile| (tile, hash))
                .collect()
        })
        .unwrap_or_default();
    (tiles, reach)
}

/// The hash of every tile of content made of parts drawn in order, each
/// with the hash of what it draws over each tile it reaches (see
/// [`part_tile_hashes`]), and of every tile of `region`, empty or not. Two
/// tiles hashing alike draw the same pixels, as with [`tile_hashes`]: a
/// tile's pixels are what the parts reaching it draw, in order.
pub(crate) fn combine_tile_hashes<'a>(
    parts: impl IntoIterator<Item = &'a [(TileCoord, u64)]>,
    tile_size: u32,
    region: Bounds<ScaledPixels>,
) -> FxHashMap<TileCoord, u64> {
    let mut hashers: FxHashMap<TileCoord, FxHasher> = FxHashMap::default();
    for coord in tiles_over(region, tile_size) {
        hashers.insert(coord, FxHasher::default());
    }
    for part in parts {
        for (coord, hash) in part {
            hashers.entry(*coord).or_default().write_u64(*hash);
        }
    }
    hashers
        .into_iter()
        .map(|(tile, hasher)| (tile, hasher.finish()))
        .collect()
}

/// The tiles of `new` whose hash differs from `old`'s or that `old` lacks,
/// sorted.
pub(crate) fn dirty_tiles(
    old: &FxHashMap<TileCoord, u64>,
    new: &FxHashMap<TileCoord, u64>,
) -> Vec<TileCoord> {
    let mut dirty: Vec<TileCoord> = new
        .iter()
        .filter(|(tile, hash)| old.get(tile) != Some(hash))
        .map(|(tile, _)| *tile)
        .collect();
    dirty.sort();
    dirty
}

/// The tiles `bounds` overlaps, in rows.
pub(crate) fn tiles_over(
    bounds: Bounds<ScaledPixels>,
    tile_size: u32,
) -> impl Iterator<Item = TileCoord> {
    let side = tile_size as f32;
    let (x0, x1, y0, y1) = if bounds.size.width.0 <= 0. || bounds.size.height.0 <= 0. {
        (0, -1, 0, -1)
    } else {
        let max = bounds.bottom_right();
        (
            (bounds.origin.x.0 / side).floor() as i32,
            (max.x.0 / side).ceil() as i32 - 1,
            (bounds.origin.y.0 / side).floor() as i32,
            (max.y.0 / side).ceil() as i32 - 1,
        )
    };
    (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| TileCoord { x, y }))
}

/// Calls `f` with the hasher of each tile of `hashers` that `bounds`
/// overlaps, and the offset that moves content into that tile's space.
fn for_tiles(
    hashers: &mut FxHashMap<TileCoord, FxHasher>,
    bounds: Bounds<ScaledPixels>,
    tile_size: u32,
    mut f: impl FnMut(&mut FxHasher, Point<ScaledPixels>),
) {
    let side = tile_size as f32;
    for tile in tiles_over(bounds, tile_size) {
        if let Some(hasher) = hashers.get_mut(&tile) {
            let origin = point(
                ScaledPixels(-(tile.x as f32) * side),
                ScaledPixels(-(tile.y as f32) * side),
            );
            f(hasher, origin);
        }
    }
}

/// A tile's rect in its own space.
fn tile_rect(tile_size: u32) -> Bounds<ScaledPixels> {
    let side = ScaledPixels(tile_size as f32);
    Bounds {
        origin: Point::default(),
        size: size(side, side),
    }
}

/// Feeds every field of `primitive`, in the space of the tile `tile`, that
/// decides its pixels in the tile to `hasher`: all but its draw order and
/// ids, which the scene assigns, with its masks clipped to the tile.
fn hash_primitive(primitive: &Primitive, tile: &Bounds<ScaledPixels>, hasher: &mut FxHasher) {
    match primitive {
        Primitive::Shadow(shadow) => {
            hasher.write_u8(0);
            hash_f32(shadow.blur_radius.0, hasher);
            hash_bounds(&shadow.bounds, hasher);
            hash_corners(&shadow.corner_radii, hasher);
            hash_mask(&shadow.content_mask, tile, hasher);
            hash_hsla(&shadow.color, hasher);
            hash_bounds(&shadow.element_bounds, hasher);
            hash_corners(&shadow.element_corner_radii, hasher);
            hasher.write_u32(shadow.inset);
        }
        Primitive::Quad(quad) => {
            hasher.write_u8(1);
            hasher.write_u8(quad.border_style as u8);
            hash_bounds(&quad.bounds, hasher);
            hash_mask(&quad.content_mask, tile, hasher);
            hash_background(&quad.background, hasher);
            hash_hsla(&quad.border_color, hasher);
            hash_corners(&quad.corner_radii, hasher);
            hash_edges(&quad.border_widths, hasher);
        }
        Primitive::Path(path) => {
            hasher.write_u8(2);
            hash_bounds(&path.bounds, hasher);
            hash_mask(&path.content_mask, tile, hasher);
            hash_background(&path.color, hasher);
            hasher.write_usize(path.vertices.len());
            for vertex in &path.vertices {
                hash_f32(vertex.xy_position.x.0, hasher);
                hash_f32(vertex.xy_position.y.0, hasher);
                hash_f32(vertex.st_position.x, hasher);
                hash_f32(vertex.st_position.y, hasher);
                hash_mask(&vertex.content_mask, tile, hasher);
            }
        }
        Primitive::Underline(underline) => {
            hasher.write_u8(3);
            hash_bounds(&underline.bounds, hasher);
            hash_mask(&underline.content_mask, tile, hasher);
            hash_hsla(&underline.color, hasher);
            hash_f32(underline.thickness.0, hasher);
            hasher.write_u8((underline.wavy == true.into()) as u8);
        }
        Primitive::MonochromeSprite(sprite) => {
            hasher.write_u8(4);
            hash_bounds(&sprite.bounds, hasher);
            hash_mask(&sprite.content_mask, tile, hasher);
            hash_hsla(&sprite.color, hasher);
            hash_tile(&sprite.tile, hasher);
            hash_transformation(&sprite.transformation, hasher);
        }
        Primitive::SubpixelSprite(sprite) => {
            hasher.write_u8(5);
            hash_bounds(&sprite.bounds, hasher);
            hash_mask(&sprite.content_mask, tile, hasher);
            hash_hsla(&sprite.color, hasher);
            hash_tile(&sprite.tile, hasher);
            hash_transformation(&sprite.transformation, hasher);
        }
        Primitive::PolychromeSprite(sprite) => {
            hasher.write_u8(6);
            hasher.write_u8((sprite.grayscale == true.into()) as u8);
            hash_f32(sprite.opacity, hasher);
            hash_bounds(&sprite.bounds, hasher);
            hash_mask(&sprite.content_mask, tile, hasher);
            hash_corners(&sprite.corner_radii, hasher);
            hash_tile(&sprite.tile, hasher);
        }
        Primitive::Surface(surface) => {
            // A surface's pixels come from outside the scene; layers never
            // hold one (spec §6.5), so it only needs to hash apart.
            hasher.write_u8(7);
            hash_bounds(&surface.bounds, hasher);
            hash_mask(&surface.content_mask, tile, hasher);
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Primitive::Backdrop(backdrop) => {
            // A backdrop samples the frame behind it; layers never hold one
            // (the content is painted into the frame instead), so it only
            // needs to hash apart.
            hasher.write_u8(8);
            hash_bounds(&backdrop.bounds, hasher);
            hash_mask(&backdrop.content_mask, tile, hasher);
            hash_corners(&backdrop.corner_radii, hasher);
            hash_f32(backdrop.blur_radius, hasher);
            hash_f32(backdrop.saturation, hasher);
            hash_f32(backdrop.opacity, hasher);
        }
    }
}

fn hash_f32(value: f32, hasher: &mut FxHasher) {
    hasher.write_u32(value.to_bits());
}

fn hash_bounds(bounds: &Bounds<ScaledPixels>, hasher: &mut FxHasher) {
    hash_f32(bounds.origin.x.0, hasher);
    hash_f32(bounds.origin.y.0, hasher);
    hash_f32(bounds.size.width.0, hasher);
    hash_f32(bounds.size.height.0, hasher);
}

fn hash_mask(mask: &ContentMask<ScaledPixels>, tile: &Bounds<ScaledPixels>, hasher: &mut FxHasher) {
    hash_clipped(&mask.bounds, tile, hasher);
}

/// Hashes the part of `bounds` inside `tile`, all empty parts alike.
fn hash_clipped(bounds: &Bounds<ScaledPixels>, tile: &Bounds<ScaledPixels>, hasher: &mut FxHasher) {
    let clipped = bounds.intersect(tile);
    if clipped.is_empty() {
        hash_bounds(&Bounds::default(), hasher);
    } else {
        hash_bounds(&clipped, hasher);
    }
}

fn hash_hsla(color: &Hsla, hasher: &mut FxHasher) {
    hash_f32(color.h, hasher);
    hash_f32(color.s, hasher);
    hash_f32(color.l, hasher);
    hash_f32(color.a, hasher);
}

fn hash_corners(corners: &Corners<ScaledPixels>, hasher: &mut FxHasher) {
    hash_f32(corners.top_left.0, hasher);
    hash_f32(corners.top_right.0, hasher);
    hash_f32(corners.bottom_right.0, hasher);
    hash_f32(corners.bottom_left.0, hasher);
}

fn hash_edges(edges: &Edges<ScaledPixels>, hasher: &mut FxHasher) {
    hash_f32(edges.top.0, hasher);
    hash_f32(edges.right.0, hasher);
    hash_f32(edges.bottom.0, hasher);
    hash_f32(edges.left.0, hasher);
}

fn hash_background(background: &Background, hasher: &mut FxHasher) {
    hasher.write_u8(background.tag as u8);
    hasher.write_u8(background.color_space as u8);
    hash_hsla(&background.solid, hasher);
    hash_f32(background.gradient_angle_or_pattern_height, hasher);
    for stop in &background.colors {
        hash_hsla(&stop.color, hasher);
        hash_f32(stop.percentage, hasher);
    }
}

fn hash_tile(tile: &AtlasTile, hasher: &mut FxHasher) {
    hasher.write_u32(tile.texture_id.index);
    hasher.write_u8(tile.texture_id.kind as u8);
    hasher.write_u32(tile.tile_id.0);
    hasher.write_u32(tile.padding);
    hasher.write_i32(tile.bounds.origin.x.0);
    hasher.write_i32(tile.bounds.origin.y.0);
    hasher.write_i32(tile.bounds.size.width.0);
    hasher.write_i32(tile.bounds.size.height.0);
}

fn hash_transformation(matrix: &TransformationMatrix, hasher: &mut FxHasher) {
    for row in &matrix.rotation_scale {
        hash_f32(row[0], hasher);
        hash_f32(row[1], hasher);
    }
    hash_f32(matrix.translation[0], hasher);
    hash_f32(matrix.translation[1], hasher);
}
