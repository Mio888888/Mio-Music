// Modified by Longbridge for gpui-fast.
//! Renderers for the extra GPUI surfaces of a composed window.
//!
//! Ported from zed-industries/zed#62379. A window that composes native views
//! between GPUI's base content and its overlays draws each GPUI surface on a
//! `CAMetalLayer` of its own. Every surface's renderer shares the window
//! renderer's device and sprite atlas, so glyphs and images rasterized for
//! one surface are there for the others, and the tiles a frame's scene refers
//! to are valid whichever surface draws them.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::metal_renderer::{Context, InstanceBufferPool, MetalRenderer, Renderer};

/// A renderer for an overlay surface above `base`: transparent, on its own
/// layer, sharing `base`'s device and sprite atlas.
pub(crate) fn new_overlay_renderer(context: Context, base: &Renderer) -> Renderer {
    new_sharing_atlas(base, context, true)
}

/// A renderer on a new `CAMetalLayer` that draws with `base`'s device and
/// sprite atlas.
fn new_sharing_atlas(
    base: &MetalRenderer,
    instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
    transparent: bool,
) -> MetalRenderer {
    let device = base.device.clone();
    let layer = new_layer(&device, transparent);
    let mut renderer =
        MetalRenderer::new_internal(device, Some(layer), !transparent, instance_buffer_pool);
    renderer.sprite_atlas = base.sprite_atlas().clone();
    renderer
}

/// A window's `CAMetalLayer`, set up as `MetalRenderer::new` sets up its own.
fn new_layer(device: &metal::Device, transparent: bool) -> metal::MetalLayer {
    let layer = metal::MetalLayer::new();
    MetalRenderer::configure_layer(&layer, device, transparent);
    layer
}
