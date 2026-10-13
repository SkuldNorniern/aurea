//! ZenGPU 2D painter on the unified graphics API — `impl Gpu2dBackend for ZenGpuBackend`.
//!
//! Owns only ZenGPU device bring-up, pipeline creation, GPU instance-buffer
//! management, and the render-pass record loop. Texture upload/eviction is
//! called from the backend-agnostic `gpu2d` core; the slots returned by
//! `upload_image` are the global bindless indices the shaders see directly.

use std::collections::HashMap;
use std::mem::{size_of, size_of_val};
use std::ops::Deref;
use std::slice::{from_raw_parts, from_ref};
use std::sync::{Arc, Mutex, Weak};

use aurea_foundation::{AureaError, AureaResult};

#[cfg(not(target_os = "macos"))]
use zengpu::DeviceContext;
use zengpu::native::{
    NativeAdapter, NativeCommandList, NativeDevice, NativeInstance, NativeSurface,
};
use zengpu_hal::{
    Acquire, Bindings, BufferHandle, ColorAttachment, DeviceRequest, DeviceType, FilterMode,
    Format, Frame, GpuAdapter, GpuDevice, GpuError, GraphicsDevice, LoadOp, PipelineHandle,
    Rect as HalRect, RenderCommands, RenderPassDesc, SamplerDesc, SamplerHandle, Scalar, Surface,
    TexDim, TextureDesc, TextureHandle, TextureUsage, Viewport, ViewportScissor, WindowHandles,
};

use crate::capabilities::GpuFeatures;
use crate::gpu::batch::{DrawRef, RectInstance, ShapeInstance};
use crate::gpu::{FramePlan, Gpu2dBackend, Gpu2dRenderer};
use crate::numeric::f32_to_u32_clamped;
use crate::renderer::FrameOutcome;
use crate::types::{GpuTexture, Rect};

use super::buffer::GrowableBuffer;
use super::pipelines::{GradientInstance, ImageInstance, Pipelines, TextInstance};
use super::surface::create_surface;

// Guard that batch-layer rects/shapes reinterpret to pipeline instances safely.
const _: () = assert!(size_of::<RectInstance>() == size_of::<super::pipelines::RectInstance>());
const _: () = assert!(size_of::<ShapeInstance>() == size_of::<super::pipelines::ShapeInstance>());

/// Shareable ZenGPU instance/device ownership for Aurea UI and engine rendering.
///
/// After a device loss, a frame ends in [`FrameOutcome::Recreate`]. Every
/// renderer on the context goes, then the context: drop each renderer
/// first, since a window takes one surface at a time. Then make a context
/// with [`ZenGpuContext::reopen`], on the same GPU, and the renderers again
/// with `create_zengpu_2d_with_context`, and upload the app's textures
/// again from its own copies.
pub struct ZenGpuContext {
    pipelines: Mutex<Weak<SharedPipelines>>,
    instance: NativeInstance,
    device: NativeDevice,
    adapter: [u8; 16],
}

/// A GPU a context can open, for a "which GPU" setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuChoice {
    pub name: String,
    /// The same for this GPU and driver from one run to the next: what to
    /// save, and give [`ZenGpuContext::on_adapter`].
    pub uuid: [u8; 16],
    /// A separate card rather than one built into the CPU.
    pub discrete: bool,
}

impl ZenGpuContext {
    /// A context on the best GPU: a discrete one if there is one.
    pub fn new() -> AureaResult<Self> {
        let instance = NativeInstance::new_with_surface().map_err(gpu_err)?;
        let adapter = instance
            .request_graphics_adapter()
            .ok_or(AureaError::ElementOperationFailed)?;
        Self::open(instance, &adapter)
    }

    /// The GPUs there are to choose from.
    pub fn adapters() -> AureaResult<Vec<GpuChoice>> {
        let instance = NativeInstance::new_with_surface().map_err(gpu_err)?;
        Ok(instance
            .graphics_adapters()
            .iter()
            .map(|a| GpuChoice {
                name: a.info().name.clone(),
                uuid: a.uuid(),
                discrete: a.info().device_type == DeviceType::Discrete,
            })
            .collect())
    }

    /// A context on the GPU with this [`GpuChoice::uuid`]. Fails when it
    /// is gone, unplugged or its driver changed.
    pub fn on_adapter(uuid: [u8; 16]) -> AureaResult<Self> {
        let instance = NativeInstance::new_with_surface().map_err(gpu_err)?;
        let adapter = instance
            .graphics_adapter_by_uuid(uuid)
            .ok_or(AureaError::ElementOperationFailed)?;
        Self::open(instance, &adapter)
    }

    /// A new context on this one's GPU, for after a device loss.
    pub fn reopen(&self) -> AureaResult<Self> {
        Self::on_adapter(self.adapter)
    }

    /// Whether the device is gone, after a driver reset or a GPU removal.
    /// Nothing made on it works again; [`Self::reopen`] makes a new one.
    pub fn is_lost(&self) -> bool {
        self.device.is_lost()
    }

    /// Which GPU this context is on.
    pub fn adapter_uuid(&self) -> [u8; 16] {
        self.adapter
    }

    fn open(instance: NativeInstance, adapter: &NativeAdapter) -> AureaResult<Self> {
        let device = adapter
            .open_graphics(DeviceRequest::default())
            .map_err(gpu_err)?;
        Ok(Self {
            pipelines: Mutex::new(Weak::new()),
            adapter: adapter.uuid(),
            instance,
            device,
        })
    }

    pub fn instance(&self) -> &NativeInstance {
        &self.instance
    }

    pub fn device(&self) -> &NativeDevice {
        &self.device
    }

    fn shared_pipelines(self: &Arc<Self>) -> AureaResult<Arc<SharedPipelines>> {
        let mut cached = self
            .pipelines
            .lock()
            .map_err(|_| AureaError::RenderingFailed)?;
        if let Some(pipelines) = cached.upgrade() {
            return Ok(pipelines);
        }
        let pipelines = Arc::new(SharedPipelines {
            pipelines: Pipelines::new(self.device(), Format::Bgra8Unorm).map_err(gpu_err)?,
            context: Arc::clone(self),
        });
        *cached = Arc::downgrade(&pipelines);
        Ok(pipelines)
    }

    /// The Vulkan device context, for frame graphs and engine-side Vulkan
    /// resources. Vulkan only; macOS runs on Metal.
    #[cfg(not(target_os = "macos"))]
    pub fn device_context(&self) -> DeviceContext {
        self.device.context()
    }
}

/// This frame's instance-buffer handles, one per primitive kind. `None` when
/// that kind has zero instances this frame (its buffer wasn't (re)uploaded).
struct FrameBuffers {
    rect: Option<BufferHandle>,
    shape: Option<BufferHandle>,
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

/// Pipelines shared by renderers without a strong cache cycle.
struct SharedPipelines {
    pipelines: Pipelines,
    context: Arc<ZenGpuContext>,
}

impl Deref for SharedPipelines {
    type Target = Pipelines;
    fn deref(&self) -> &Pipelines {
        &self.pipelines
    }
}

impl Drop for SharedPipelines {
    fn drop(&mut self) {
        self.pipelines.destroy(self.context.device());
    }
}

/// ZenGPU device backend for `Gpu2dRenderer`, using the unified graphics API.
pub struct ZenGpuBackend {
    // Surface drops first; shared pipelines keep the context alive for cleanup.
    surface: NativeSurface,
    context: Arc<ZenGpuContext>,
    pipelines: Arc<SharedPipelines>,
    sampler: SamplerHandle,
    rect_buf: GrowableBuffer,
    shape_buf: GrowableBuffer,
    gradient_buf: GrowableBuffer,
    image_buf: GrowableBuffer,
    text_buf: GrowableBuffer,
    gradient_instances: Vec<GradientInstance>,
    image_instances: Vec<ImageInstance>,
    text_instances: Vec<TextInstance>,
    /// Maps shader slot (global bindless index) → TextureHandle for cleanup.
    slot_textures: HashMap<u32, TextureHandle>,
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

        let surface = create_surface(context.device(), handles, pw, ph).map_err(gpu_err)?;
        let pipelines = context.shared_pipelines()?;
        // Trilinear: a texture with mips is averaged as it shrinks, which is
        // `Sampling::Smooth`; one without has a single level to read.
        let sampler = context.device().create_sampler(SamplerDesc {
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mip_filter: FilterMode::Linear,
            lod_max: 32.0,
            ..SamplerDesc::default()
        });
        let sampler = match sampler {
            Ok(sampler) => sampler,
            Err(e) => {
                pipelines.destroy(context.device());
                return Err(gpu_err(e));
            }
        };

        let backend = ZenGpuBackend {
            surface,
            context,
            pipelines,
            sampler,
            rect_buf: GrowableBuffer::new(Default::default()),
            shape_buf: GrowableBuffer::new(Default::default()),
            gradient_buf: GrowableBuffer::new(Default::default()),
            image_buf: GrowableBuffer::new(Default::default()),
            text_buf: GrowableBuffer::new(Default::default()),
            gradient_instances: Vec::new(),
            image_instances: Vec::new(),
            text_instances: Vec::new(),
            slot_textures: HashMap::new(),
        };
        Ok(Gpu2dRenderer::from_backend(backend, width, height, scale))
    }

    pub fn size(&self) -> (u32, u32) {
        self.backend().surface.size()
    }

    pub fn context(&self) -> &Arc<ZenGpuContext> {
        &self.backend().context
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

    /// Bind `pipeline`/`buffer`, push `scalars`+`textures`, and issue one
    /// instanced draw over `start..start + count`.
    fn draw_run(
        cmd: &mut NativeCommandList,
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
    fn set_scissor(cmd: &mut NativeCommandList, viewport: (f32, f32), clip: Option<Rect>) {
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
        cmd: &mut NativeCommandList,
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
                DrawRef::Shape(start) => {
                    let count =
                        coalesce_run(order, clips, i, |r, n| *r == DrawRef::Shape(start + n));
                    Self::draw_run(
                        cmd,
                        self.pipelines.shape,
                        buffers.shape,
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
}

impl ZenGpuBackend {
    /// Uploads RGBA8 `rgba` as a texture of `levels` mip levels, the ones
    /// past the first made on the device, and binds it.
    fn upload(&mut self, width: u32, height: u32, rgba: &[u8], levels: u32) -> AureaResult<u32> {
        let device = self.context.device();
        let mut usage = TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST;
        if levels > 1 {
            usage |= TextureUsage::TRANSFER_SRC;
        }
        let texture = device
            .create_texture(TextureDesc {
                width,
                height,
                depth: 1,
                format: Format::Rgba8Unorm,
                usage,
                samples: 1,
                dimension: TexDim::D2,
                mip_levels: levels,
                array_layers: 1,
            })
            .map_err(gpu_err)?;
        let filled = device.upload_texture_data(texture, rgba).and_then(|()| {
            if levels > 1 {
                device.generate_mipmaps(texture)
            } else {
                Ok(())
            }
        });
        if let Err(e) = filled {
            device.destroy_texture(texture);
            return Err(gpu_err(e));
        }
        let Some(slot) = device.bind_texture(texture, self.sampler) else {
            device.destroy_texture(texture);
            return Err(AureaError::RenderingFailed);
        };
        self.slot_textures.insert(slot, texture);
        Ok(slot)
    }
}

/// Mip levels down to one texel for an image this size.
fn mip_levels(width: u32, height: u32) -> u32 {
    u32::BITS - width.max(height).max(1).leading_zeros()
}

impl Gpu2dBackend for ZenGpuBackend {
    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()> {
        self.surface
            .resize(physical_width, physical_height)
            .map_err(gpu_err)
    }

    fn upload_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> AureaResult<u32> {
        self.upload(width, height, rgba, 1)
    }

    fn upload_image_mipmapped(&mut self, width: u32, height: u32, rgba: &[u8]) -> AureaResult<u32> {
        self.upload(width, height, rgba, mip_levels(width, height))
    }

    fn evict_image(&mut self, shader_slot: u32) {
        if let Some(texture) = self.slot_textures.remove(&shader_slot) {
            self.context.device().destroy_texture(texture);
        }
    }

    /// Binds the app's texture with the backend's sampler. Its slot is its
    /// own handle index, so binding it again each frame reuses one slot.
    fn bind_texture(&mut self, texture: GpuTexture) -> AureaResult<u32> {
        self.context
            .device()
            .bind_texture(texture.zengpu(), self.sampler)
            .ok_or(AureaError::RenderingFailed)
    }

    fn features(&self) -> GpuFeatures {
        GpuFeatures {
            textures: true,
            gradients: true,
            images: true,
            text: true,
            clips: true,
            rounded_rects: true,
            stroked_circles: true,
            ..GpuFeatures::NONE
        }
    }

    fn supports_dual_source(&self) -> bool {
        self.context.device().supports_dual_source_blending()
    }

    fn present_frame(
        &mut self,
        plan: &FramePlan,
        rects: &[RectInstance],
        shapes: &[ShapeInstance],
    ) -> AureaResult<FrameOutcome> {
        let frame = match self.surface.acquire().map_err(gpu_err)? {
            Acquire::Frame(f) => f,
            Acquire::Skip => return Ok(FrameOutcome::Retry),
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
                sampling: i.sampling,
                slot: i.slot,
                _pad: [0; 3],
            }));
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
        let shape_handle = self
            .shape_buf
            .upload(device, as_bytes(shapes))
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
            shape: shape_handle,
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

        cmd.end_render_pass();
        self.surface.present(frame, cmd).map_err(gpu_err)?;
        Ok(FrameOutcome::Presented)
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
        self.shape_buf.destroy(device);
        self.gradient_buf.destroy(device);
        self.image_buf.destroy(device);
        self.text_buf.destroy(device);
        device.destroy_sampler(self.sampler);
    }
}

/// A lost device is told apart, so the renderer is made again on a new one
/// instead of failing every frame.
fn gpu_err(e: GpuError) -> AureaError {
    match e {
        GpuError::DeviceLost => AureaError::DeviceLost,
        _ => AureaError::ElementOperationFailed,
    }
}

fn as_bytes<T: Copy>(slice: &[T]) -> &[u8] {
    unsafe { from_raw_parts(slice.as_ptr() as *const u8, size_of_val(slice)) }
}

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn renderers_share_pipelines_without_retaining_the_device_forever() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        let first = context.shared_pipelines().expect("pipelines");
        let second = context.shared_pipelines().expect("shared pipelines");
        assert!(Arc::ptr_eq(&first, &second));
        let weak = Arc::downgrade(&first);
        drop(first);
        assert!(weak.upgrade().is_some());
        drop(second);
        assert!(weak.upgrade().is_none());
        assert_eq!(Arc::strong_count(&context), 1);
    }

    #[test]
    fn mip_chains_run_down_to_one_texel() {
        assert_eq!(mip_levels(1, 1), 1);
        assert_eq!(mip_levels(256, 1), 9);
        assert_eq!(mip_levels(4096, 300), 13);
        assert_eq!(mip_levels(4097, 1), 13);
        assert_eq!(mip_levels(0, 0), 1, "an empty image still has one");
    }

    /// What a smooth image upload asks of the device: a full mip chain,
    /// level 0 uploaded, the rest made on the device.
    #[test]
    fn a_mipmapped_image_uploads() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let device = context.device();
        let (w, h) = (64, 32);
        let texture = device
            .create_texture(TextureDesc {
                width: w,
                height: h,
                depth: 1,
                format: Format::Rgba8Unorm,
                usage: TextureUsage::SAMPLED
                    | TextureUsage::TRANSFER_DST
                    | TextureUsage::TRANSFER_SRC,
                samples: 1,
                dimension: TexDim::D2,
                mip_levels: mip_levels(w, h),
                array_layers: 1,
            })
            .expect("texture");
        device
            .upload_texture_data(texture, &vec![0x80u8; (w * h * 4) as usize])
            .expect("level 0");
        device.generate_mipmaps(texture).expect("mips");
        device.destroy_texture(texture);
    }

    #[test]
    fn a_context_reopens_on_its_own_gpu() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let uuid = context.adapter_uuid();
        let listed = ZenGpuContext::adapters().expect("adapters");
        assert!(listed.iter().any(|a| a.uuid == uuid), "{listed:?}");
        let again = context.reopen().expect("reopen");
        assert_eq!(again.adapter_uuid(), uuid);
        assert!(ZenGpuContext::on_adapter([0xEE; 16]).is_err());
    }
}
