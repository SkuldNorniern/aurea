//! ZenGPU 2D painter on the unified graphics API — `impl Gpu2dBackend for ZenGpuBackend`.

use std::collections::HashMap;
use std::iter::once;
use std::mem::{size_of, size_of_val};
use std::slice::{from_raw_parts, from_raw_parts_mut, from_ref};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use aurea_foundation::{AureaError, AureaResult};

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
use zengpu::DeviceContext;
use zengpu::native::{
    NativeAdapter, NativeCommandList, NativeDevice, NativeInstance, NativeOffscreenTarget,
    NativeSurface,
};
use zengpu_hal::{
    Acquire, Bindings, BufferDesc, BufferHandle, BufferUsage, ColorAttachment, DeviceRequest,
    DeviceType, FilterMode, Format, Frame, GpuAdapter, GpuDevice, GpuError, GraphicsDevice, LoadOp,
    MemoryUsage, PipelineHandle, Rect as HalRect, RenderCommands, RenderPassDesc, SamplerDesc,
    SamplerHandle, Scalar, Surface, TexDim, TextureDesc, TextureHandle, TextureRegion,
    TextureUsage, TextureWrite, Viewport, ViewportScissor, WindowHandles,
};

use crate::capabilities::GpuFeatures;
use crate::gpu::batch::{DrawRef, RectInstance, ShapeInstance};
use crate::gpu::{FramePlan, Gpu2dBackend, Gpu2dRenderer};
use crate::memory::smaller_buffer;
use crate::numeric::{f32_to_u8_clamped, f32_to_u32_clamped};
use crate::renderer::FrameOutcome;
use crate::surface::pixel_size;
use crate::types::{GpuTexture, Image, Rect, Sampling};

use super::buffer::GrowableBuffer;
use super::pipelines::{GradientInstance, ImageInstance, Pipelines, TextInstance};
use super::surface::create_surface;

const _: () = assert!(size_of::<RectInstance>() == size_of::<super::pipelines::RectInstance>());
const _: () = assert!(size_of::<ShapeInstance>() == size_of::<super::pipelines::ShapeInstance>());

/// Shareable ZenGPU instance/device ownership for Aurea UI and engine rendering.
pub struct ZenGpuContext {
    pipelines: Mutex<Weak<SharedPipelines>>,
    instance: NativeInstance,
    device: NativeDevice,
    adapter: [u8; 16],
}

/// A GPU a context can open, for a "which GPU" setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuChoice {
    /// Display name.
    pub name: String,
    /// The same for this GPU and driver from one run to the next: what to save, and give
    /// [`ZenGpuContext::on_adapter`].
    pub uuid: [u8; 16],
    /// A separate card rather than one built into the CPU.
    pub discrete: bool,
}

impl ZenGpuContext {
    /// Uploads an app-owned image on the shared device.
    pub fn upload_image(
        self: &Arc<Self>,
        image: &Image,
        sampling: Sampling,
    ) -> AureaResult<ZenGpuImage> {
        let levels = if sampling == Sampling::Smooth {
            mip_levels(image.width, image.height)
        } else {
            1
        };
        let handle = make_image_texture(
            self.device(),
            image.width,
            image.height,
            &image.data,
            levels,
        )?;
        Ok(ZenGpuImage {
            context: Arc::clone(self),
            texture: GpuTexture::from_zengpu(handle, image.width, image.height),
            levels,
        })
    }

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

    /// A context on the GPU with this [`GpuChoice::uuid`].
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

    /// The ZenGPU instance this context owns.
    pub fn instance(&self) -> &NativeInstance {
        &self.instance
    }

    /// The ZenGPU device this context owns.
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
            pipelines: Mutex::new(Pipelines::default()),
            context: Arc::clone(self),
        });
        *cached = Arc::downgrade(&pipelines);
        Ok(pipelines)
    }

    /// The Vulkan device context, for frame graphs and engine-side Vulkan resources.
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    pub fn device_context(&self) -> DeviceContext {
        self.device.context()
    }
}

/// An image texture owned by its shared GPU context.
pub struct ZenGpuImage {
    context: Arc<ZenGpuContext>,
    texture: GpuTexture,
    levels: u32,
}

impl ZenGpuImage {
    /// The texture handle.
    pub fn texture(&self) -> GpuTexture {
        self.texture
    }

    /// Updates equal-sized RGBA8 pixels and regenerates this image's mips.
    pub fn update(&mut self, image: &Image) -> AureaResult<()> {
        if image.width != self.texture.width
            || image.height != self.texture.height
            || !valid_image(image.width, image.height, &image.data)
        {
            return Err(AureaError::RenderingFailed);
        }
        let device = self.context.device();
        let texture = self.texture.zengpu();
        upload_levels(
            device,
            texture,
            image.width,
            image.height,
            &image.data,
            self.levels,
        )
        .map_err(gpu_err)
    }
}

impl Drop for ZenGpuImage {
    fn drop(&mut self) {
        self.context.device().destroy_texture(self.texture.zengpu());
    }
}

fn valid_image(width: u32, height: u32, rgba: &[u8]) -> bool {
    width > 0
        && height > 0
        && (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            == Some(rgba.len())
}

fn make_image_texture(
    device: &NativeDevice,
    width: u32,
    height: u32,
    rgba: &[u8],
    levels: u32,
) -> AureaResult<TextureHandle> {
    if !valid_image(width, height, rgba) {
        return Err(AureaError::RenderingFailed);
    }
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
    let filled = upload_levels(device, texture, width, height, rgba, levels);
    if let Err(e) = filled {
        device.destroy_texture(texture);
        return Err(gpu_err(e));
    }
    Ok(texture)
}

fn upload_levels(
    device: &NativeDevice,
    texture: TextureHandle,
    width: u32,
    height: u32,
    rgba: &[u8],
    levels: u32,
) -> Result<(), GpuError> {
    let mut chain = Vec::with_capacity(levels.saturating_sub(1) as usize);
    let (mut w, mut h) = (width, height);
    for _ in 1..levels {
        let (dw, dh) = ((w >> 1).max(1), (h >> 1).max(1));
        let level = area_downsample(chain.last().map_or(rgba, Vec::as_slice), (w, h), (dw, dh));
        chain.push(level);
        (w, h) = (dw, dh);
    }
    if device.capabilities().transfers.texture_region_write {
        let writes: Vec<TextureWrite<'_>> = once(rgba)
            .chain(chain.iter().map(Vec::as_slice))
            .zip(0u32..)
            .map(|(data, mip)| {
                let (lw, lh) = ((width >> mip).max(1), (height >> mip).max(1));
                TextureWrite {
                    texture,
                    region: TextureRegion {
                        mip_level: mip,
                        ..TextureRegion::rect(0, 0, lw, lh)
                    },
                    data,
                    bytes_per_row: lw * 4,
                }
            })
            .collect();
        return device.write_textures(&writes)?.wait(Duration::MAX);
    }
    device.upload_texture_data(texture, rgba)?;
    for (mip, level) in (1..).zip(&chain) {
        match device.upload_texture_data_region(texture, mip, 0, level) {
            Ok(()) => {}
            Err(GpuError::UnsupportedFeatures(_)) if mip == 1 => {
                return device.generate_mipmaps(texture);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn area_downsample(src: &[u8], (sw, sh): (u32, u32), (dw, dh): (u32, u32)) -> Vec<u8> {
    fn spans(src: u32, dst: u32) -> Vec<Vec<(usize, f32)>> {
        let ratio = src as f32 / dst as f32;
        (0..dst)
            .map(|i| {
                let (lo, hi) = (i as f32 * ratio, (i + 1) as f32 * ratio);
                let first = f32_to_u32_clamped(lo.floor());
                let last = f32_to_u32_clamped(hi.ceil()).min(src);
                (first..last)
                    .map(|t| {
                        let weight = hi.min((t + 1) as f32) - lo.max(t as f32);
                        (t as usize, weight.max(0.0) / ratio)
                    })
                    .collect()
            })
            .collect()
    }
    let (xs, ys) = (spans(sw, dw), spans(sh, dh));
    let mut out = Vec::with_capacity(dw as usize * dh as usize * 4);
    for row in &ys {
        for column in &xs {
            let mut sum = [0.0f32; 4];
            for &(y, wy) in row {
                for &(x, wx) in column {
                    let at = (y * sw as usize + x) * 4;
                    let p = &src[at..at + 4];
                    let a = f32::from(p[3]) * wx * wy;
                    sum[0] += f32::from(p[0]) * a;
                    sum[1] += f32::from(p[1]) * a;
                    sum[2] += f32::from(p[2]) * a;
                    sum[3] += a;
                }
            }
            let alpha = sum[3];
            let straight = |c: f32| {
                if alpha > 0.0 {
                    f32_to_u8_clamped((c / alpha).round())
                } else {
                    0
                }
            };
            out.extend_from_slice(&[
                straight(sum[0]),
                straight(sum[1]),
                straight(sum[2]),
                f32_to_u8_clamped(alpha.round()),
            ]);
        }
    }
    out
}

struct FrameBuffers {
    rect: Option<BufferHandle>,
    shape: Option<BufferHandle>,
    gradient: Option<BufferHandle>,
    image: Option<BufferHandle>,
    text: Option<BufferHandle>,
}

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

struct SharedPipelines {
    pipelines: Mutex<Pipelines>,
    context: Arc<ZenGpuContext>,
}

impl Drop for SharedPipelines {
    fn drop(&mut self) {
        let pipelines = self
            .pipelines
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pipelines.destroy(self.context.device());
    }
}

enum Target {
    /// A window's swapchain: presented, never read back.
    Window(Box<NativeSurface>),
    /// A texture of its own, read back after each frame.
    Offscreen(Offscreen),
}

struct Offscreen {
    target: NativeOffscreenTarget,
    readback: Option<(BufferHandle, u64)>,
    pixels: Vec<u32>,
}

impl Offscreen {
    fn read_back(&mut self, device: &NativeDevice) -> AureaResult<()> {
        let (width, height) = self.target.extent();
        let row = width.checked_mul(4).ok_or(AureaError::RenderingFailed)?;
        let len = u64::from(row) * u64::from(height);
        let count = usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|_| AureaError::RenderingFailed)?;
        let buffer = match self.readback {
            Some((buffer, size)) if size >= len => buffer,
            stale => {
                if let Some((buffer, _)) = stale {
                    device.destroy_buffer(buffer);
                    self.readback = None;
                }
                let buffer = device
                    .create_buffer(BufferDesc {
                        size: len,
                        usage: BufferUsage::READBACK | BufferUsage::TRANSFER_DST,
                        memory: MemoryUsage::Readback,
                    })
                    .map_err(gpu_err)?;
                self.readback = Some((buffer, len));
                buffer
            }
        };
        device
            .copy_texture_region_to_buffer(
                self.target.texture_handle(),
                TextureRegion::rect(0, 0, width, height),
                buffer,
                0,
                row,
            )
            .map_err(gpu_err)?
            .wait(Duration::MAX)
            .map_err(gpu_err)?;
        if let Some(compact) = smaller_buffer(self.pixels.capacity(), count) {
            self.pixels = compact;
        }
        self.pixels.resize(count, 0);
        let bytes = unsafe {
            from_raw_parts_mut(
                self.pixels.as_mut_ptr().cast::<u8>(),
                size_of_val(self.pixels.as_slice()),
            )
        };
        device.read_buffer_into(buffer, 0, bytes).map_err(gpu_err)
    }

    fn release(&mut self, device: &NativeDevice) {
        if let Some((buffer, _)) = self.readback.take() {
            device.destroy_buffer(buffer);
        }
    }
}

/// ZenGPU device backend for `Gpu2dRenderer`, using the unified graphics API.
pub struct ZenGpuBackend {
    target: Target,
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
    slot_textures: HashMap<u32, TextureHandle>,
}

/// Public renderer type alias — `Gpu2dRenderer` parameterized on `ZenGpuBackend`.
pub type ZenGpuRenderer = Gpu2dRenderer<ZenGpuBackend>;

impl ZenGpuRenderer {
    /// Creates a renderer on a new context.
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

    /// Creates a renderer sharing `context`.
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
        Self::on_target(
            Target::Window(Box::new(surface)),
            context,
            width,
            height,
            scale,
        )
    }

    /// A renderer drawing into a texture of its own instead of a window, `width` by `height`
    /// logical pixels at `scale_factor`.
    pub fn offscreen(
        context: Arc<ZenGpuContext>,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> AureaResult<Self> {
        let scale = scale_factor.max(1.0);
        let (pw, ph) = pixel_size(width, height, scale, None);
        let target = NativeOffscreenTarget::new(context.device(), Format::Bgra8Unorm, pw, ph)
            .map_err(gpu_err)?;
        let offscreen = Offscreen {
            target,
            readback: None,
            pixels: Vec::new(),
        };
        Self::on_target(Target::Offscreen(offscreen), context, width, height, scale)
    }

    fn on_target(
        target: Target,
        context: Arc<ZenGpuContext>,
        width: u32,
        height: u32,
        scale: f32,
    ) -> AureaResult<Self> {
        let pipelines = context.shared_pipelines()?;
        let sampler = context.device().create_sampler(SamplerDesc {
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mip_filter: FilterMode::Linear,
            lod_max: 32.0,
            ..SamplerDesc::default()
        });
        let sampler = sampler.map_err(gpu_err)?;

        let backend = ZenGpuBackend {
            target,
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

    /// The target's size in device pixels.
    pub fn size(&self) -> (u32, u32) {
        match &self.backend().target {
            Target::Window(surface) => surface.size(),
            Target::Offscreen(offscreen) => offscreen.target.extent(),
        }
    }

    /// The shared GPU context.
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
        pipelines: Pipelines,
    ) -> AureaResult<()> {
        let mut i = 0;
        let mut active_clip: Option<Option<Rect>> = None;
        while i < order.len() {
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
                        pipelines.rect.ok_or(AureaError::RenderingFailed)?,
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
                        pipelines.shape.ok_or(AureaError::RenderingFailed)?,
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
                        pipelines.gradient.ok_or(AureaError::RenderingFailed)?,
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
                        pipelines.image.ok_or(AureaError::RenderingFailed)?,
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
                        pipelines.text.ok_or(AureaError::RenderingFailed)?,
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
        Ok(())
    }
}

impl ZenGpuBackend {
    fn upload(&mut self, width: u32, height: u32, rgba: &[u8], levels: u32) -> AureaResult<u32> {
        let device = self.context.device();
        let texture = make_image_texture(device, width, height, rgba, levels)?;
        let Some(slot) = device.bind_texture(texture, self.sampler) else {
            device.destroy_texture(texture);
            return Err(AureaError::RenderingFailed);
        };
        self.slot_textures.insert(slot, texture);
        Ok(slot)
    }
}

fn mip_levels(width: u32, height: u32) -> u32 {
    u32::BITS - width.max(height).max(1).leading_zeros()
}

impl Gpu2dBackend for ZenGpuBackend {
    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()> {
        match &mut self.target {
            Target::Window(surface) => surface
                .resize(physical_width, physical_height)
                .map_err(gpu_err),
            Target::Offscreen(offscreen) => {
                let size = (physical_width.max(1), physical_height.max(1));
                if offscreen.target.extent() == size {
                    return Ok(());
                }
                offscreen
                    .target
                    .resize(self.context.device(), size.0, size.1)
                    .map_err(gpu_err)
            }
        }
    }

    fn frame_pixels(&self) -> Option<(&[u32], u32, u32)> {
        let Target::Offscreen(offscreen) = &self.target else {
            return None;
        };
        let (width, height) = offscreen.target.extent();
        let full = usize::try_from(u64::from(width) * u64::from(height)).ok()?;
        (offscreen.pixels.len() == full).then_some((offscreen.pixels.as_slice(), width, height))
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
        let pipelines = {
            let mut pipelines = self
                .pipelines
                .pipelines
                .lock()
                .map_err(|_| AureaError::RenderingFailed)?;
            pipelines
                .ensure(self.context.device(), Format::Bgra8Unorm, &plan.order)
                .map_err(gpu_err)?;
            *pipelines
        };

        let vw = plan.viewport_width as f32;
        let vh = plan.viewport_height as f32;
        let device = self.context.device();

        if let Some(compact) =
            smaller_buffer(self.gradient_instances.capacity(), plan.gradients.len())
        {
            self.gradient_instances = compact;
        }
        if let Some(compact) = smaller_buffer(self.image_instances.capacity(), plan.images.len()) {
            self.image_instances = compact;
        }
        if let Some(compact) = smaller_buffer(self.text_instances.capacity(), plan.texts.len()) {
            self.text_instances = compact;
        }
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
                affine: i.affine,
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

        self.rect_buf
            .prepare(device, size_of_val(rects))
            .map_err(gpu_err)?;
        self.shape_buf
            .prepare(device, size_of_val(shapes))
            .map_err(gpu_err)?;
        self.gradient_buf
            .prepare(device, size_of_val(self.gradient_instances.as_slice()))
            .map_err(gpu_err)?;
        self.image_buf
            .prepare(device, size_of_val(self.image_instances.as_slice()))
            .map_err(gpu_err)?;
        self.text_buf
            .prepare(device, size_of_val(self.text_instances.as_slice()))
            .map_err(gpu_err)?;
        let mut cmd = device.create_command_list().map_err(gpu_err)?;
        let frame = match &self.target {
            Target::Window(surface) => match surface.acquire().map_err(gpu_err)? {
                Acquire::Frame(f) => Some(f),
                Acquire::Skip => return Ok(FrameOutcome::Retry),
            },
            Target::Offscreen(_) => None,
        };
        let (target, sample_after) = match (&frame, &self.target) {
            (Some(frame), _) => (frame.target(), false),
            (None, Target::Offscreen(offscreen)) => (offscreen.target.target_handle(), true),
            (None, Target::Window(_)) => return Ok(FrameOutcome::Retry),
        };

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
                target,
                load,
                store: true,
                sample_after,
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
            pipelines,
        )?;

        cmd.end_render_pass();
        match (&mut self.target, frame) {
            (Target::Window(surface), Some(frame)) => {
                surface.present(frame, cmd).map_err(gpu_err)?;
            }
            (Target::Offscreen(offscreen), _) => {
                let device = self.context.device();
                device.submit_and_wait(cmd).map_err(gpu_err)?;
                offscreen.read_back(device)?;
            }
            (Target::Window(_), None) => return Ok(FrameOutcome::Retry),
        }
        Ok(FrameOutcome::Presented)
    }
}

impl Drop for ZenGpuBackend {
    fn drop(&mut self) {
        let device = self.context.device();
        let _ = device.wait_idle();
        if let Target::Offscreen(offscreen) = &mut self.target {
            offscreen.release(device);
        }
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
    use crate::cpu::CpuRasterizer;
    use crate::renderer::{DrawingContext, Renderer};
    use crate::surface::{Surface as RenderSurface, SurfaceInfo};
    use crate::types::{Color, Paint, PaintStyle, Point};

    fn scene(ctx: &mut dyn DrawingContext, image: &Image) {
        ctx.clear(Color::rgb(20, 24, 30)).unwrap();
        ctx.draw_rect(
            Rect::new(3.0, 4.0, 22.0, 9.0),
            &Paint::new().color(Color::rgb(200, 60, 40)),
        )
        .unwrap();
        let border = Paint::new()
            .color(Color::rgb(240, 240, 240))
            .style(PaintStyle::Stroke)
            .stroke_width(1.0);
        ctx.set_pixel_snap(true).unwrap();
        ctx.draw_rect(Rect::new(2.0, 16.0, 30.0, 10.0), &border)
            .unwrap();
        ctx.set_pixel_snap(false).unwrap();
        ctx.draw_circle(
            Point::new(33.0, 8.0),
            5.0,
            &Paint::new().color(Color::rgb(60, 180, 90)),
        )
        .unwrap();
        ctx.set_sampling(Sampling::Nearest).unwrap();
        ctx.draw_image_rect(image, Rect::new(6.0, 18.0, 8.0, 6.0))
            .unwrap();
    }

    #[test]
    fn shadows_match_the_cpu() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let (w, h) = (48, 40);
        let draw = |ctx: &mut dyn DrawingContext| {
            ctx.clear(Color::rgb(255, 255, 255)).unwrap();
            ctx.draw_shadow(
                Rect::new(12.0, 10.0, 24.0, 18.0),
                5.0,
                3.0,
                Color::rgba(0, 0, 0, 200),
            )
            .unwrap();
        };
        let mut cpu = CpuRasterizer::new(w, h);
        cpu.init(RenderSurface::Cpu, SurfaceInfo::new(w, h, 1.0))
            .unwrap();
        draw(&mut *cpu.begin_frame().unwrap());
        cpu.end_frame().unwrap();
        let mut gpu = ZenGpuRenderer::offscreen(Arc::new(context), w, h, 1.0).unwrap();
        draw(&mut *gpu.begin_frame().unwrap());
        assert_eq!(gpu.end_frame().unwrap(), FrameOutcome::Presented);

        let (want, got) = (
            cpu.frame_output().expect("cpu"),
            gpu.frame_output().expect("gpu"),
        );
        let linear = |v: u32| {
            let c = v as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let alpha = 200.0 / 255.0;
        let worst = got
            .words()
            .iter()
            .zip(want.words())
            .map(|(&g, &c)| {
                let gpu = (255.0 - (g & 0xFF) as f32) / 255.0 / alpha;
                let cpu = (1.0 - linear(c & 0xFF)) / alpha;
                (gpu - cpu).abs()
            })
            .fold(0.0, f32::max);
        assert!(
            worst < 0.04,
            "gpu and cpu shadow coverage differ by {worst}"
        );
        let middle = got.words()[(19 * w + 24) as usize] & 0xFF;
        assert!(middle < 80, "no shadow under the middle: {middle}");
    }

    #[test]
    fn offscreen_frames_match_the_cpu_at_150_percent() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        let (w, h, scale) = (40, 30, 1.5);
        let image = Image::new(
            2,
            2,
            [
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                [0, 255, 0, 255],
                [255, 255, 0, 255],
            ]
            .concat(),
        );

        let mut cpu = CpuRasterizer::new(w, h);
        cpu.init(RenderSurface::Cpu, SurfaceInfo::new(w, h, scale))
            .unwrap();
        scene(&mut *cpu.begin_frame().unwrap(), &image);
        cpu.end_frame().unwrap();

        let mut gpu = ZenGpuRenderer::offscreen(Arc::clone(&context), w, h, scale).unwrap();
        scene(&mut *gpu.begin_frame().unwrap(), &image);
        assert_eq!(gpu.end_frame().unwrap(), FrameOutcome::Presented);

        let want = cpu.frame_output().expect("cpu frame");
        let got = gpu.frame_output().expect("gpu frame");
        assert_eq!((got.width, got.height), (60, 45));
        assert_eq!((got.width, got.height), (want.width, want.height));
        let channel = |p: u32, s: u32| (p >> s) & 0xFF;
        let mut off = 0;
        for (i, (&a, &b)) in got.words().iter().zip(want.words()).enumerate() {
            let worst = [0, 8, 16]
                .iter()
                .map(|&s| channel(a, s).abs_diff(channel(b, s)))
                .max()
                .unwrap_or(0);
            if worst > 48 {
                off += 1;
                assert!(
                    off <= 12,
                    "pixel {} {}: gpu {a:08x} cpu {b:08x}",
                    i % 60,
                    i / 60
                );
            }
        }

        let backend = gpu.backend();
        let Target::Offscreen(first) = &backend.target else {
            unreachable!("made offscreen")
        };
        let (buffer, storage) = (first.readback.map(|r| r.0), first.pixels.as_ptr());
        scene(&mut *gpu.begin_frame().unwrap(), &image);
        gpu.end_frame().unwrap();
        let Target::Offscreen(second) = &gpu.backend().target else {
            unreachable!("made offscreen")
        };
        assert_eq!(second.readback.map(|r| r.0), buffer);
        assert_eq!(second.pixels.as_ptr(), storage);
    }

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
    fn pipelines_are_created_only_when_drawn_and_reused_afterward() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        let shared = context.shared_pipelines().expect("cache");
        let mut pipelines = shared.pipelines.lock().expect("lock");
        pipelines
            .ensure(context.device(), Format::Bgra8Unorm, &[])
            .expect("clear-only frame");
        assert!(pipelines.rect.is_none() && pipelines.text.is_none());
        assert!(
            pipelines.shape.is_none() && pipelines.image.is_none() && pipelines.gradient.is_none()
        );
        pipelines
            .ensure(
                context.device(),
                Format::Bgra8Unorm,
                &[DrawRef::Rect(0), DrawRef::Rect(1)],
            )
            .expect("rects");
        let rect = pipelines.rect.expect("rect pipeline");
        assert!(
            pipelines.text.is_none()
                && pipelines.shape.is_none()
                && pipelines.image.is_none()
                && pipelines.gradient.is_none()
        );
        pipelines
            .ensure(
                context.device(),
                Format::Bgra8Unorm,
                &[DrawRef::Rect(0), DrawRef::Text(0)],
            )
            .expect("text");
        assert_eq!(pipelines.rect, Some(rect));
        assert!(pipelines.text.is_some());
        pipelines
            .ensure(
                context.device(),
                Format::Bgra8Unorm,
                &[DrawRef::Shape(0), DrawRef::Gradient(0), DrawRef::Image(0)],
            )
            .expect("remaining kinds");
        assert!(
            pipelines.shape.is_some() && pipelines.image.is_some() && pipelines.gradient.is_some()
        );
    }

    #[test]
    fn area_mips_cover_odd_edges_without_darkening_clear_texels() {
        let src = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255];
        assert_eq!(area_downsample(&src, (3, 1), (1, 1)), [85, 85, 85, 255]);
        let src = [255, 255, 255, 255, 0, 0, 0, 0];
        assert_eq!(area_downsample(&src, (2, 1), (1, 1)), [255, 255, 255, 128]);
        assert_eq!(area_downsample(&[0; 16], (2, 2), (1, 1)), [0; 4]);
    }

    #[test]
    fn mip_chains_run_down_to_one_texel() {
        assert_eq!(mip_levels(1, 1), 1);
        assert_eq!(mip_levels(256, 1), 9);
        assert_eq!(mip_levels(4096, 300), 13);
        assert_eq!(mip_levels(4097, 1), 13);
        assert_eq!(mip_levels(0, 0), 1, "an empty image still has one");
    }

    #[test]
    fn smooth_images_carry_area_filtered_mips() {
        use zengpu_hal::{BufferDesc, BufferUsage, MemoryUsage};
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        let image = Image::new(3, 1, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
        let owned = context
            .upload_image(&image, Sampling::Smooth)
            .expect("upload");
        let device = context.device();
        let buffer = device
            .create_buffer(BufferDesc {
                size: 12,
                usage: BufferUsage::READBACK | BufferUsage::TRANSFER_DST,
                memory: MemoryUsage::Readback,
            })
            .expect("buffer");
        let read = |mip_level: u32, width: u32| {
            let region = TextureRegion {
                mip_level,
                ..TextureRegion::rect(0, 0, width, 1)
            };
            device
                .copy_texture_region_to_buffer(
                    owned.texture().zengpu(),
                    region,
                    buffer,
                    0,
                    width * 4,
                )
                .expect("copy")
                .wait(Duration::MAX)
                .expect("wait");
            device
                .read_buffer(buffer, 0, u64::from(width) * 4)
                .expect("read")
        };
        assert_eq!(read(0, 3), &image.data[..]);
        assert_eq!(read(1, 1), [85, 85, 85, 255]);
        device.destroy_buffer(buffer);
    }

    #[test]
    fn owned_shared_images_update_and_release_their_native_texture() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        let image = Image::new(17, 9, vec![128; 17 * 9 * 4]);
        let mut owned = context
            .upload_image(&image, Sampling::Smooth)
            .expect("upload");
        assert_eq!(owned.levels, 5);
        owned.update(&image).expect("regenerate mips");
        assert!(owned.update(&Image::new(1, 1, vec![0; 4])).is_err());
        assert!(
            context
                .upload_image(&Image::new(17, 9, vec![0; 4]), Sampling::Smooth)
                .is_err()
        );
        let texture = owned.texture().zengpu();
        let sampler = context
            .device()
            .create_sampler(SamplerDesc::default())
            .expect("sampler");
        assert!(context.device().bind_texture(texture, sampler).is_some());
        drop(owned);
        assert!(
            context.device().bind_texture(texture, sampler).is_none(),
            "owner releases the texture"
        );
        context.device().destroy_sampler(sampler);
    }

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
