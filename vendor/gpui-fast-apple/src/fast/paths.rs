// Modified by Longbridge for gpui-fast.
//! A frame whose path batches are rasterized in groups, with fewer render
//! command encoders.
//!
//! Upstream's `draw_primitives_to_texture` gives every path batch two render
//! command encoders of its own: it ends the main encoder, rasterizes the
//! batch's paths into the cleared intermediate texture in a new encoder, and
//! begins a new main encoder to composite them. A frame with six path batches
//! (six sparklines) thus encodes thirteen render passes. Every encoder costs
//! the CPU a render pass descriptor, an encoder and its viewport, and on Apple
//! GPUs, which render in tiles, every restarted main pass stores the whole
//! drawable to memory and loads it back.
//!
//! Here, as in `gpui_wgpu`'s `fast::frame`, consecutive path batches that don't
//! come near each other share one rasterization pass, and the first group's
//! pass runs before the main pass begins, so a frame whose path batches don't
//! overlap is encoded in two passes. The vertices of every path in the frame
//! are written to the instance buffer once, and each group draws its range of
//! them. Each batch's pixels in the intermediate texture are then the same as
//! if it had been rasterized alone into the cleared texture, so the image is
//! the same as upstream's.

use std::{mem, ops::Range};

use anyhow::{Context as _, Result};
use gpui::{Bounds, DevicePixels, PrimitiveBatch, ScaledPixels, Scene, Size};

use crate::metal_renderer::{
    InstanceBinding, InstanceBindings, InstanceBufferWriter, MetalRenderer,
    PathRasterizationInputIndex, PathRasterizationVertex, new_command_encoder_for_texture,
};

/// How far apart, in device pixels, path batches must be to share the
/// intermediate texture. Rasterization is clipped to each path's bounds, and
/// the intermediate texture is sampled with linear filtering at texel
/// centers, so a pixel is plenty; two leave room for rounding.
const PATH_BATCH_MARGIN: f32 = 2.;

/// Past this many path batches in a group, checking that a batch overlaps
/// none of them costs more than the render passes it would save.
const MAX_GROUP_PATH_BATCHES: usize = 64;

/// A non-empty `PrimitiveBatch::Paths`: its paths, its vertices in the frame's
/// path vertices, and the bounds everything it draws stays within.
struct PathBatch {
    paths: Range<usize>,
    vertices: Range<u64>,
    bounds: Bounds<ScaledPixels>,
    /// For the first batch of a group, the vertices of the whole group,
    /// rasterized together before the batch is drawn.
    group: Option<Range<u64>>,
}

/// Forwarded to by `MetalRenderer::draw_primitives_to_texture`. Encodes the
/// frame and returns its command buffer, or returns `None` for upstream to
/// encode a frame without paths, which has no pass to save.
pub(crate) fn draw_primitives_to_texture(
    renderer: &mut MetalRenderer,
    scene: &Scene,
    instance_bindings: &InstanceBindings,
    writer: &mut InstanceBufferWriter,
    texture: &metal::TextureRef,
    viewport_size: Size<DevicePixels>,
) -> Result<Option<metal::CommandBuffer>> {
    // A backdrop ends and restarts the main pass between the batches it
    // separates; that frame is encoded by the renderer's own loop.
    if scene.paths.is_empty() || !scene.backdrops.is_empty() {
        return Ok(None);
    }
    let path_batches = plan_paths(scene);
    let vertices = write_vertices(scene, &path_batches, writer)?;

    let command_queue = renderer.command_queue.clone();
    let command_buffer = command_queue.new_command_buffer();
    let alpha = if renderer.opaque { 1. } else { 0. };

    // The first group is rasterized before the main pass, which saves ending
    // the main pass for it.
    let mut rasterized = match path_batches.first() {
        Some(first) => rasterize_group(
            renderer,
            vertices.as_ref(),
            first.group.clone(),
            viewport_size,
            command_buffer,
        )?,
        None => false,
    };

    let mut command_encoder = new_command_encoder_for_texture(
        command_buffer,
        texture,
        viewport_size,
        Some(metal::MTLClearColor::new(0., 0., 0., alpha)),
    );
    let mut path_batches = path_batches.iter().enumerate();

    for batch in scene.batches() {
        match batch {
            PrimitiveBatch::Shadows(range) => {
                renderer.draw_shadows(range, instance_bindings, viewport_size, command_encoder)
            }
            PrimitiveBatch::Quads(range) => {
                renderer.draw_quads(range, instance_bindings, viewport_size, command_encoder)
            }
            PrimitiveBatch::Paths(range) => {
                if range.is_empty() {
                    continue;
                }
                let Some((index, path_batch)) = path_batches.next() else {
                    continue;
                };
                if index > 0 && path_batch.group.is_some() {
                    command_encoder.end_encoding();
                    rasterized = rasterize_group(
                        renderer,
                        vertices.as_ref(),
                        path_batch.group.clone(),
                        viewport_size,
                        command_buffer,
                    )?;
                    command_encoder = new_command_encoder_for_texture(
                        command_buffer,
                        texture,
                        viewport_size,
                        None,
                    );
                }
                // A batch without vertices left its bounds in the cleared
                // texture transparent: compositing them draws nothing.
                if !rasterized || path_batch.vertices.is_empty() {
                    continue;
                }
                if let Err(error) = renderer.draw_paths_from_intermediate(
                    &scene.paths[range],
                    writer,
                    viewport_size,
                    command_encoder,
                ) {
                    command_encoder.end_encoding();
                    return Err(error);
                }
            }
            PrimitiveBatch::Underlines(range) => {
                renderer.draw_underlines(range, instance_bindings, viewport_size, command_encoder)
            }
            PrimitiveBatch::MonochromeSprites { texture_id, range } => renderer
                .draw_monochrome_sprites(
                    texture_id,
                    range,
                    instance_bindings,
                    viewport_size,
                    command_encoder,
                ),
            PrimitiveBatch::PolychromeSprites { texture_id, range } => renderer
                .draw_polychrome_sprites(
                    texture_id,
                    range,
                    instance_bindings,
                    viewport_size,
                    command_encoder,
                ),
            PrimitiveBatch::Surfaces(range) => renderer.draw_surfaces(
                &scene.surfaces[range.clone()],
                range.start,
                instance_bindings,
                viewport_size,
                command_encoder,
            ),
            PrimitiveBatch::SubpixelSprites { .. } => unreachable!(),
            PrimitiveBatch::Backdrops(_) => unreachable!("frames with backdrops are not encoded here"),
        }
    }

    command_encoder.end_encoding();

    Ok(Some(command_buffer.to_owned()))
}

/// Lays out every path batch's rasterization vertices, in the order of the
/// scene's batches, and groups the batches.
fn plan_paths(scene: &Scene) -> Vec<PathBatch> {
    let mut path_batches: Vec<PathBatch> = Vec::new();
    let mut vertex_count = 0u64;
    let mut group_start = 0;
    for batch in scene.batches() {
        let PrimitiveBatch::Paths(range) = batch else {
            continue;
        };
        let paths = &scene.paths[range.clone()];
        let Some(first_path) = paths.first() else {
            continue;
        };

        let first_vertex = vertex_count;
        let mut union = first_path.clipped_bounds();
        for path in paths {
            union = union.union(&path.clipped_bounds());
            vertex_count += path.vertices.len() as u64;
        }

        let near = union.dilate(ScaledPixels(PATH_BATCH_MARGIN));
        let group = &path_batches[group_start..];
        if group.is_empty()
            || group.len() >= MAX_GROUP_PATH_BATCHES
            || group.iter().any(|batch| near.intersects(&batch.bounds))
        {
            close_group(&mut path_batches[group_start..]);
            group_start = path_batches.len();
        }
        path_batches.push(PathBatch {
            paths: range,
            vertices: first_vertex..vertex_count,
            bounds: union,
            group: None,
        });
    }
    close_group(&mut path_batches[group_start..]);
    path_batches
}

/// Gives the first batch of `group` the vertices of all of it.
fn close_group(group: &mut [PathBatch]) {
    if let Some(end) = group.last().map(|last| last.vertices.end)
        && let Some(first) = group.first_mut()
    {
        first.group = Some(first.vertices.start..end);
    }
}

/// Writes the rasterization vertices of every planned batch into the instance
/// buffer in one allocation, as upstream's `draw_paths_to_intermediate` builds
/// them for each batch. Returns `None` when no path has vertices.
fn write_vertices(
    scene: &Scene,
    path_batches: &[PathBatch],
    writer: &mut InstanceBufferWriter,
) -> Result<Option<InstanceBinding>> {
    let count = path_batches.last().map_or(0, |last| last.vertices.end) as usize;
    if count == 0 {
        return Ok(None);
    }
    let vertices = path_batches
        .iter()
        .flat_map(|batch| &scene.paths[batch.paths.clone()])
        .flat_map(|path| {
            let bounds = path.bounds.intersect(&path.content_mask.bounds);
            path.vertices.iter().map(move |v| PathRasterizationVertex {
                xy_position: v.xy_position,
                st_position: v.st_position,
                color: path.color,
                bounds,
            })
        });
    // The plan counted these same vertices, so the iterator yields exactly
    // `count` of them and every reserved slot is written.
    let binding = writer.write_iter(Counted {
        inner: vertices,
        remaining: count,
    })?;
    Ok(Some(binding))
}

/// An iterator whose length is known ahead, which `write_iter` needs to
/// reserve its slots.
struct Counted<I> {
    inner: I,
    remaining: usize,
}

impl<I: Iterator> Iterator for Counted<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        let item = self.inner.next()?;
        self.remaining = self.remaining.saturating_sub(1);
        Some(item)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<I: Iterator> ExactSizeIterator for Counted<I> {}

/// Rasterizes a group's range of the frame's path vertices into the cleared
/// intermediate texture, in an encoder of its own, as upstream's
/// `draw_paths_to_intermediate` does for a batch. Returns false if there is
/// nothing to rasterize.
fn rasterize_group(
    renderer: &MetalRenderer,
    vertices: Option<&InstanceBinding>,
    group: Option<Range<u64>>,
    viewport_size: Size<DevicePixels>,
    command_buffer: &metal::CommandBufferRef,
) -> Result<bool> {
    let (Some(vertices), Some(group)) = (vertices, group.filter(|group| !group.is_empty())) else {
        return Ok(false);
    };
    let intermediate_texture = renderer
        .path_intermediate_texture
        .as_ref()
        .context("missing path intermediate texture")?;

    let render_pass_descriptor = metal::RenderPassDescriptor::new();
    let color_attachment = render_pass_descriptor
        .color_attachments()
        .object_at(0)
        .context("render pass has no color attachment")?;
    color_attachment.set_load_action(metal::MTLLoadAction::Clear);
    color_attachment.set_clear_color(metal::MTLClearColor::new(0., 0., 0., 0.));

    if let Some(msaa_texture) = &renderer.path_intermediate_msaa_texture {
        color_attachment.set_texture(Some(msaa_texture));
        color_attachment.set_resolve_texture(Some(intermediate_texture));
        color_attachment.set_store_action(metal::MTLStoreAction::MultisampleResolve);
    } else {
        color_attachment.set_texture(Some(intermediate_texture));
        color_attachment.set_store_action(metal::MTLStoreAction::Store);
    }

    let command_encoder = command_buffer.new_render_command_encoder(render_pass_descriptor);
    command_encoder.set_render_pipeline_state(&renderer.paths_rasterization_pipeline_state);
    command_encoder.set_vertex_buffer(
        PathRasterizationInputIndex::Vertices as u64,
        Some(&vertices.buffer),
        vertices.offset as u64,
    );
    command_encoder.set_vertex_bytes(
        PathRasterizationInputIndex::ViewportSize as u64,
        mem::size_of_val(&viewport_size) as u64,
        &viewport_size as *const Size<DevicePixels> as *const _,
    );
    command_encoder.set_fragment_buffer(
        PathRasterizationInputIndex::Vertices as u64,
        Some(&vertices.buffer),
        vertices.offset as u64,
    );
    // The shaders index the vertices by `vertex_id`, which counts from the
    // group's first vertex.
    command_encoder.draw_primitives(
        metal::MTLPrimitiveType::Triangle,
        group.start,
        group.end - group.start,
    );
    command_encoder.end_encoding();
    Ok(true)
}
