// Modified by Longbridge for gpui-fast.
//! What the core hands a renderer about scroll layers: the tiles it
//! composites, as polychrome sprites whose texture id lies in a range no
//! atlas allocates, and the content those tiles are rasterized from.

use crate::{
    AtlasTextureId, AtlasTextureKind, Bounds, Point, Rgba, ScaledPixels, Scene, TileId, point,
    scene::{PaintOperation, Primitive, TransformationMatrix},
    size,
};
use std::rc::Rc;

/// A live scroll layer, stable while it lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayerKey(pub u32);

/// A tile of a layer's content space, in units of the tile size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileCoord {
    /// The tile's column; tile `x` starts at `x * tile_size` device pixels.
    pub x: i32,
    /// The tile's row; tile `y` starts at `y * tile_size` device pixels.
    pub y: i32,
}

/// The first texture index of layer tiles; atlases allocate indices from 0.
pub const LAYER_TILE_TEXTURE_BASE: u32 = 0xF000_0000;
pub(crate) const LAYER_KEY_LIMIT: u32 = 0x0100_0000;
const TILE_COORD_BIAS: i32 = 2048;

/// The texture id the tiles of `layer` are composited with.
pub fn layer_tile_texture_id(layer: LayerKey) -> AtlasTextureId {
    debug_assert!(layer.0 < LAYER_KEY_LIMIT);
    AtlasTextureId {
        index: LAYER_TILE_TEXTURE_BASE + layer.0,
        kind: AtlasTextureKind::Polychrome,
    }
}

/// The tile id a composited `tile` carries.
pub fn layer_tile_id(tile: TileCoord) -> TileId {
    debug_assert!((-TILE_COORD_BIAS..TILE_COORD_BIAS).contains(&tile.x));
    debug_assert!((-TILE_COORD_BIAS..TILE_COORD_BIAS).contains(&tile.y));
    let x = (tile.x + TILE_COORD_BIAS) as u32;
    let y = (tile.y + TILE_COORD_BIAS) as u32;
    TileId((x << 16) | y)
}

/// The layer and tile a composited sprite stands for, if it is one.
pub fn decode_layer_tile(texture: AtlasTextureId, tile: TileId) -> Option<(LayerKey, TileCoord)> {
    if texture.kind != AtlasTextureKind::Polychrome
        || texture.index < LAYER_TILE_TEXTURE_BASE
        || texture.index - LAYER_TILE_TEXTURE_BASE >= LAYER_KEY_LIMIT
    {
        return None;
    }
    let layer = LayerKey(texture.index - LAYER_TILE_TEXTURE_BASE);
    let x = (tile.0 >> 16) as i32 - TILE_COORD_BIAS;
    let y = (tile.0 & 0xFFFF) as i32 - TILE_COORD_BIAS;
    Some((layer, TileCoord { x, y }))
}

/// The scroll layers a frame composites, for the renderer to rasterize their
/// tiles from.
#[derive(Default)]
pub struct SceneLayers {
    /// One per layer the frame composites.
    pub frames: Vec<LayerFrame>,
}

impl SceneLayers {
    /// Forgets the frame's layers, as [`Scene::clear`] forgets its primitives.
    pub fn clear(&mut self) {
        self.frames.clear();
    }
}

/// A layer the frame composites: the content its tiles are rasterized from.
#[derive(Clone)]
pub struct LayerFrame {
    /// The layer, stable while it lives.
    pub key: LayerKey,
    /// Bumps whenever `content` is replaced.
    pub generation: u64,
    /// The opaque colour every tile is cleared with before its content is drawn.
    pub background: Rgba,
    /// The side of a tile, in device pixels.
    pub tile_size: u32,
    /// The painted content in content space.
    pub content: LayerContent,
    /// The tiles whose content changed in this generation.
    pub dirty_tiles: Vec<TileCoord>,
}

impl LayerFrame {
    /// The rectangle `tile` covers in content space.
    pub fn tile_bounds(&self, tile: TileCoord) -> Bounds<ScaledPixels> {
        let side = self.tile_size as f32;
        Bounds {
            origin: point(
                ScaledPixels(tile.x as f32 * side),
                ScaledPixels(tile.y as f32 * side),
            ),
            size: size(ScaledPixels(side), ScaledPixels(side)),
        }
    }

    /// The primitives of `content` that are visible over `tile`, translated
    /// into the tile's space (its top-left corner at the origin), in drawing
    /// order, finished and ready to batch.
    pub fn tile_scene(&self, tile: TileCoord) -> Scene {
        let bounds = self.tile_bounds(tile);
        let delta = point(
            ScaledPixels(-bounds.origin.x.0),
            ScaledPixels(-bounds.origin.y.0),
        );
        let mut scene = Scene::default();
        for operation in self.content.operations_over(bounds) {
            match operation {
                PaintOperation::Primitive(primitive) => {
                    if visible_bounds(primitive).intersects(&bounds) {
                        scene.insert_primitive(translate_primitive(primitive, delta));
                    }
                }
                PaintOperation::StartLayer(layer_bounds) => {
                    scene.push_layer(translate_bounds(*layer_bounds, delta))
                }
                PaintOperation::EndLayer => scene.pop_layer(),
            }
        }
        scene.finish();
        scene
    }
}

/// A layer's content, in content space: one or more parts drawn one after
/// another. A scrolling `div`'s content is one part, a finished scene; a
/// virtual list's is one part per row, so that a frame adding or changing
/// rows hands the renderer the rows it kept as they were, without copying
/// them.
#[derive(Clone, Default)]
pub struct LayerContent {
    parts: Rc<[LayerPart]>,
}

/// A part of a layer's content.
#[derive(Clone)]
pub(crate) struct LayerPart {
    /// Where the part's primitives can draw, if known: a tile it misses
    /// draws nothing of it.
    pub(crate) bounds: Option<Bounds<ScaledPixels>>,
    /// The part's paint operations, in drawing order. A row's scene holds
    /// nothing else.
    pub(crate) scene: Rc<Scene>,
}

impl From<Scene> for LayerContent {
    fn from(scene: Scene) -> Self {
        Self::from(Rc::new(scene))
    }
}

impl From<Rc<Scene>> for LayerContent {
    fn from(scene: Rc<Scene>) -> Self {
        LayerContent {
            parts: Rc::new([LayerPart {
                bounds: None,
                scene,
            }]),
        }
    }
}

impl LayerContent {
    /// Content made of `parts`, drawn in order.
    pub(crate) fn from_parts(parts: impl IntoIterator<Item = LayerPart>) -> Self {
        LayerContent {
            parts: parts.into_iter().collect(),
        }
    }

    /// The content's scene, when it is one part, as a `div`'s is.
    pub fn scene(&self) -> Option<&Scene> {
        match &*self.parts {
            [part] => Some(&part.scene),
            _ => None,
        }
    }

    /// The scenes of the content's parts, in order.
    #[cfg(test)]
    pub(crate) fn part_scenes(&self) -> impl Iterator<Item = &Rc<Scene>> {
        self.parts.iter().map(|part| &part.scene)
    }

    /// Whether `self` and `other` are the same content, shared.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.parts, &other.parts)
    }

    /// Every paint operation of the content, in drawing order.
    pub(crate) fn operations(&self) -> impl Iterator<Item = &PaintOperation> {
        self.parts
            .iter()
            .flat_map(|part| part.scene.paint_operations.iter())
    }

    /// The paint operations of the parts that can draw over `bounds`, in
    /// drawing order. Leaving out a part that draws nothing there changes
    /// no pixel there: a layer it pushed only raises the draw orders of what
    /// follows, keeping every overlapping pair in order.
    pub(crate) fn operations_over(
        &self,
        bounds: Bounds<ScaledPixels>,
    ) -> impl Iterator<Item = &PaintOperation> {
        self.parts
            .iter()
            .filter(move |part| part.bounds.is_none_or(|part| part.intersects(&bounds)))
            .flat_map(|part| part.scene.paint_operations.iter())
    }
}

fn translate_bounds(
    bounds: Bounds<ScaledPixels>,
    delta: Point<ScaledPixels>,
) -> Bounds<ScaledPixels> {
    Bounds {
        origin: bounds.origin + delta,
        size: bounds.size,
    }
}

/// The part of window (or content) space `primitive` can draw into: what
/// the renderer rasterizes for it, clipped to its content mask. A drop
/// shadow reaches three blur radii past its bounds and an inset one fills
/// its element's bounds (`vs_shadow`); a mono or subpixel sprite's bounds
/// are transformed (`to_device_position_transformed`).
pub(crate) fn visible_bounds(primitive: &Primitive) -> Bounds<ScaledPixels> {
    drawn_bounds(primitive).intersect(&primitive.content_mask().bounds)
}

/// The part of window (or content) space `primitive` can draw into, its
/// content mask aside. See [`visible_bounds`].
pub(crate) fn drawn_bounds(primitive: &Primitive) -> Bounds<ScaledPixels> {
    match primitive {
        Primitive::Shadow(shadow) if shadow.inset != 0 => shadow.element_bounds,
        Primitive::Shadow(shadow) => {
            let margin = ScaledPixels(3. * shadow.blur_radius.0.max(0.));
            Bounds {
                origin: point(
                    shadow.bounds.origin.x - margin,
                    shadow.bounds.origin.y - margin,
                ),
                size: size(
                    shadow.bounds.size.width + margin * 2.,
                    shadow.bounds.size.height + margin * 2.,
                ),
            }
        }
        Primitive::MonochromeSprite(sprite) => {
            transformed_bounds(sprite.bounds, &sprite.transformation)
        }
        Primitive::SubpixelSprite(sprite) => {
            transformed_bounds(sprite.bounds, &sprite.transformation)
        }
        primitive => *primitive.bounds(),
    }
}

/// The smallest rectangle holding `bounds` transformed by `matrix`.
fn transformed_bounds(
    bounds: Bounds<ScaledPixels>,
    matrix: &TransformationMatrix,
) -> Bounds<ScaledPixels> {
    if *matrix == TransformationMatrix::unit() {
        return bounds;
    }
    let apply = |x: f32, y: f32| {
        let m = &matrix.rotation_scale;
        (
            m[0][0] * x + m[0][1] * y + matrix.translation[0],
            m[1][0] * x + m[1][1] * y + matrix.translation[1],
        )
    };
    let (x0, y0) = (bounds.origin.x.0, bounds.origin.y.0);
    let (x1, y1) = (x0 + bounds.size.width.0, y0 + bounds.size.height.0);
    let corners = [apply(x0, y0), apply(x1, y0), apply(x0, y1), apply(x1, y1)];
    let min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
    let max_x = corners
        .iter()
        .map(|c| c.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
    let max_y = corners
        .iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    Bounds {
        origin: point(ScaledPixels(min_x), ScaledPixels(min_y)),
        size: size(ScaledPixels(max_x - min_x), ScaledPixels(max_y - min_y)),
    }
}

/// `primitive` moved by `delta`: every position it carries, its content mask
/// included, so it draws the same pixels `delta` away.
pub(crate) fn translate_primitive(primitive: &Primitive, delta: Point<ScaledPixels>) -> Primitive {
    let mut primitive = primitive.clone();
    move_primitive(&mut primitive, delta);
    primitive
}

/// Moves `primitive` by `delta`, as [`translate_primitive`] does, in place.
pub(crate) fn move_primitive(primitive: &mut Primitive, delta: Point<ScaledPixels>) {
    let mv = |bounds: &mut Bounds<ScaledPixels>| *bounds = translate_bounds(*bounds, delta);
    match primitive {
        Primitive::Shadow(shadow) => {
            mv(&mut shadow.bounds);
            mv(&mut shadow.element_bounds);
            mv(&mut shadow.content_mask.bounds);
        }
        Primitive::Quad(quad) => {
            mv(&mut quad.bounds);
            mv(&mut quad.content_mask.bounds);
        }
        Primitive::Path(path) => {
            mv(&mut path.bounds);
            mv(&mut path.content_mask.bounds);
            for vertex in &mut path.vertices {
                vertex.xy_position = vertex.xy_position + delta;
                mv(&mut vertex.content_mask.bounds);
            }
        }
        Primitive::Underline(underline) => {
            mv(&mut underline.bounds);
            mv(&mut underline.content_mask.bounds);
        }
        Primitive::MonochromeSprite(sprite) => {
            mv(&mut sprite.bounds);
            mv(&mut sprite.content_mask.bounds);
            translate_transformation(&mut sprite.transformation, delta);
        }
        Primitive::SubpixelSprite(sprite) => {
            mv(&mut sprite.bounds);
            mv(&mut sprite.content_mask.bounds);
            translate_transformation(&mut sprite.transformation, delta);
        }
        Primitive::PolychromeSprite(sprite) => {
            mv(&mut sprite.bounds);
            mv(&mut sprite.content_mask.bounds);
        }
        Primitive::Surface(surface) => {
            mv(&mut surface.bounds);
            mv(&mut surface.content_mask.bounds);
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        Primitive::Backdrop(backdrop) => {
            mv(&mut backdrop.bounds);
            mv(&mut backdrop.sample_bounds);
            mv(&mut backdrop.content_mask.bounds);
        }
    }
}

/// A sprite's transformation applies to window positions (`R·p + t`, see
/// `to_device_position_transformed` in the shaders), so for the sprite to
/// move by `delta` with it, `t` becomes `t + (I − R)·delta`. The unit
/// transformation stays the unit.
fn translate_transformation(matrix: &mut TransformationMatrix, delta: Point<ScaledPixels>) {
    let r = matrix.rotation_scale;
    let (dx, dy) = (delta.x.0, delta.y.0);
    matrix.translation[0] += dx - (r[0][0] * dx + r[0][1] * dy);
    matrix.translation[1] += dy - (r[1][0] * dx + r[1][1] * dy);
}
