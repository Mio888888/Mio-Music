//! Mio's live backdrop extension. Captures and filters only on the GPU.
use anyhow::{Context, Result, ensure};
use foreign_types::{ForeignType, ForeignTypeRef};
use gpui::{DevicePixels, PaintBackdrop, Size};
use metal::{MTLPixelFormat, MTLStorageMode, MTLTextureUsage};
use objc2::{
    msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject},
};

#[repr(C)]
struct Uniforms {
    bounds: [f32; 4],
    clip: [f32; 4],
    radii: [f32; 4],
    region: [f32; 4],
    effects: [f32; 4],
    viewport: [f32; 4],
}
struct Targets {
    snapshot: metal::Texture,
    saturated: metal::Texture,
    blurred: metal::Texture,
}
pub(super) struct BackdropRenderer {
    device: metal::Device,
    prepare: metal::RenderPipelineState,
    composite: metal::RenderPipelineState,
    targets: Option<Targets>,
    gaussian: Option<(u32, Retained<AnyObject>)>,
}
impl BackdropRenderer {
    pub(super) fn new(device: &metal::DeviceRef, library: &metal::LibraryRef) -> Result<Self> {
        ensure!(
            metal::mps::mps_supports_device(device),
            "MPS unsupported on this GPU"
        );
        Ok(Self {
            device: device.to_owned(),
            prepare: pipeline(
                device,
                library,
                "mio_backdrop_prepare_vertex",
                "mio_backdrop_prepare_fragment",
            )?,
            composite: pipeline(
                device,
                library,
                "mio_backdrop_vertex",
                "mio_backdrop_fragment",
            )?,
            targets: None,
            gaussian: None,
        })
    }
    pub(super) fn apply(
        &mut self,
        backdrop: &PaintBackdrop,
        command: &metal::CommandBufferRef,
        target: &metal::TextureRef,
        viewport: Size<DevicePixels>,
    ) -> Result<()> {
        let width = target.width();
        let height = target.height();
        let b = backdrop.bounds;
        let clip = backdrop.content_mask.bounds;
        let visible = b.intersect(&clip);
        let sigma = backdrop.blur_radius;
        ensure!(sigma.is_finite() && sigma >= 0., "invalid backdrop radius");
        let padding = (sigma * 3.).ceil() + 2.;
        let x = (visible.origin.x.0 - padding)
            .floor()
            .clamp(0., width as f32) as u64;
        let y = (visible.origin.y.0 - padding)
            .floor()
            .clamp(0., height as f32) as u64;
        let right = (visible.right().0 + padding).ceil().clamp(0., width as f32) as u64;
        let bottom = (visible.bottom().0 + padding)
            .ceil()
            .clamp(0., height as f32) as u64;
        if right <= x || bottom <= y || visible.is_empty() {
            return Ok(());
        }
        let w = right - x;
        let h = bottom - y;
        let old = self.targets.as_ref();
        if old.is_none_or(|t| t.snapshot.width() < w || t.snapshot.height() < h) {
            // Grow in blocks; animated entry must not allocate on every frame.
            let capacity_w = ((w + 127) / 128 * 128)
                .max(old.map_or(0, |t| t.snapshot.width()))
                .min(width);
            let capacity_h = ((h + 127) / 128 * 128)
                .max(old.map_or(0, |t| t.snapshot.height()))
                .min(height);
            let make = || {
                let desc = metal::TextureDescriptor::new();
                desc.set_width(capacity_w);
                desc.set_height(capacity_h);
                desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
                desc.set_storage_mode(MTLStorageMode::Private);
                desc.set_usage(
                    MTLTextureUsage::ShaderRead
                        | MTLTextureUsage::ShaderWrite
                        | MTLTextureUsage::RenderTarget,
                );
                // metal-rs new_texture assumes non-null, while Metal permits
                // allocation failure. Adopt the +1 result only after checking.
                let raw: *mut AnyObject = unsafe {
                    msg_send![self.device.as_ptr().cast::<AnyObject>(),
                        newTextureWithDescriptor: desc.as_ptr().cast::<AnyObject>()]
                };
                ensure!(!raw.is_null(), "backdrop texture allocation failed");
                Ok::<_, anyhow::Error>(unsafe { metal::Texture::from_ptr(raw.cast()) })
            };
            self.targets = Some(Targets {
                snapshot: make()?,
                saturated: make()?,
                blurred: make()?,
            });
        }
        if sigma > 0.
            && self
                .gaussian
                .as_ref()
                .is_none_or(|(key, _)| *key != sigma.to_bits())
        {
            let class = AnyClass::get(c"MPSImageGaussianBlur")
                .context("MPSImageGaussianBlur unavailable")?;
            // Init consumes the allocation; Retained owns the initialized kernel.
            let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
            let kernel: Option<Retained<AnyObject>> = unsafe {
                msg_send![allocated, initWithDevice: self.device.as_ptr().cast::<AnyObject>(), sigma: sigma]
            };
            let kernel = kernel.context("Gaussian kernel initialization failed")?;
            unsafe {
                let _: () = msg_send![&*kernel, setEdgeMode: 1_usize];
            }
            self.gaussian = Some((sigma.to_bits(), kernel));
        }
        let t = self.targets.as_ref().unwrap();
        let blit = command.new_blit_command_encoder();
        blit.copy_from_texture(
            target,
            0,
            0,
            metal::MTLOrigin { x, y, z: 0 },
            metal::MTLSize {
                width: w,
                height: h,
                depth: 1,
            },
            &t.snapshot,
            0,
            0,
            metal::MTLOrigin { x: 0, y: 0, z: 0 },
        );
        blit.end_encoding();
        let radii = backdrop.corner_radii;
        let u = Uniforms {
            bounds: [b.origin.x.0, b.origin.y.0, b.size.width.0, b.size.height.0],
            clip: [
                clip.origin.x.0,
                clip.origin.y.0,
                clip.size.width.0,
                clip.size.height.0,
            ],
            radii: [
                radii.top_left.0,
                radii.top_right.0,
                radii.bottom_right.0,
                radii.bottom_left.0,
            ],
            region: [x as f32, y as f32, w as f32, h as f32],
            effects: [sigma, backdrop.saturation, backdrop.opacity, 0.],
            viewport: [
                viewport.width.0 as f32,
                viewport.height.0 as f32,
                t.snapshot.width() as f32,
                t.snapshot.height() as f32,
            ],
        };
        // Fill the entire capacity, clamping at the copied region's edge. Reused
        // spare pixels must never leak stale content into the Gaussian kernel.
        let prepared_size = gpui::size(
            DevicePixels(t.saturated.width() as i32),
            DevicePixels(t.saturated.height() as i32),
        );
        let encoder = super::metal_renderer::new_command_encoder_for_texture(
            command,
            &t.saturated,
            prepared_size,
            None,
        );
        draw(encoder, &self.prepare, &u, &t.snapshot, None);
        encoder.end_encoding();
        let filtered = if sigma > 0. {
            let kernel = &self.gaussian.as_ref().unwrap().1;
            unsafe {
                let _: () = msg_send![&**kernel,
                encodeToCommandBuffer: command.as_ptr().cast::<AnyObject>(),
                sourceTexture: t.saturated.as_ptr().cast::<AnyObject>(),
                destinationTexture: t.blurred.as_ptr().cast::<AnyObject>()];
            }
            &t.blurred
        } else {
            &t.saturated
        };
        let encoder =
            super::metal_renderer::new_command_encoder_for_texture(command, target, viewport, None);
        draw(encoder, &self.composite, &u, &t.snapshot, Some(filtered));
        encoder.end_encoding();
        Ok(())
    }
}
fn draw(
    encoder: &metal::RenderCommandEncoderRef,
    pipeline: &metal::RenderPipelineStateRef,
    u: &Uniforms,
    original: &metal::TextureRef,
    filtered: Option<&metal::TextureRef>,
) {
    encoder.set_render_pipeline_state(pipeline);
    encoder.set_vertex_bytes(
        0,
        std::mem::size_of::<Uniforms>() as u64,
        std::ptr::from_ref(u).cast(),
    );
    encoder.set_fragment_bytes(
        0,
        std::mem::size_of::<Uniforms>() as u64,
        std::ptr::from_ref(u).cast(),
    );
    encoder.set_fragment_texture(0, Some(original));
    encoder.set_fragment_texture(1, filtered);
    encoder.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, 6);
}
fn pipeline(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    vertex: &str,
    fragment: &str,
) -> Result<metal::RenderPipelineState> {
    let desc = metal::RenderPipelineDescriptor::new();
    let vertex = library
        .get_function(vertex, None)
        .map_err(anyhow::Error::msg)?;
    let fragment = library
        .get_function(fragment, None)
        .map_err(anyhow::Error::msg)?;
    desc.set_vertex_function(Some(&vertex));
    desc.set_fragment_function(Some(&fragment));
    let color = desc.color_attachments().object_at(0).unwrap();
    color.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    // Replace with mix(snapshot, filtered, coverage); blending would double
    // alpha-composite a transparent framebuffer into itself.
    color.set_blending_enabled(false);
    device
        .new_render_pipeline_state(&desc)
        .map_err(anyhow::Error::msg)
}
