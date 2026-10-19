// Modified by Longbridge for gpui-fast.
//! Rasterizing scroll layer tiles: each tile's part of its layer's content,
//! translated into the tile's space, drawn into the tile's texture cleared
//! to the layer's baked background, with the frame's pipelines and a
//! viewport the size of the tile.
//!
//! All position-dependent shading is relative to each primitive's own bounds,
//! and the tile is a whole number of device pixels from the content's
//! origin, so a tile holds exactly the pixels the content would have drawn
//! over the same background in the window.
//!
//! The tiles are encoded in a command buffer of their own, committed before
//! the frame's. Metal runs a queue's command buffers in the order they are
//! committed and orders the frame's reads of the tile textures after these
//! writes, and the frame's encoding stays upstream's.

use std::cell::Cell;
use std::ptr;

use anyhow::Result;
use block2::RcBlock;
use foreign_types::ForeignTypeRef;
use gpui::{
    DevicePixels, LayerKey, PrimitiveBatch, Rgba, Scene, TileCoord, decode_layer_tile, size,
};

use objc2::runtime::AnyObject;

use crate::metal_renderer::{
    InstanceBindings, InstanceBufferWriter, MetalRenderer, SurfaceBounds,
    new_command_encoder_for_texture,
};

/// The pixel format of tiles: the drawable's and every pipeline's.
const TILE_FORMAT: metal::MTLPixelFormat = metal::MTLPixelFormat::BGRA8Unorm;

/// Forwarded to by `MetalRenderer::render_frame` before it encodes `scene`:
/// rasterizes the layer tiles `scene` needs that the cache lacks, and notes
/// the tiles it composites for `composite::draw_tiles`.
pub(crate) fn rasterize_tiles(renderer: &mut MetalRenderer, scene: &Scene) {
    let cache = &mut renderer.fast_layers;
    if cache.take_failure() {
        cache.clear();
    }
    cache.note_composited(scene);
    let planned = cache.begin_frame(&scene.layers, composited_tiles(scene));
    if planned.is_empty() {
        return;
    }
    cache.prepare(&renderer.device, TILE_FORMAT, &scene.layers, &planned);
    if let Err(error) = encode(renderer, scene, &planned) {
        log::error!("failed to rasterize scroll layer tiles: {error:#}");
        // The tiles this frame was to rasterize were not.
        renderer.fast_layers.clear();
    }
}

/// The layer tiles `scene` composites, in the order it lists them.
fn composited_tiles(scene: &Scene) -> impl Iterator<Item = (LayerKey, TileCoord)> + '_ {
    let has_layers = !scene.layers.frames.is_empty();
    scene
        .polychrome_sprites
        .iter()
        .take_while(move |_| has_layers)
        .filter_map(|sprite| decode_layer_tile(sprite.tile.texture_id, sprite.tile.tile_id))
}

/// The clear colour a tile over `background` starts from: the bytes an
/// opaque quad of that colour writes to the frame's non-sRGB texture.
pub(crate) fn clear_color(background: Rgba) -> metal::MTLClearColor {
    metal::MTLClearColor::new(
        background.r as f64,
        background.g as f64,
        background.b as f64,
        background.a as f64,
    )
}

/// Encodes the passes that rasterize `planned` into their textures, in a
/// command buffer of their own, and commits it.
fn encode(
    renderer: &mut MetalRenderer,
    scene: &Scene,
    planned: &[(usize, TileCoord)],
) -> Result<()> {
    let mut writer = InstanceBufferWriter::new(
        &renderer.device,
        &renderer.instance_buffer_pool,
        renderer.is_unified_memory,
    );
    let command_queue = renderer.command_queue.clone();
    let command_buffer = command_queue.new_command_buffer();
    command_buffer.set_label("scroll_layer_tiles");

    let mut result = Ok(());
    for &(index, tile) in planned {
        let layer = &scene.layers.frames[index];
        let Some(texture) = renderer
            .fast_layers
            .texture(layer.key, tile)
            .map(|texture| texture.to_owned())
        else {
            debug_assert!(false, "a planned tile was not prepared");
            continue;
        };
        let tile_scene = layer.tile_scene(tile);
        let bindings = match write_instances(&tile_scene, &mut writer) {
            Ok(bindings) => bindings,
            Err(error) => {
                result = Err(error);
                break;
            }
        };
        let viewport_size = size(
            DevicePixels(layer.tile_size as i32),
            DevicePixels(layer.tile_size as i32),
        );
        let command_encoder = new_command_encoder_for_texture(
            command_buffer,
            &texture,
            viewport_size,
            Some(clear_color(layer.background)),
        );
        draw_tile_scene(
            renderer,
            &tile_scene,
            &bindings,
            viewport_size,
            command_encoder,
        );
        command_encoder.end_encoding();
        #[cfg(test)]
        {
            renderer.fast_layers.rasterized += 1;
        }
    }

    let instance_buffer = writer.finish();
    if let Err(error) = result {
        renderer
            .instance_buffer_pool
            .lock()
            .release(instance_buffer);
        return Err(error);
    }

    let instance_buffer_pool = renderer.instance_buffer_pool.clone();
    let failed = renderer.fast_layers.failure_flag();
    let instance_buffer = Cell::new(Some(instance_buffer));
    let block = RcBlock::new(move |command_buffer: ptr::NonNull<AnyObject>| {
        // SAFETY: Metal hands its completion handlers the command buffer that completed.
        let command_buffer =
            unsafe { metal::CommandBufferRef::from_ptr(command_buffer.as_ptr().cast()) };
        if command_buffer.status() == metal::MTLCommandBufferStatus::Error {
            failed.store(true, std::sync::atomic::Ordering::Release);
        }
        if let Some(instance_buffer) = instance_buffer.take() {
            instance_buffer_pool.lock().release(instance_buffer);
        }
    });
    // SAFETY: Both pointee types are opaque views of the same Objective-C block
    // pointer ABI, as upstream's `draw_primitives_to_texture` relies on too.
    unsafe {
        command_buffer.add_completed_handler(&*RcBlock::as_ptr(&block).cast());
    }
    command_buffer.commit();
    Ok(())
}

/// Writes a tile scene's instance data, as upstream's `write_instances` does
/// for a frame.
fn write_instances(scene: &Scene, writer: &mut InstanceBufferWriter) -> Result<InstanceBindings> {
    Ok(InstanceBindings {
        quads: writer.write(&scene.quads)?,
        shadows: writer.write(&scene.shadows)?,
        underlines: writer.write(&scene.underlines)?,
        monochrome_sprites: writer.write(&scene.monochrome_sprites)?,
        polychrome_sprites: writer.write(&scene.polychrome_sprites)?,
        surfaces: writer.write_iter(scene.surfaces.iter().map(|surface| SurfaceBounds {
            bounds: surface.bounds,
            content_mask: surface.content_mask,
        }))?,
    })
}

/// Draws a tile scene's batches with the renderer's pipelines, as upstream's
/// `draw_primitives_to_texture` draws a frame without paths.
fn draw_tile_scene(
    renderer: &mut MetalRenderer,
    scene: &Scene,
    bindings: &InstanceBindings,
    viewport_size: gpui::Size<DevicePixels>,
    command_encoder: &metal::RenderCommandEncoderRef,
) {
    for batch in scene.batches() {
        match batch {
            PrimitiveBatch::Shadows(range) => {
                renderer.draw_shadows(range, bindings, viewport_size, command_encoder)
            }
            PrimitiveBatch::Quads(range) => {
                renderer.draw_quads(range, bindings, viewport_size, command_encoder)
            }
            PrimitiveBatch::Underlines(range) => {
                renderer.draw_underlines(range, bindings, viewport_size, command_encoder)
            }
            PrimitiveBatch::MonochromeSprites { texture_id, range } => renderer
                .draw_monochrome_sprites(
                    texture_id,
                    range,
                    bindings,
                    viewport_size,
                    command_encoder,
                ),
            PrimitiveBatch::PolychromeSprites { texture_id, range } => renderer
                .draw_polychrome_sprites(
                    texture_id,
                    range,
                    bindings,
                    viewport_size,
                    command_encoder,
                ),
            PrimitiveBatch::Surfaces(range) => renderer.draw_surfaces(
                &scene.surfaces[range.clone()],
                range.start,
                bindings,
                viewport_size,
                command_encoder,
            ),
            // The core keeps paths out of layers: a path rasterized into a
            // tile and moved by an odd number of pixels antialiases
            // differently. Metal draws no subpixel sprites. A backdrop reads the frame,
            // so the core paints content holding one into the frame.
            PrimitiveBatch::Paths(range)
            | PrimitiveBatch::SubpixelSprites { range, .. }
            | PrimitiveBatch::Backdrops(range) => {
                debug_assert!(
                    range.is_empty(),
                    "a layer tile holds a path or subpixel sprite"
                );
            }
        }
    }
}
