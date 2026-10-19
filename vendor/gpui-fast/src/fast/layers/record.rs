// Modified by Longbridge for gpui-fast.
//! A layer's recorded content: its content-space scene, painted region,
//! translation at paint, prepaint and paint ranges and dependencies (M3).

use crate::{
    Bounds, EntityId, GlobalElementId, HitboxId, PaintIndex, Path, Pixels, Point,
    PrepaintStateIndex, Rgba, ScaledPixels, TileCoord,
    fast::{
        dependencies::RenderDependencies,
        layers::{reuse::KeptLayout, scene::LayerContent},
    },
};
use collections::FxHashMap;
use std::{ops::Range, rc::Rc};

/// The content of a scroll container as it was last painted into its layer.
#[allow(
    dead_code,
    reason = "the invalidation and input streams read the ranges, hovers and dependencies"
)]
#[derive(Default)]
pub(crate) struct LayerRecord {
    /// The painted content in content space (window space less
    /// `translation`).
    pub(crate) content: LayerContent,
    /// Bumps each time `content` is replaced.
    pub(crate) generation: u64,
    /// The part of the content painted, in window space at paint: the
    /// viewport and the overscan around it.
    pub(crate) painted_region: Bounds<Pixels>,
    /// The container's clip rect in window space at paint.
    pub(crate) viewport: Bounds<Pixels>,
    /// The scroll offset the content was painted at, snapped.
    pub(crate) scroll_offset: Point<Pixels>,
    /// The layer's translation at paint, in whole device pixels: window
    /// position = content position + translation.
    pub(crate) translation: Point<ScaledPixels>,
    /// What prepainting the content added to the frame.
    pub(crate) prepaint_range: Range<PrepaintStateIndex>,
    /// What painting the content added to the frame; its scene indices are
    /// the layer's own scene's.
    pub(crate) paint_range: Range<PaintIndex>,
    /// The hash of each tile of the painted region.
    pub(crate) tile_hashes: FxHashMap<TileCoord, u64>,
    /// The tiles `generation` changed.
    pub(crate) dirty_tiles: Vec<TileCoord>,
    /// The opaque colour under the viewport, which every tile is cleared with.
    pub(crate) background: Rgba,
    /// The hovers the content was painted by.
    pub(crate) hovers: Rc<[(HitboxId, bool)]>,
    /// What prepainting and painting the content read.
    pub(crate) dependencies: RenderDependencies,
    /// The views drawn inside the content, which render before it is
    /// recorded.
    pub(crate) views: Rc<[EntityId]>,
    /// Whether the content painted a path that other content draws over.
    /// Paths are never composited from tiles: their antialiasing pairs
    /// pixels in 2×2 quads, so a path moved by an odd number of device
    /// pixels would not match a direct draw (spec §5.6). Such content is
    /// drawn into the frame instead.
    pub(crate) has_paths: bool,
    /// The paths the content painted that nothing in it draws over, in
    /// content space, in drawing order: left out of `content`, and drawn
    /// into the frame over the layer's tiles wherever it is composited.
    pub(crate) paths: Rc<[Path<ScaledPixels>]>,
    /// How the views drawn inside the content were laid out when it was
    /// painted, by the views' ids: a frame that composites the layer lays
    /// them out again from these instead of rendering them (spec §6.3).
    pub(crate) view_layouts: Rc<FxHashMap<GlobalElementId, KeptLayout>>,
}
