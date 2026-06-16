//! ZenGPU 2D painter on the unified graphics API — `impl Gpu2dBackend for ZenGpuBackend`.
//!
//! Owns only ZenGPU device bring-up, pipeline creation, GPU instance-buffer
//! management, and the render-pass record loop. Texture upload/eviction is
//! called from the backend-agnostic `gpu2d` core; the slots returned by
//! `upload_image` are the global bindless indices the shaders see directly.

use std::collections::HashMap;
use std::sync::Arc;

use aurea_foundation::{AureaError, AureaResult};

use zengpu_hal::{
    Acquire, Bindings, ColorAttachment, DeviceRequest, FilterMode, Format, Frame, GpuDevice,
    GraphicsDevice, LoadOp, RenderCommands, RenderPassDesc, SamplerDesc, SamplerHandle, Scalar,
    Surface, TextureDesc, TextureHandle, TextureUsage, Viewport, ViewportScissor, WindowHandles,
};
use zengpu_vulkan::instance::VulkanInstance;
use zengpu_vulkan::{VulkanDevice, VulkanSurface};

use crate::batch::{CircleInstance, DrawRef, RectInstance};
use crate::gpu2d::{FramePlan, Gpu2dBackend, Gpu2dRenderer};

use super::buffer::GrowableBuffer;
use super::pipelines::{GradientInstance, ImageInstance, Pipelines, TextInstance};
use super::surface::create_surface;

// Guard that batch-layer rects/circles reinterpret to pipeline instances safely.
const _: () = assert!(
    std::mem::size_of::<crate::batch::RectInstance>()
        == std::mem::size_of::<super::pipelines::RectInstance>()
);
const _: () = assert!(
    std::mem::size_of::<crate::batch::CircleInstance>()
        == std::mem::size_of::<super::pipelines::CircleInstance>()
);

/// Shareable ZenGPU instance/device ownership for Aurea UI and engine rendering.
pub struct ZenGpuContext {
    instance: VulkanInstance,
    device: VulkanDevice,
}

impl ZenGpuContext {
    pub fn new() -> AureaResult<Self> {
        let instance = VulkanInstance::new_with_surface().map_err(gpu_err)?;
        let adapter = instance
            .request_vulkan_adapter()
            .ok_or(AureaError::ElementOperationFailed)?;
        let device = adapter
            .open_with_surface(DeviceRequest::default())
            .map_err(gpu_err)?;
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

/// ZenGPU device backend for `Gpu2dRenderer`, using the unified graphics API.
pub struct ZenGpuBackend {
    // Drop order: surface → pipelines/buffers → context (Arc may outlive).
    surface: VulkanSurface,
    context: Arc<ZenGpuContext>,
    pipelines: Pipelines,
    sampler: SamplerHandle,
    rect_buf: GrowableBuffer,
    circle_buf: GrowableBuffer,
    gradient_buf: GrowableBuffer,
    image_buf: GrowableBuffer,
    text_buf: GrowableBuffer,
    /// Maps shader slot (global bindless index) → TextureHandle for cleanup.
    slot_textures: HashMap<u32, TextureHandle>,
    external_images: Vec<ExternalImageDraw>,
}

/// Public renderer type alias — `Gpu2dRenderer` parameterized on `ZenGpuBackend`.
pub type ZenGpuRenderer = Gpu2dRenderer<ZenGpuBackend>;

impl ZenGpuRenderer {
    pub fn new(
        handles: &WindowHandles,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> AureaResult<Self> {
        Self::with_context(
            handles,
            Arc::new(ZenGpuContext::new()?),
            width,
            height,
            scale_factor,
        )
    }

    pub fn with_context(
        handles: &WindowHandles,
        context: Arc<ZenGpuContext>,
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

        let backend = ZenGpuBackend {
            surface,
            context,
            pipelines,
            sampler,
            rect_buf: GrowableBuffer::new(Default::default()),
            circle_buf: GrowableBuffer::new(Default::default()),
            gradient_buf: GrowableBuffer::new(Default::default()),
            image_buf: GrowableBuffer::new(Default::default()),
            text_buf: GrowableBuffer::new(Default::default()),
            slot_textures: HashMap::new(),
            external_images: Vec::new(),
        };
        Ok(Gpu2dRenderer::from_backend(backend, width, height, scale))
    }

    pub fn size(&self) -> (u32, u32) {
        self.backend().surface.size()
    }

    pub fn context(&self) -> &Arc<ZenGpuContext> {
        &self.backend().context
    }

    /// Draw a caller-owned GPU texture after the ordinary display list. The
    /// texture must be in `SHADER_READ_ONLY_OPTIMAL` (achieved via
    /// [`zengpu_hal::ColorAttachment::sample_after`] or an explicit barrier).
    pub fn draw_sampled_image(
        &mut self,
        texture: TextureHandle,
        dest: crate::types::Rect,
    ) -> AureaResult<()> {
        self.backend_mut().push_external_image(texture, dest)
    }

    /// Remove all caller-owned sampled images queued with [`draw_sampled_image`].
    pub fn clear_sampled_images(&mut self) -> AureaResult<()> {
        self.backend_mut().external_images.clear();
        Ok(())
    }
}

impl ZenGpuBackend {
    fn push_external_image(
        &mut self,
        texture: TextureHandle,
        dest: crate::types::Rect,
    ) -> AureaResult<()> {
        let device = self.context.device();
        let slot = device
            .bind_texture(texture, self.sampler)
            .ok_or(AureaError::RenderingFailed)?;
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

impl Gpu2dBackend for ZenGpuBackend {
    fn begin_frame(&mut self) -> AureaResult<()> {
        self.external_images.clear();
        Ok(())
    }

    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()> {
        self.surface
            .resize(physical_width, physical_height)
            .map_err(gpu_err)
    }

    fn upload_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> AureaResult<u32> {
        let device = self.context.device();
        let texture = device
            .create_texture(TextureDesc {
                width,
                height,
                format: Format::Rgba8Unorm,
                usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                samples: 1,
            })
            .map_err(gpu_err)?;
        if let Err(e) = device.upload_texture_data(texture, rgba) {
            device.destroy_texture(texture);
            return Err(gpu_err(e));
        }
        let slot = device
            .bind_texture(texture, self.sampler)
            .ok_or(AureaError::RenderingFailed)?;
        self.slot_textures.insert(slot, texture);
        Ok(slot)
    }

    fn evict_image(&mut self, shader_slot: u32) {
        if let Some(texture) = self.slot_textures.remove(&shader_slot) {
            self.context.device().destroy_texture(texture);
        }
    }

    fn supports_dual_source(&self) -> bool {
        self.context.device().supports_dual_source_blending()
    }

    fn present_frame(
        &mut self,
        plan: &FramePlan,
        rects: &[RectInstance],
        circles: &[CircleInstance],
    ) -> AureaResult<()> {
        let frame = match self.surface.acquire().map_err(gpu_err)? {
            Acquire::Frame(f) => f,
            Acquire::Skip => return Ok(()),
        };

        let vw = plan.viewport_width as f32;
        let vh = plan.viewport_height as f32;
        let device = self.context.device();

        // Build padded GPU instance arrays from the resolved plan entries.
        let vk_gradients: Vec<GradientInstance> = plan
            .gradients
            .iter()
            .map(|g| GradientInstance {
                rect: g.rect,
                a: g.a,
                b: g.b,
                slot: g.slot,
                _pad: [0; 3],
            })
            .collect();
        let mut vk_images: Vec<ImageInstance> = plan
            .images
            .iter()
            .map(|i| ImageInstance {
                rect: i.rect,
                uv: i.uv,
                tint: i.tint,
                slot: i.slot,
                _pad: [0; 3],
            })
            .collect();
        // Append external (engine-side) images after display-list images.
        let ext_image_base = vk_images.len() as u32;
        vk_images.extend(self.external_images.iter().map(|e| e.instance));
        let vk_texts: Vec<TextInstance> = plan
            .texts
            .iter()
            .map(|t| TextInstance {
                rect: t.rect,
                color: t.color,
                slot: t.slot,
                _pad: [0; 3],
            })
            .collect();

        // Upload instance buffers.
        let rect_handle = self
            .rect_buf
            .upload(device, as_bytes(rects))
            .map_err(gpu_err)?;
        let circle_handle = self
            .circle_buf
            .upload(device, as_bytes(circles))
            .map_err(gpu_err)?;
        let gradient_handle = self
            .gradient_buf
            .upload(device, as_bytes(&vk_gradients))
            .map_err(gpu_err)?;
        let image_handle = self
            .image_buf
            .upload(device, as_bytes(&vk_images))
            .map_err(gpu_err)?;
        let text_handle = self
            .text_buf
            .upload(device, as_bytes(&vk_texts))
            .map_err(gpu_err)?;

        // Record.
        let mut cmd = device.create_command_list().map_err(gpu_err)?;

        let load = match plan.clear {
            Some(c) => {
                LoadOp::clear_rgb(c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0)
            }
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
            viewport: Viewport {
                x: 0.0,
                y: 0.0,
                width: vw,
                height: vh,
                min_depth: 0.0,
                max_depth: 1.0,
            },
            scissor: None,
        });

        let viewport_scalars = [Scalar::F32(vw), Scalar::F32(vh)];
        let mut cur_kind: Option<DrawKind> = None;

        for draw_ref in &plan.order {
            match *draw_ref {
                DrawRef::Rect(idx) => {
                    if cur_kind != Some(DrawKind::Rect) {
                        cmd.set_pipeline(self.pipelines.rect);
                        if let Some(buf) = rect_handle {
                            cmd.set_vertex_buffer(0, buf);
                        }
                        cur_kind = Some(DrawKind::Rect);
                    }
                    cmd.bind(Bindings {
                        scalars: &viewport_scalars,
                        ..Default::default()
                    });
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
                    cmd.bind(Bindings {
                        scalars: &viewport_scalars,
                        ..Default::default()
                    });
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
                    let slot = vk_gradients.get(idx as usize).map(|g| g.slot).unwrap_or(0);
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
                    let slot = vk_images.get(idx as usize).map(|i| i.slot).unwrap_or(0);
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
                    let slot = vk_texts.get(idx as usize).map(|t| t.slot).unwrap_or(0);
                    cmd.bind(Bindings {
                        scalars: &viewport_scalars,
                        textures: std::slice::from_ref(&slot),
                        ..Default::default()
                    });
                    cmd.draw(0..6, idx..idx + 1);
                }
            }
        }

        // External (engine-side) images after display-list painter order.
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

impl Drop for ZenGpuBackend {
    fn drop(&mut self) {
        let device = self.context.device();
        let _ = device.wait_idle();
        for (_, texture) in self.slot_textures.drain() {
            device.destroy_texture(texture);
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrawKind {
    Rect,
    Circle,
    Gradient,
    Image,
    Text,
}

fn gpu_err(_e: zengpu_hal::GpuError) -> AureaError {
    AureaError::ElementOperationFailed
}

fn as_bytes<T: Copy>(slice: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice)) }
}
