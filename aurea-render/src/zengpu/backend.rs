//! ZenGPU 2D painter on the unified graphics API — `impl Gpu2dBackend for ZenGpuBackend`.
//!
//! Owns only ZenGPU device bring-up, pipeline creation, GPU instance-buffer
//! management, and the render-pass record loop. Texture upload/eviction is
//! called from the backend-agnostic `gpu2d` core; the slots returned by
//! `upload_image` are the global bindless indices the shaders see directly.

use std::collections::HashMap;
use std::mem::{size_of, size_of_val};
use std::slice::{from_raw_parts, from_ref};
use std::sync::Arc;

use aurea_foundation::{AureaError, AureaResult};

use zengpu_hal::{
    Acquire, Bindings, BufferHandle, ColorAttachment, DeviceRequest, FilterMode, Format, Frame,
    GpuDevice, GpuError, GraphicsDevice, LoadOp, PipelineHandle, Rect as HalRect, RenderCommands,
    RenderPassDesc, SamplerDesc, SamplerHandle, Scalar, Surface, TexDim, TextureDesc,
    TextureHandle, TextureUsage, Viewport, ViewportScissor, WindowHandles,
};
use zengpu_vulkan::instance::VulkanInstance;
use zengpu_vulkan::{DeviceContext, VulkanCommandList, VulkanDevice, VulkanSurface};

use crate::batch::{CircleInstance, DrawRef, RectInstance};
use crate::gpu2d::{FramePlan, Gpu2dBackend, Gpu2dRenderer};
use crate::numeric::f32_to_u32_clamped;
use crate::types::Rect;

use super::buffer::GrowableBuffer;
use super::pipelines::{GradientInstance, ImageInstance, Pipelines, TextInstance};
use super::surface::create_surface;

// Guard that batch-layer rects/circles reinterpret to pipeline instances safely.
const _: () = assert!(size_of::<RectInstance>() == size_of::<super::pipelines::RectInstance>());
const _: () = assert!(size_of::<CircleInstance>() == size_of::<super::pipelines::CircleInstance>());

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

    pub fn device_context(&self) -> DeviceContext {
        self.device.context()
    }
}

struct ExternalImageDraw {
    instance: ImageInstance,
}

/// This frame's instance-buffer handles, one per primitive kind. `None` when
/// that kind has zero instances this frame (its buffer wasn't (re)uploaded).
struct FrameBuffers {
    rect: Option<BufferHandle>,
    circle: Option<BufferHandle>,
    gradient: Option<BufferHandle>,
    image: Option<BufferHandle>,
    text: Option<BufferHandle>,
}

/// Length of the maximal run starting at `order[i]` (always ≥ 1) for which
/// `keep(element, offset)` holds, `offset` being the 1-based distance from
/// `i`. Used to coalesce contiguous same-kind (same-slot, for
/// gradient/image/text) painter-order runs into one instanced draw.
/// How many draws from `i` can be issued as one instanced call.
///
/// A run needs the same kind, contiguous instances, and the same clip: the clip
/// is a scissor set around the draw, so a change of clip ends the run.
fn coalesce_run(
    order: &[DrawRef],
    clips: &[Option<Rect>],
    i: usize,
    mut keep: impl FnMut(&DrawRef, u32) -> bool,
) -> u32 {
    let clip = clips.get(i).copied().flatten();
    let mut count = 1u32;
    while order
        .get(i + count as usize)
        .is_some_and(|r| keep(r, count))
        && clips.get(i + count as usize).copied().flatten() == clip
    {
        count += 1;
    }
    count
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
    gradient_instances: Vec<GradientInstance>,
    image_instances: Vec<ImageInstance>,
    text_instances: Vec<TextInstance>,
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
        let pw = f32_to_u32_clamped((width as f32 * scale).round()).max(1);
        let ph = f32_to_u32_clamped((height as f32 * scale).round()).max(1);

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
            gradient_instances: Vec::new(),
            image_instances: Vec::new(),
            text_instances: Vec::new(),
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
    pub fn draw_sampled_image(&mut self, texture: TextureHandle, dest: Rect) -> AureaResult<()> {
        self.backend_mut().push_external_image(texture, dest)
    }

    /// Remove all caller-owned sampled images queued with [`draw_sampled_image`].
    pub fn clear_sampled_images(&mut self) -> AureaResult<()> {
        self.backend_mut().external_images.clear();
        Ok(())
    }
}

impl ZenGpuBackend {
    fn gradient_slot(&self, idx: u32) -> u32 {
        self.gradient_instances
            .get(idx as usize)
            .map_or(0, |g| g.slot)
    }

    fn image_slot(&self, idx: u32) -> u32 {
        self.image_instances.get(idx as usize).map_or(0, |i| i.slot)
    }

    fn text_slot(&self, idx: u32) -> u32 {
        self.text_instances.get(idx as usize).map_or(0, |t| t.slot)
    }

    fn push_external_image(&mut self, texture: TextureHandle, dest: Rect) -> AureaResult<()> {
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

    /// Bind `pipeline`/`buffer`, push `scalars`+`textures`, and issue one
    /// instanced draw over `start..start + count`.
    fn draw_run(
        cmd: &mut VulkanCommandList,
        pipeline: PipelineHandle,
        buffer: Option<BufferHandle>,
        scalars: &[Scalar],
        textures: &[u32],
        start: u32,
        count: u32,
    ) {
        cmd.set_pipeline(pipeline);
        if let Some(buf) = buffer {
            cmd.set_vertex_buffer(0, buf);
        }
        cmd.bind(Bindings {
            scalars,
            textures,
            ..Default::default()
        });
        cmd.draw(0..6, start..start + count);
    }

    /// Record the display list's painter-ordered draws (see `present_frame`'s
    /// former doc comment on why coalescing contiguous runs is valid).
    /// Sets the scissor for the draws that follow.
    ///
    /// `None` restores the full viewport. The clip arrives in physical pixels
    /// with the same origin as the viewport, so it needs clamping but no
    /// conversion; an empty result would draw nothing, which is what a clip
    /// that excludes everything should do.
    fn set_scissor(cmd: &mut VulkanCommandList, viewport: (f32, f32), clip: Option<Rect>) {
        let (vw, vh) = viewport;
        let scissor = clip.map(|c| {
            let x = c.x.clamp(0.0, vw);
            let y = c.y.clamp(0.0, vh);
            HalRect {
                x,
                y,
                width: (c.x + c.width).clamp(0.0, vw) - x,
                height: (c.y + c.height).clamp(0.0, vh) - y,
            }
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
            scissor,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn record_ordered_draws(
        &self,
        cmd: &mut VulkanCommandList,
        order: &[DrawRef],
        clips: &[Option<Rect>],
        viewport: (f32, f32),
        buffers: &FrameBuffers,
        viewport_scalars: &[Scalar],
    ) {
        let mut i = 0;
        let mut active_clip: Option<Option<Rect>> = None;
        while i < order.len() {
            // The clip is a scissor around the draw. Set only when it changes,
            // so a run of draws sharing one costs a single state change.
            let clip = clips.get(i).copied().flatten();
            if active_clip != Some(clip) {
                Self::set_scissor(cmd, viewport, clip);
                active_clip = Some(clip);
            }

            match order[i] {
                DrawRef::Rect(start) => {
                    let count =
                        coalesce_run(order, clips, i, |r, n| *r == DrawRef::Rect(start + n));
                    Self::draw_run(
                        cmd,
                        self.pipelines.rect,
                        buffers.rect,
                        viewport_scalars,
                        &[],
                        start,
                        count,
                    );
                    i += count as usize;
                }
                DrawRef::Circle(start) => {
                    let count =
                        coalesce_run(order, clips, i, |r, n| *r == DrawRef::Circle(start + n));
                    Self::draw_run(
                        cmd,
                        self.pipelines.circle,
                        buffers.circle,
                        viewport_scalars,
                        &[],
                        start,
                        count,
                    );
                    i += count as usize;
                }
                DrawRef::Gradient(start) => {
                    let slot = self.gradient_slot(start);
                    let count = coalesce_run(order, clips, i, |r, n| {
                        *r == DrawRef::Gradient(start + n) && self.gradient_slot(start + n) == slot
                    });
                    Self::draw_run(
                        cmd,
                        self.pipelines.gradient,
                        buffers.gradient,
                        viewport_scalars,
                        from_ref(&slot),
                        start,
                        count,
                    );
                    i += count as usize;
                }
                DrawRef::Image(start) => {
                    let slot = self.image_slot(start);
                    let count = coalesce_run(order, clips, i, |r, n| {
                        *r == DrawRef::Image(start + n) && self.image_slot(start + n) == slot
                    });
                    Self::draw_run(
                        cmd,
                        self.pipelines.image,
                        buffers.image,
                        viewport_scalars,
                        from_ref(&slot),
                        start,
                        count,
                    );
                    i += count as usize;
                }
                DrawRef::Text(start) => {
                    let slot = self.text_slot(start);
                    let count = coalesce_run(order, clips, i, |r, n| {
                        *r == DrawRef::Text(start + n) && self.text_slot(start + n) == slot
                    });
                    Self::draw_run(
                        cmd,
                        self.pipelines.text,
                        buffers.text,
                        viewport_scalars,
                        from_ref(&slot),
                        start,
                        count,
                    );
                    i += count as usize;
                }
            }
        }
    }

    /// Record caller-owned images queued via `draw_sampled_image`, after the
    /// display-list painter order. They occupy a contiguous tail of
    /// `image_instances` starting at `ext_image_base`, so coalesce same-slot
    /// runs the same way as `record_ordered_draws`.
    fn record_external_images(
        &self,
        cmd: &mut VulkanCommandList,
        image_handle: Option<BufferHandle>,
        ext_image_base: u32,
        viewport_scalars: &[Scalar],
    ) {
        let ext_count = self.external_images.len();
        let mut j = 0;
        while j < ext_count {
            let slot = self.external_images[j].instance.slot;
            let mut run = 1usize;
            while j + run < ext_count && self.external_images[j + run].instance.slot == slot {
                run += 1;
            }
            let start =
                ext_image_base + u32::try_from(j).expect("external image index fits in u32");
            let count = u32::try_from(run).expect("external image run length fits in u32");
            Self::draw_run(
                cmd,
                self.pipelines.image,
                image_handle,
                viewport_scalars,
                from_ref(&slot),
                start,
                count,
            );
            j += run;
        }
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
                depth: 1,
                format: Format::Rgba8Unorm,
                usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                samples: 1,
                dimension: TexDim::D2,
                mip_levels: 1,
                array_layers: 1,
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
        self.gradient_instances.clear();
        self.gradient_instances
            .extend(plan.gradients.iter().map(|g| GradientInstance {
                rect: g.rect,
                a: g.a,
                b: g.b,
                slot: g.slot,
                _pad: [0; 3],
            }));
        self.image_instances.clear();
        self.image_instances
            .extend(plan.images.iter().map(|i| ImageInstance {
                rect: i.rect,
                uv: i.uv,
                tint: i.tint,
                slot: i.slot,
                _pad: [0; 3],
            }));
        // Append external (engine-side) images after display-list images.
        let ext_image_base =
            u32::try_from(self.image_instances.len()).expect("image instance count fits in u32");
        self.image_instances
            .extend(self.external_images.iter().map(|e| e.instance));
        self.text_instances.clear();
        self.text_instances
            .extend(plan.texts.iter().map(|t| TextInstance {
                rect: t.rect,
                color: t.color,
                slot: t.slot,
                _pad: [0; 3],
            }));

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
            .upload(device, as_bytes(&self.gradient_instances))
            .map_err(gpu_err)?;
        let image_handle = self
            .image_buf
            .upload(device, as_bytes(&self.image_instances))
            .map_err(gpu_err)?;
        let text_handle = self
            .text_buf
            .upload(device, as_bytes(&self.text_instances))
            .map_err(gpu_err)?;

        // Record.
        let mut cmd = device.create_command_list().map_err(gpu_err)?;

        let load = match plan.clear {
            Some(c) => LoadOp::clear_rgb(
                f32::from(c.r) / 255.0,
                f32::from(c.g) / 255.0,
                f32::from(c.b) / 255.0,
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

        // Painter order assigns each kind's instances contiguous indices in
        // submission order, so a maximal run of adjacent same-kind refs always
        // spans a contiguous instance range. Collapse each run into a single
        // instanced draw (`first_instance = start`, `instance_count = run len`)
        // instead of one draw per instance. Rect/circle carry no per-instance
        // shader state, so any run coalesces; gradient/image/text push their
        // texture slot per draw, so a run is split where the slot changes.
        let buffers = FrameBuffers {
            rect: rect_handle,
            circle: circle_handle,
            gradient: gradient_handle,
            image: image_handle,
            text: text_handle,
        };
        self.record_ordered_draws(
            &mut cmd,
            &plan.order,
            &plan.clips,
            (vw, vh),
            &buffers,
            &viewport_scalars,
        );
        self.record_external_images(&mut cmd, image_handle, ext_image_base, &viewport_scalars);

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

fn gpu_err(_e: GpuError) -> AureaError {
    AureaError::ElementOperationFailed
}

fn as_bytes<T: Copy>(slice: &[T]) -> &[u8] {
    unsafe { from_raw_parts(slice.as_ptr() as *const u8, size_of_val(slice)) }
}
