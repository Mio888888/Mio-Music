// Modified by Longbridge for gpui-fast.
//! Drawing scroll layer tiles where the scene composites them: polychrome
//! sprites whose texture id lies in the range no atlas allocates stand for
//! tiles, and are drawn with the polychrome sprite pipeline from the tiles'
//! own textures instead of an atlas texture.
//!
//! A tile sprite covers the tile's whole texture at whole device pixels, so
//! every fragment samples a texel center and copies the tile's pixel.

use std::{mem, ops::Range};

use gpui::{AtlasTextureId, DevicePixels, Size, TileId, decode_layer_tile, size};

use crate::metal_renderer::{InstanceBindings, MetalRenderer, SpriteInputIndex};

/// Forwarded to by `MetalRenderer::draw_polychrome_sprites`. Draws the batch
/// `sprites` if its texture is a layer's tiles, and returns whether it was:
/// otherwise upstream draws it from the atlas. A tile the cache does not hold
/// draws nothing; rasterizing every composited tile it lacks before the
/// frame is drawn prevents that.
pub(crate) fn draw_tiles(
    renderer: &MetalRenderer,
    texture_id: AtlasTextureId,
    sprites: &Range<usize>,
    instance_bindings: &InstanceBindings,
    viewport_size: Size<DevicePixels>,
    command_encoder: &metal::RenderCommandEncoderRef,
) -> bool {
    let Some((layer, _)) = decode_layer_tile(texture_id, TileId(0)) else {
        return false;
    };
    let cache = &renderer.fast_layers;
    let polychrome_sprites = &instance_bindings.polychrome_sprites;
    let mut pipeline_set = false;
    for (tile, run) in cache.tile_runs(sprites.clone()) {
        let Some(texture) = cache.texture(layer, tile) else {
            continue;
        };
        if !pipeline_set {
            pipeline_set = true;
            command_encoder.set_render_pipeline_state(&renderer.polychrome_sprites_pipeline_state);
            command_encoder.set_vertex_buffer(
                SpriteInputIndex::Vertices as u64,
                Some(&renderer.unit_vertices),
                0,
            );
            command_encoder.set_vertex_buffer(
                SpriteInputIndex::Sprites as u64,
                Some(&polychrome_sprites.buffer),
                polychrome_sprites.offset as u64,
            );
            command_encoder.set_vertex_bytes(
                SpriteInputIndex::ViewportSize as u64,
                mem::size_of_val(&viewport_size) as u64,
                &viewport_size as *const Size<DevicePixels> as *const _,
            );
            command_encoder.set_fragment_buffer(
                SpriteInputIndex::Sprites as u64,
                Some(&polychrome_sprites.buffer),
                polychrome_sprites.offset as u64,
            );
        }
        let texture_size = size(
            DevicePixels(texture.width() as i32),
            DevicePixels(texture.height() as i32),
        );
        command_encoder.set_vertex_bytes(
            SpriteInputIndex::AtlasTextureSize as u64,
            mem::size_of_val(&texture_size) as u64,
            &texture_size as *const Size<DevicePixels> as *const _,
        );
        command_encoder.set_fragment_texture(SpriteInputIndex::AtlasTexture as u64, Some(texture));
        command_encoder.draw_primitives_instanced_base_instance(
            metal::MTLPrimitiveType::Triangle,
            0,
            6,
            run.len() as u64,
            run.start as u64,
        );
    }
    true
}
