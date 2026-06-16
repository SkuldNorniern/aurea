//! Unified-API ZenGPU 2D painter — `impl Gpu2dBackend for Painter`.
//!
//! At capstone this module (+ the rest of `zengpu_v2/`) replaces
//! `zengpu.rs` + `zengpu_surface.rs` entirely: `Painter` becomes
//! `ZenGpuBackend`, `PainterContext` becomes `ZenGpuContext`.

use std::sync::Arc;

use aurea_foundation::{AureaError, AureaResult};

use zengpu_hal::{
    Acquire, Bindings, ColorAttachment, DeviceRequest, FilterMode, Format, Frame,
    GpuDevice, GraphicsDevice, LoadOp, RenderCommands, RenderPassDesc, Scalar,
    SamplerDesc, SamplerHandle, Surface, Viewport, ViewportScissor, WindowHandles,
};
use zengpu_vulkan::{SampledImageView, VulkanDevice, VulkanSurface};
use zengpu_vulkan::instance::VulkanInstance;

use crate::batch::{DrawRef, GradientInstance as BatchGradient, ImageDraw, RenderBatches, TextDraw};
use crate::gpu2d::{Gpu2dBackend, Gpu2dRenderer};
use crate::types::Rect;

use super::buffer::GrowableBuffer;
use super::pipelines::{GradientInstance, ImageInstance, Pipelines, RectInstance, TextInstance};
use super::surface::create_surface;
use super::texture_cache::{gpu_err, TextureCache};

// Guard that batch-layer rects/circles reinterpret to pipelines instances safely.
const _: () = assert!(
    std::mem::size_of::<crate::batch::RectInstance>()
        == std::mem::size_of::<RectInstance>()
);
const _: () = assert!(
    std::mem::size_of::<crate::batch::CircleInstance>()
        == std::mem::size_of::<super::pipelines::CircleInstance>()
);

/// Shareable ZenGPU instance/device ownership for Aurea UI and engine rendering.
///
/// At capstone this becomes `ZenGpuContext` (same fields, same API).
pub struct PainterContext {
    instance: VulkanInstance,
    device: VulkanDevice,
}

impl PainterContext {
    pub fn new() -> AureaResult<Self> {
        let instance = VulkanInstance::new_with_surface().map_err(gpu_err)?;
        let adapter =
            instance.request_vulkan_adapter().ok_or(AureaError::ElementOperationFailed)?;
        let device = adapter.open_with_surface(DeviceRequest::default()).map_err(gpu_err)?;
        Ok(Self { instance, device })
    }

    pub fn instance(&self) -> &VulkanInstance {
        &self.instance
    }

    pub fn device(&self) -> &VulkanDevice {
        &self.device
    }

    pub fn device_context(&self) -> zengpu_vulkan::DeviceContext {
        self.device.context()
    }
}

struct ExternalImageDraw {
    instance: ImageInstance,
}

/// ZenGPU device backend for the shared `Gpu2dRenderer` core, using the
/// unified graphics API (no raw `ash`/`vk` calls).
///
/// At capstone this becomes `ZenGpuBackend`.
pub struct Painter {
    // Drop order: surface → pipelines/buffers → context (Arc may outlive).
    surface: VulkanSurface,
    context: Arc<PainterContext>,
    pipelines: Pipelines,
    color_format: Format,
    sampler: SamplerHandle,
    rect_buf: GrowableBuffer,
    circle_buf: GrowableBuffer,
    gradient_buf: GrowableBuffer,
    image_buf: GrowableBuffer,
    text_buf: GrowableBuffer,
    texture_cache: TextureCache,
    frame_counter: u64,
    vk_gradients: Vec<GradientInstance>,
    vk_images: Vec<ImageInstance>,
    vk_texts: Vec<TextInstance>,
    external_images: Vec<ExternalImageDraw>,
}

/// Public renderer type (staging name; becomes `ZenGpuRenderer` at capstone).
pub type PainterRenderer = Gpu2dRenderer<Painter>;

impl PainterRenderer {
    pub fn new(
        handles: &WindowHandles,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> AureaResult<Self> {
        Self::with_context(handles, Arc::new(PainterContext::new()?), width, height, scale_factor)
    }

    pub fn with_context(
        handles: &WindowHandles,
        context: Arc<PainterContext>,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> AureaResult<Self> {
        let scale = scale_factor.max(1.0);
        let pw = ((width as f32 * scale).round() as u32).max(1);
        let ph = ((height as f32 * scale).round() as u32).max(1);

        let color_format = Format::Bgra8Unorm;
        let surface = create_surface(context.device(), handles, pw, ph).map_err(gpu_err)?;
        let pipelines = Pipelines::new(context.device(), color_format).map_err(gpu_err)?;
        let sampler = context
            .device()
            .create_sampler(SamplerDesc {
                min_filter: FilterMode::Linear,
                mag_filter: FilterMode::Linear,
                ..SamplerDesc::default()
            })
            .map_err(gpu_err)?;

        let backend = Painter {
            surface,
            context,
            pipelines,
            color_format,
            sampler,
            rect_buf: GrowableBuffer::new(Default::default()),
            circle_buf: GrowableBuffer::new(Default::default()),
            gradient_buf: GrowableBuffer::new(Default::default()),
            image_buf: GrowableBuffer::new(Default::default()),
            text_buf: GrowableBuffer::new(Default::default()),
            texture_cache: TextureCache::new(),
            frame_counter: 0,
            vk_gradients: Vec::new(),
            vk_images: Vec::new(),
            vk_texts: Vec::new(),
            external_images: Vec::new(),
        };
        Ok(Gpu2dRenderer::from_backend(backend, width, height, scale))
    }

    pub fn size(&self) -> (u32, u32) {
        self.backend().surface.size()
    }

    pub fn context(&self) -> &Arc<PainterContext> {
        &self.backend().context
    }

    /// Draw a caller-owned GPU image after the ordinary display list. The image
    /// must come from the same [`PainterContext`] and already be in
    /// `SHADER_READ_ONLY_OPTIMAL`. Bound via `bind_raw_image_view` into the
    /// device's global bindless table (slot range 512-1023).
    pub fn draw_sampled_image(
        &mut self,
        image: SampledImageView<'_>,
        dest: Rect,
    ) -> AureaResult<()> {
        self.backend_mut().push_external_image(image, dest)
    }

    /// Remove all caller-owned sampled images queued with [`draw_sampled_image`].
    ///
    /// Call this before destroying their backing targets if no subsequent
    /// `begin_frame` will clear them automatically.
    pub fn clear_sampled_images(&mut self) -> AureaResult<()> {
        self.backend_mut().external_images.clear();
        Ok(())
    }
}

impl Painter {
    fn push_external_image(&mut self, image: SampledImageView<'_>, dest: Rect) -> AureaResult<()> {
        let device = self.context.device();
        let sampler_vk = device
            .sampler_vk(self.sampler)
            .ok_or(AureaError::ElementOperationFailed)?;
        let slot = device.bind_raw_image_view(image.raw(), sampler_vk);
        self.external_images.push(ExternalImageDraw {
            instance: ImageInstance {
                rect: [dest.x, dest.y, dest.width, dest.height],
                uv: [0.0, 0.0, 1.0, 1.0],
                tint: [1.0; 4],
                slot,
                _pad: [0; 3],
            },
        });
        Ok(())
    }
}

impl Gpu2dBackend for Painter {
    fn begin_frame(&mut self) -> AureaResult<()> {
        self.external_images.clear();
        Ok(())
    }

    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()> {
        self.surface.resize(physical_width, physical_height).map_err(gpu_err)
    }

    fn present(&mut self, batches: &RenderBatches) -> AureaResult<()> {
        let frame = match self.surface.acquire().map_err(gpu_err)? {
            Acquire::Frame(f) => f,
            Acquire::Skip => return Ok(()),
        };

        let (pw, ph) = self.surface.size();
        let vw = pw as f32;
        let vh = ph as f32;

        self.frame_counter += 1;
        let fc = self.frame_counter;
        let device = self.context.device();

        self.texture_cache.prune_dropped(device);

        // Resolve textured instance streams (texture upload + LUT cache lookup).
        resolve_gradients(
            &batches.gradients,
            device,
            self.sampler,
            fc,
            &mut self.texture_cache,
            &mut self.vk_gradients,
        )?;
        resolve_images(
            &batches.images,
            device,
            self.sampler,
            fc,
            &mut self.texture_cache,
            &mut self.vk_images,
        )?;
        // Append external (engine-side) images after display-list images.
        let ext_image_base = self.vk_images.len() as u32;
        self.vk_images.extend(self.external_images.iter().map(|e| e.instance));
        resolve_texts(
            &batches.texts,
            device,
            self.sampler,
            fc,
            &mut self.texture_cache,
            &mut self.vk_texts,
        )?;

        // Upload instance buffers (zero-copy for rects/circles via reinterpret).
        let rect_handle =
            self.rect_buf.upload(device, as_bytes(&batches.rects)).map_err(gpu_err)?;
        let circle_handle =
            self.circle_buf.upload(device, as_bytes(&batches.circles)).map_err(gpu_err)?;
        let gradient_handle =
            self.gradient_buf.upload(device, as_bytes(&self.vk_gradients)).map_err(gpu_err)?;
        let image_handle =
            self.image_buf.upload(device, as_bytes(&self.vk_images)).map_err(gpu_err)?;
        let text_handle =
            self.text_buf.upload(device, as_bytes(&self.vk_texts)).map_err(gpu_err)?;

        // Record.
        let mut cmd = device.create_command_list().map_err(gpu_err)?;

        let load = match batches.clear {
            Some(c) => LoadOp::clear_rgb(
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
            ),
            None => LoadOp::Load,
        };
        cmd.begin_render_pass(&RenderPassDesc {
            color: &[ColorAttachment {
                target: frame.target(),
                load,
                store: true,
                sample_after: false,
            }],
            depth: None,
        });
        cmd.set_viewport_scissor(ViewportScissor {
            viewport: Viewport { x: 0.0, y: 0.0, width: vw, height: vh, min_depth: 0.0, max_depth: 1.0 },
            scissor: None,
        });

        let viewport_scalars = [Scalar::F32(vw), Scalar::F32(vh)];

        // Track current pipeline to avoid redundant set_pipeline calls.
        let mut cur_kind: Option<DrawKind> = None;

        for draw_ref in &batches.order {
            match *draw_ref {
                DrawRef::Rect(idx) => {
                    if cur_kind != Some(DrawKind::Rect) {
                        cmd.set_pipeline(self.pipelines.rect);
                        if let Some(buf) = rect_handle {
                            cmd.set_vertex_buffer(0, buf);
                        }
                        cur_kind = Some(DrawKind::Rect);
                    }
                    cmd.bind(Bindings { scalars: &viewport_scalars, ..Default::default() });
                    cmd.draw(0..6, idx..idx + 1);
                }
                DrawRef::Circle(idx) => {
                    if cur_kind != Some(DrawKind::Circle) {
                        cmd.set_pipeline(self.pipelines.circle);
                        if let Some(buf) = circle_handle {
                            cmd.set_vertex_buffer(0, buf);
                        }
                        cur_kind = Some(DrawKind::Circle);
                    }
                    cmd.bind(Bindings { scalars: &viewport_scalars, ..Default::default() });
                    cmd.draw(0..6, idx..idx + 1);
                }
                DrawRef::Gradient(idx) => {
                    if cur_kind != Some(DrawKind::Gradient) {
                        cmd.set_pipeline(self.pipelines.gradient);
                        if let Some(buf) = gradient_handle {
                            cmd.set_vertex_buffer(0, buf);
                        }
                        cur_kind = Some(DrawKind::Gradient);
                    }
                    let slot = self.vk_gradients.get(idx as usize).map(|g| g.slot).unwrap_or(0);
                    cmd.bind(Bindings {
                        scalars: &viewport_scalars,
                        textures: std::slice::from_ref(&slot),
                        ..Default::default()
                    });
                    cmd.draw(0..6, idx..idx + 1);
                }
                DrawRef::Image(idx) => {
                    if cur_kind != Some(DrawKind::Image) {
                        cmd.set_pipeline(self.pipelines.image);
                        if let Some(buf) = image_handle {
                            cmd.set_vertex_buffer(0, buf);
                        }
                        cur_kind = Some(DrawKind::Image);
                    }
                    let slot = self.vk_images.get(idx as usize).map(|i| i.slot).unwrap_or(0);
                    cmd.bind(Bindings {
                        scalars: &viewport_scalars,
                        textures: std::slice::from_ref(&slot),
                        ..Default::default()
                    });
                    cmd.draw(0..6, idx..idx + 1);
                }
                DrawRef::Text(idx) => {
                    if cur_kind != Some(DrawKind::Text) {
                        cmd.set_pipeline(self.pipelines.text);
                        if let Some(buf) = text_handle {
                            cmd.set_vertex_buffer(0, buf);
                        }
                        cur_kind = Some(DrawKind::Text);
                    }
                    let slot = self.vk_texts.get(idx as usize).map(|t| t.slot).unwrap_or(0);
                    cmd.bind(Bindings {
                        scalars: &viewport_scalars,
                        textures: std::slice::from_ref(&slot),
                        ..Default::default()
                    });
                    cmd.draw(0..6, idx..idx + 1);
                }
            }
        }

        // External (engine-side) images after the display-list painter order.
        for (i, ext) in self.external_images.iter().enumerate() {
            let idx = ext_image_base + i as u32;
            if cur_kind != Some(DrawKind::Image) {
                cmd.set_pipeline(self.pipelines.image);
                if let Some(buf) = image_handle {
                    cmd.set_vertex_buffer(0, buf);
                }
                cur_kind = Some(DrawKind::Image);
            }
            let slot = ext.instance.slot;
            cmd.bind(Bindings {
                scalars: &viewport_scalars,
                textures: std::slice::from_ref(&slot),
                ..Default::default()
            });
            cmd.draw(0..6, idx..idx + 1);
        }

        cmd.end_render_pass();
        self.surface.present(frame, cmd).map_err(gpu_err)
    }
}

impl Drop for Painter {
    fn drop(&mut self) {
        let device = self.context.device();
        let _ = device.wait_idle();
        self.texture_cache.drain(device);
        self.rect_buf.destroy(device);
        self.circle_buf.destroy(device);
        self.gradient_buf.destroy(device);
        self.image_buf.destroy(device);
        self.text_buf.destroy(device);
        device.destroy_pipeline(self.pipelines.rect);
        device.destroy_pipeline(self.pipelines.circle);
        device.destroy_pipeline(self.pipelines.gradient);
        device.destroy_pipeline(self.pipelines.image);
        device.destroy_pipeline(self.pipelines.text);
        device.destroy_sampler(self.sampler);
    }
}

// ── Draw-kind enum for pipeline-switch tracking ───────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrawKind {
    Rect,
    Circle,
    Gradient,
    Image,
    Text,
}

// ── Texture-slot resolve helpers ──────────────────────────────────────────────

fn resolve_gradients(
    gradients: &[BatchGradient],
    device: &VulkanDevice,
    sampler: SamplerHandle,
    frame: u64,
    cache: &mut TextureCache,
    out: &mut Vec<GradientInstance>,
) -> AureaResult<()> {
    out.clear();
    for g in gradients {
        let slot = cache.resolve(&g.lut, 256, 1, device, sampler, frame)?;
        out.push(GradientInstance { rect: g.rect, a: g.a, b: g.b, slot, _pad: [0; 3] });
    }
    Ok(())
}

fn resolve_images(
    images: &[ImageDraw],
    device: &VulkanDevice,
    sampler: SamplerHandle,
    frame: u64,
    cache: &mut TextureCache,
    out: &mut Vec<ImageInstance>,
) -> AureaResult<()> {
    out.clear();
    for draw in images {
        let (iw, ih) = (draw.image.width, draw.image.height);
        if iw == 0 || ih == 0 {
            continue;
        }
        let expected = (iw as usize).saturating_mul(ih as usize).saturating_mul(4);
        if draw.image.data.len() != expected {
            return Err(AureaError::RenderingFailed);
        }
        let slot = cache.resolve(&draw.image.data, iw, ih, device, sampler, frame)?;
        let (iwf, ihf) = (iw as f32, ih as f32);
        out.push(ImageInstance {
            rect: [draw.dest.x, draw.dest.y, draw.dest.width, draw.dest.height],
            uv: [
                draw.src.x / iwf,
                draw.src.y / ihf,
                (draw.src.x + draw.src.width) / iwf,
                (draw.src.y + draw.src.height) / ihf,
            ],
            tint: [
                draw.tint.r as f32 / 255.0,
                draw.tint.g as f32 / 255.0,
                draw.tint.b as f32 / 255.0,
                draw.tint.a as f32 / 255.0,
            ],
            slot,
            _pad: [0; 3],
        });
    }
    Ok(())
}

fn resolve_texts(
    texts: &[TextDraw],
    device: &VulkanDevice,
    sampler: SamplerHandle,
    frame: u64,
    cache: &mut TextureCache,
    out: &mut Vec<TextInstance>,
) -> AureaResult<()> {
    out.clear();
    for text in texts {
        let w = text.rect.width as u32;
        let h = text.rect.height as u32;
        if w == 0 || h == 0 {
            continue;
        }
        let slot = cache.resolve(&text.mask, w, h, device, sampler, frame)?;
        out.push(TextInstance {
            rect: [text.rect.x, text.rect.y, text.rect.width, text.rect.height],
            color: [
                text.color.r as f32 / 255.0,
                text.color.g as f32 / 255.0,
                text.color.b as f32 / 255.0,
                text.color.a as f32 / 255.0,
            ],
            slot,
            _pad: [0; 3],
        });
    }
    Ok(())
}

/// Reinterpret a slice of `Copy` elements as a byte slice (for `write_buffer`).
fn as_bytes<T: Copy>(slice: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice)) }
}
