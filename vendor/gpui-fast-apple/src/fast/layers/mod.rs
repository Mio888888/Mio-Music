// Modified by Longbridge for gpui-fast.
//! Scroll layer tiles on Metal: the textures a layer's content is rasterized
//! into, and the passes that draw them where the scene composites them.
//! The core side is `gpui::fast::layers`; see
//! docs/superpowers/specs/2026-09-30-scroll-layers-design.md, §5. The wgpu
//! renderer's counterpart is `gpui_wgpu`'s `fast::layers`.

pub(crate) mod composite;
pub(crate) mod raster;
pub(crate) mod tile_cache;

pub(crate) use tile_cache::TileCache;

#[cfg(test)]
mod tests;
