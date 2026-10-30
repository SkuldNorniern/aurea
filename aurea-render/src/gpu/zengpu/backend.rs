//! ZenGPU 2D painter on the unified graphics API — `impl Gpu2dBackend for ZenGpuBackend`.

use std::collections::HashMap;
use std::iter::once;
use std::mem::{size_of, size_of_val};
use std::slice::{from_raw_parts, from_raw_parts_mut, from_ref};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
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
    DeviceType, DispatchOp, FilterMode, Format, Frame, GpuAdapter, GpuDevice, GpuError,
    GraphicsDevice, LoadOp, MemoryUsage, PipelineHandle, Rect as HalRect, RenderCommands,
    RenderPassDesc, SamplerDesc, SamplerHandle, Scalar, Surface, TexDim, TextureDesc,
    TextureHandle, TextureRegion, TextureUsage, TextureWrite, Viewport, ViewportScissor,
    WindowHandles,
};
use zengpu_hal::{Submission, SubmissionStatus};

use crate::capabilities::GpuFeatures;
use crate::gpu::backend::full_frame_plan;
use crate::gpu::batch::{DrawRef, RectInstance, ShapeInstance};
use crate::gpu::{FramePlan, Gpu2dBackend, Gpu2dRenderer, PixelFormat};
use crate::memory::smaller_buffer;
use crate::numeric::{f32_to_u8_clamped, f32_to_u32_clamped};
use crate::renderer::FrameOutcome;
use crate::surface::pixel_size;
use crate::types::{GpuTexture, Image, Rect, Sampling};

use super::buffer::GrowableBuffer;
use super::clips::ClipMasks;
use super::pipelines::{GradientInstance, ImageInstance, Pipelines, TextInstance, create_mip_down};
use super::surface::create_surface;

const _: () = assert!(size_of::<RectInstance>() == size_of::<super::pipelines::RectInstance>());
const _: () = assert!(size_of::<ShapeInstance>() == size_of::<super::pipelines::ShapeInstance>());

/// Shareable ZenGPU instance/device ownership for Aurea UI and engine rendering.
pub struct ZenGpuContext {
    pipelines: Mutex<Weak<SharedPipelines>>,
    /// The mip kernel, made on first use. Owned here, not by `pipelines`, which may be gone.
    mip_down: Mutex<Option<PipelineHandle>>,
    mip_jobs: Mutex<HashMap<TextureHandle, PendingMip>>,
    instance: NativeInstance,
    device: NativeDevice,
    adapter: [u8; 16],
}

impl Drop for ZenGpuContext {
    fn drop(&mut self) {
        let jobs = self.mip_jobs.get_mut().unwrap_or_else(|p| p.into_inner());
        if !jobs.is_empty() {
            // Teardown may wait. Normal upload submission and polling never do.
            let _ = self.device.wait_idle();
            for (texture, job) in jobs.drain() {
                drop(job.work);
                let _ = self.device.release_storage_levels(texture);
                if job.destroy {
                    self.device.destroy_texture(texture);
                }
            }
        }
        let mip = self
            .mip_down
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(pipeline) = mip {
            self.device.destroy_pipeline(pipeline);
        }
    }
}

struct PendingMip {
    work: Submission,
    // Keep base uploads and superseded updates owned until the newest update is
    // done. This also avoids backend orphan collection under compute-table locks.
    dependencies: Vec<Submission>,
    destroy: bool,
}

fn track_upload(
    jobs: &mut HashMap<TextureHandle, PendingMip>,
    texture: TextureHandle,
    mut job: PendingMip,
) {
    if let Some(old) = jobs.remove(&texture) {
        job.dependencies.push(old.work);
        job.dependencies.extend(old.dependencies);
    }
    jobs.insert(texture, job);
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
    /// Polls image uploads and releases completed mip views without waiting.
    /// Returns the number of images still uploading on this context.
    pub fn poll_uploads(&self) -> AureaResult<usize> {
        self.collect_uploads().map_err(gpu_err)
    }

    fn collect_uploads(&self) -> Result<usize, GpuError> {
        let mut jobs = self.mip_jobs.lock().unwrap_or_else(|p| p.into_inner());
        let mut error = None;
        jobs.retain(|&texture, job| match job.work.poll() {
            Ok(SubmissionStatus::Complete) => {
                if let Err(e) = self.device.release_storage_levels(texture) {
                    error = Some(e);
                    return true;
                }
                if job.destroy {
                    self.device.destroy_texture(texture);
                }
                false
            }
            Ok(SubmissionStatus::Pending) => true,
            Err(e) => {
                error = Some(e);
                true
            }
        });
        match error {
            Some(e) => Err(e),
            None => Ok(jobs.len()),
        }
    }

    fn destroy_image(&self, texture: TextureHandle) {
        let _ = self.collect_uploads();
        let mut jobs = self.mip_jobs.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(job) = jobs.get_mut(&texture) {
            job.destroy = true;
        } else {
            self.device.destroy_texture(texture);
        }
    }
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
        let mips = self.mip_path(Format::Rgba8Unorm, levels);
        let handle = make_image_texture(
            self,
            image.width,
            image.height,
            &image.data,
            levels,
            Format::Rgba8Unorm,
            mips,
        )?;
        Ok(ZenGpuImage {
            context: Arc::clone(self),
            texture: GpuTexture::from_zengpu(handle, image.width, image.height),
            levels,
            mips,
        })
    }

    /// Where mips of an image in `format` get built: on the GPU when this device can write mips of
    /// a storage texture in it, else on the CPU.
    fn mip_path(&self, format: Format, levels: u32) -> Mips {
        let device = self.device();
        if levels < 2
            || format != Format::Rgba8Unorm
            || !device.format_caps(format).storage
            || device.limits().max_storage_textures == 0
        {
            return Mips::Cpu;
        }
        let mut cached = self
            .mip_down
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cached.is_none() {
            *cached = create_mip_down(device).ok();
        }
        cached.map_or(Mips::Cpu, Mips::Gpu)
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
            mip_down: Mutex::new(None),
            mip_jobs: Mutex::new(HashMap::new()),
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
    mips: Mips,
}

impl ZenGpuImage {
    /// Whether the last upload has completed. Drawing immediately after submission
    /// is valid on the context's ordered queue; callers need not wait for this.
    pub fn upload_complete(&self) -> AureaResult<bool> {
        self.context.poll_uploads()?;
        Ok(!self
            .context
            .mip_jobs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(&self.texture.zengpu()))
    }
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
        let texture = self.texture.zengpu();
        upload_levels(
            &self.context,
            texture,
            image.width,
            image.height,
            &image.data,
            self.levels,
            self.mips,
        )
        .map_err(gpu_err)
    }
}

impl Drop for ZenGpuImage {
    fn drop(&mut self) {
        self.context.destroy_image(self.texture.zengpu());
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

/// Who builds an image's mips: the CPU, or the GPU kernel with its pipeline.
#[derive(Clone, Copy)]
enum Mips {
    Cpu,
    Gpu(PipelineHandle),
}

fn make_image_texture(
    context: &ZenGpuContext,
    width: u32,
    height: u32,
    rgba: &[u8],
    levels: u32,
    format: Format,
    mips: Mips,
) -> AureaResult<TextureHandle> {
    let device = context.device();
    if !valid_image(width, height, rgba) {
        return Err(AureaError::RenderingFailed);
    }
    let mut usage = TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST;
    if levels > 1 {
        usage |= TextureUsage::TRANSFER_SRC;
    }
    if matches!(mips, Mips::Gpu(_)) {
        usage |= TextureUsage::STORAGE;
    }
    let texture = device
        .create_texture(TextureDesc {
            width,
            height,
            depth: 1,
            format,
            usage,
            samples: 1,
            dimension: TexDim::D2,
            mip_levels: levels,
            array_layers: 1,
        })
        .map_err(gpu_err)?;
    let filled = upload_levels(context, texture, width, height, rgba, levels, mips);
    if let Err(e) = filled {
        context.destroy_image(texture);
        return Err(gpu_err(e));
    }
    Ok(texture)
}

/// GPU mip builds that failed and fell back to the CPU.
#[cfg(test)]
static GPU_MIP_FALLBACKS: AtomicUsize = AtomicUsize::new(0);

fn upload_levels(
    context: &ZenGpuContext,
    texture: TextureHandle,
    width: u32,
    height: u32,
    rgba: &[u8],
    levels: u32,
    mips: Mips,
) -> Result<(), GpuError> {
    context.collect_uploads()?;
    let device = context.device();
    // Keep views stable while planning another update to this image. The newest
    // submission owns their lifetime because all updates use the same queue.
    let mut jobs = context.mip_jobs.lock().unwrap_or_else(|p| p.into_inner());
    if let Mips::Gpu(pipeline) = mips {
        let built = gpu_mip_chain(device, pipeline, texture, width, height, rgba, levels);
        if let Ok(job) = built {
            track_upload(&mut jobs, texture, job);
            return Ok(());
        }
        #[cfg(test)]
        GPU_MIP_FALLBACKS.fetch_add(1, Ordering::Relaxed);
    }
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
        let work = device.write_textures(&writes)?;
        track_upload(
            &mut jobs,
            texture,
            PendingMip {
                work,
                dependencies: Vec::new(),
                destroy: false,
            },
        );
        return Ok(());
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

/// Upload level 0 without a host wait, then build all dependent levels in one batch.
/// The batch's barriers make each level visible to the next. The caller retains
/// its submission and level views until completion, including after image drop.
fn gpu_mip_chain(
    device: &NativeDevice,
    pipeline: PipelineHandle,
    texture: TextureHandle,
    width: u32,
    height: u32,
    rgba: &[u8],
    levels: u32,
) -> Result<PendingMip, GpuError> {
    let upload = if device.capabilities().transfers.texture_region_write {
        Some(device.write_textures(&[TextureWrite {
            texture,
            region: TextureRegion::rect(0, 0, width, height),
            data: rgba,
            bytes_per_row: width * 4,
        }])?)
    } else {
        device.upload_texture_data(texture, rgba)?;
        None
    };
    let mut levels_to_build = Vec::with_capacity(levels.saturating_sub(1) as usize);
    let (mut w, mut h) = (width, height);
    for mip in 1..levels {
        let (dw, dh) = ((w >> 1).max(1), (h >> 1).max(1));
        let images = [
            device.storage_level(texture, mip - 1)?,
            device.storage_level(texture, mip)?,
        ];
        let scalars = [
            Scalar::F32(w as f32 / dw as f32),
            Scalar::F32(h as f32 / dh as f32),
        ];
        levels_to_build.push((images, scalars, [dw.div_ceil(8), dh.div_ceil(8), 1]));
        (w, h) = (dw, dh);
    }
    let ops: Vec<_> = levels_to_build
        .iter()
        .map(|(images, scalars, grid)| DispatchOp {
            pipeline,
            bindings: Bindings {
                images,
                scalars,
                ..Default::default()
            },
            grid: *grid,
        })
        .collect();
    let work = device.submit_batch(0, &ops)?;
    Ok(PendingMip {
        work,
        dependencies: upload.into_iter().collect(),
        destroy: false,
    })
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
struct CpuFrame {
    slot: u32,
    width: u32,
    height: u32,
    format: PixelFormat,
}

fn texture_format(format: PixelFormat) -> Format {
    match format {
        PixelFormat::Rgba8 => Format::Rgba8Unorm,
        PixelFormat::Bgra8 => Format::Bgra8Unorm,
    }
}

pub struct ZenGpuBackend {
    clip_masks: ClipMasks,
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
    cpu_frame: Option<CpuFrame>,
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
            clip_masks: ClipMasks::default(),
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
            cpu_frame: None,
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
        pipelines: Pipelines,
    ) -> AureaResult<()> {
        let mut i = 0;
        let mut active_clip: Option<Option<Rect>> = None;
        while i < order.len() {
            let binding = self.clip_masks.bindings[i];
            let viewport_scalars = binding.scalars(viewport);
            let same_mask = |n: u32| self.clip_masks.bindings.get(i + n as usize) == Some(&binding);
            let clip = clips.get(i).copied().flatten();
            if active_clip != Some(clip) {
                Self::set_scissor(cmd, viewport, clip);
                active_clip = Some(clip);
            }

            match order[i] {
                DrawRef::Rect(start) => {
                    let count = coalesce_run(order, clips, i, |r, n| {
                        *r == DrawRef::Rect(start + n) && same_mask(n)
                    });
                    Self::draw_run(
                        cmd,
                        pipelines.rect.ok_or(AureaError::RenderingFailed)?,
                        buffers.rect,
                        &viewport_scalars,
                        from_ref(&binding.slot),
                        start,
                        count,
                    );
                    i += count as usize;
                }
                DrawRef::Shape(start) => {
                    let count = coalesce_run(order, clips, i, |r, n| {
                        *r == DrawRef::Shape(start + n) && same_mask(n)
                    });
                    Self::draw_run(
                        cmd,
                        pipelines.shape.ok_or(AureaError::RenderingFailed)?,
                        buffers.shape,
                        &viewport_scalars,
                        from_ref(&binding.slot),
                        start,
                        count,
                    );
                    i += count as usize;
                }
                DrawRef::Gradient(start) => {
                    let slot = self.gradient_slot(start);
                    let count = coalesce_run(order, clips, i, |r, n| {
                        *r == DrawRef::Gradient(start + n)
                            && self.gradient_slot(start + n) == slot
                            && same_mask(n)
                    });
                    Self::draw_run(
                        cmd,
                        pipelines.gradient.ok_or(AureaError::RenderingFailed)?,
                        buffers.gradient,
                        &viewport_scalars,
                        &[binding.slot, slot],
                        start,
                        count,
                    );
                    i += count as usize;
                }
                DrawRef::Image(start) => {
                    let slot = self.image_slot(start);
                    let count = coalesce_run(order, clips, i, |r, n| {
                        *r == DrawRef::Image(start + n)
                            && self.image_slot(start + n) == slot
                            && same_mask(n)
                    });
                    Self::draw_run(
                        cmd,
                        pipelines.image.ok_or(AureaError::RenderingFailed)?,
                        buffers.image,
                        &viewport_scalars,
                        &[binding.slot, slot],
                        start,
                        count,
                    );
                    i += count as usize;
                }
                DrawRef::Text(start) => {
                    let slot = self.text_slot(start);
                    let count = coalesce_run(order, clips, i, |r, n| {
                        *r == DrawRef::Text(start + n)
                            && self.text_slot(start + n) == slot
                            && same_mask(n)
                    });
                    Self::draw_run(
                        cmd,
                        pipelines.text.ok_or(AureaError::RenderingFailed)?,
                        buffers.text,
                        &viewport_scalars,
                        &[binding.slot, slot],
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
    fn upload(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
        levels: u32,
        format: PixelFormat,
    ) -> AureaResult<u32> {
        let device = self.context.device();
        let format = texture_format(format);
        let mips = self.context.mip_path(format, levels);
        let texture =
            make_image_texture(&self.context, width, height, pixels, levels, format, mips)?;
        let Some(slot) = device.bind_texture(texture, self.sampler) else {
            self.context.destroy_image(texture);
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
    fn begin_frame(&mut self) -> AureaResult<()> {
        self.context.poll_uploads().map(|_| ())
    }

    fn trim_memory(&mut self) {
        let device = self.context.device();
        self.clip_masks.trim(device);
        self.rect_buf.destroy(device);
        self.shape_buf.destroy(device);
        self.gradient_buf.destroy(device);
        self.image_buf.destroy(device);
        self.text_buf.destroy(device);
        self.gradient_instances = Vec::new();
        self.image_instances = Vec::new();
        self.text_instances = Vec::new();
        if let Some(frame) = self.cpu_frame.take() {
            self.evict_image(frame.slot);
        }
    }

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
        self.upload(width, height, rgba, 1, PixelFormat::Rgba8)
    }

    fn upload_image_mipmapped(&mut self, width: u32, height: u32, rgba: &[u8]) -> AureaResult<u32> {
        self.upload(
            width,
            height,
            rgba,
            mip_levels(width, height),
            PixelFormat::Rgba8,
        )
    }

    fn evict_image(&mut self, shader_slot: u32) {
        if let Some(texture) = self.slot_textures.remove(&shader_slot) {
            self.context.destroy_image(texture);
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
            path_clips: self.context.device().capabilities().compute
                && self
                    .context
                    .device()
                    .format_caps(Format::Rgba8Unorm)
                    .storage,
            rounded_rects: true,
            stroked_circles: true,
            ..GpuFeatures::NONE
        }
    }

    fn supports_dual_source(&self) -> bool {
        self.context.device().supports_dual_source_blending()
    }

    fn accepts_pixel_format(&self, format: PixelFormat) -> bool {
        matches!(format, PixelFormat::Rgba8 | PixelFormat::Bgra8)
    }

    fn present_pixels(
        &mut self,
        pixels: &[u8],
        format: PixelFormat,
        width: u32,
        height: u32,
    ) -> AureaResult<FrameOutcome> {
        let reuse = self
            .cpu_frame
            .as_ref()
            .filter(|f| f.width == width && f.height == height && f.format == format)
            .map(|f| f.slot);
        let slot = match reuse {
            Some(slot) => {
                let texture = self
                    .slot_textures
                    .get(&slot)
                    .copied()
                    .ok_or(AureaError::RenderingFailed)?;
                upload_levels(&self.context, texture, width, height, pixels, 1, Mips::Cpu)
                    .map_err(gpu_err)?;
                slot
            }
            None => {
                if let Some(old) = self.cpu_frame.take() {
                    self.evict_image(old.slot);
                }
                let slot = self.upload(width, height, pixels, 1, format)?;
                self.cpu_frame = Some(CpuFrame {
                    slot,
                    width,
                    height,
                    format,
                });
                slot
            }
        };
        let mut plan = FramePlan::new();
        full_frame_plan(&mut plan, width, height, slot);
        self.present_frame(&plan, &[], &[])
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
            if plan.path_clips.iter().any(Option::is_some) {
                pipelines
                    .ensure_clip_mask(self.context.device())
                    .map_err(gpu_err)?;
            }
            *pipelines
        };

        let vw = plan.viewport_width as f32;
        let vh = plan.viewport_height as f32;
        let device = self.context.device();
        self.clip_masks
            .prepare(device, plan, pipelines.clip_mask)
            .map_err(gpu_err)?;

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
        self.clip_masks.destroy(device);
        if let Target::Offscreen(offscreen) = &mut self.target {
            offscreen.release(device);
        }
        for (_, texture) in self.slot_textures.drain() {
            self.context.destroy_image(texture);
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

pub(super) fn as_bytes<T: Copy>(slice: &[T]) -> &[u8] {
    unsafe { from_raw_parts(slice.as_ptr() as *const u8, size_of_val(slice)) }
}

#[cfg(test)]
mod context_tests {
    use super::*;
    use crate::cpu::CpuRasterizer;
    use crate::renderer::{DrawingContext, Renderer};
    use crate::surface::{Surface as RenderSurface, SurfaceInfo};
    use crate::types::{BlendMode, Color, Paint, PaintStyle, Path, PathCommand, Point};

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
    #[allow(clippy::cognitive_complexity)] // Pixel oracle and resource reuse share the same scene.
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

        let shown = gpu.frame_output().expect("shown").words().to_vec();
        assert!(gpu.backend().rect_buf.reserved_bytes() > 0);
        assert!(gpu.backend().image_buf.reserved_bytes() > 0);
        gpu.trim_memory();
        assert_eq!(gpu.backend().rect_buf.reserved_bytes(), 0);
        assert_eq!(gpu.backend().image_buf.reserved_bytes(), 0);
        assert_eq!(gpu.backend().image_instances.capacity(), 0);
        assert_eq!(gpu.frame_output().expect("kept frame").words(), shown);
        {
            let mut ctx = gpu.begin_frame().expect("after trim");
            scene(&mut *ctx, &image);
            ctx.draw_rect(
                Rect::new(0.0, 0.0, 4.0, 4.0),
                &Paint::new().color(Color::rgb(255, 0, 0)),
            )
            .expect("changed frame");
        }
        assert_eq!(gpu.end_frame().expect("rebuilt"), FrameOutcome::Presented);
        assert_eq!(
            gpu.frame_output().expect("new frame").words()[0],
            0xFFFF0000
        );
        assert!(gpu.backend().rect_buf.reserved_bytes() > 0);
        assert!(gpu.backend().image_buf.reserved_bytes() > 0);
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

    /// `area_downsample`'s chain down to one texel, as `upload_levels` builds it on the CPU.
    fn cpu_chain(width: u32, height: u32, rgba: &[u8], levels: u32) -> Vec<Vec<u8>> {
        let mut chain = vec![rgba.to_vec()];
        let (mut w, mut h) = (width, height);
        for _ in 1..levels {
            let (dw, dh) = ((w >> 1).max(1), (h >> 1).max(1));
            let next = area_downsample(chain.last().expect("level"), (w, h), (dw, dh));
            chain.push(next);
            (w, h) = (dw, dh);
        }
        chain
    }

    fn read_mip(
        device: &NativeDevice,
        texture: TextureHandle,
        level: u32,
        w: u32,
        h: u32,
    ) -> Vec<u8> {
        use zengpu_hal::{BufferDesc, BufferUsage, MemoryUsage};
        let len = u64::from(w) * u64::from(h) * 4;
        let buffer = device
            .create_buffer(BufferDesc {
                size: len,
                usage: BufferUsage::READBACK | BufferUsage::TRANSFER_DST,
                memory: MemoryUsage::Readback,
            })
            .expect("buffer");
        let region = TextureRegion {
            mip_level: level,
            ..TextureRegion::rect(0, 0, w, h)
        };
        device
            .copy_texture_region_to_buffer(texture, region, buffer, 0, w * 4)
            .expect("copy")
            .wait(Duration::MAX)
            .expect("wait");
        let bytes = device.read_buffer(buffer, 0, len).expect("read");
        device.destroy_buffer(buffer);
        bytes
    }

    /// Builds `rgba`'s mips on the GPU and checks each level byte for byte against the CPU chain.
    fn assert_gpu_mips_match_cpu(
        context: &Arc<ZenGpuContext>,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) {
        let image = Image::new(width, height, rgba);
        let owned = context
            .upload_image(&image, Sampling::Smooth)
            .expect("upload");
        assert!(matches!(owned.mips, Mips::Gpu(_)), "mips built on the GPU");
        let expected = cpu_chain(width, height, &image.data, owned.levels);
        let (mut w, mut h) = (width, height);
        for (level, want) in expected.iter().enumerate() {
            let level = u32::try_from(level).expect("mip level");
            let got = read_mip(context.device(), owned.texture().zengpu(), level, w, h);
            // The driver fuses the GPU's `sum + p * a` into an FMA, which can move an exact
            // rounding tie by one step. The CPU chain is the reference, so only ±1 is allowed.
            let max_diff = got
                .iter()
                .zip(want)
                .map(|(g, c)| g.abs_diff(*c))
                .max()
                .unwrap_or(0);
            assert!(
                max_diff <= 1,
                "{width}x{height} mip {level}: max diff {max_diff} from the CPU"
            );
            (w, h) = ((w >> 1).max(1), (h >> 1).max(1));
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn gpu_mips_match_the_cpu_chain() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        if matches!(context.mip_path(Format::Rgba8Unorm, 2), Mips::Cpu) {
            return;
        }
        let (w, h) = (64u32, 64u32);
        let gradient: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [(x * 4) as u8, (y * 4) as u8, ((x + y) * 2) as u8, 255]
            })
            .collect();
        assert_gpu_mips_match_cpu(&context, w, h, gradient);

        let (w, h) = (37u32, 23u32);
        let edges: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                let a = if (x / 5 + y / 4) % 3 == 0 {
                    0
                } else {
                    ((x * 37 + y * 91) % 256) as u8
                };
                [
                    ((x * 7 + 3) % 256) as u8,
                    (y * 11 % 256) as u8,
                    (x * y % 256) as u8,
                    a,
                ]
            })
            .collect();
        assert_gpu_mips_match_cpu(&context, w, h, edges);

        let column: Vec<u8> = (0..8u32)
            .flat_map(|y| {
                [
                    (y * 30) as u8,
                    200 - (y * 20) as u8,
                    7,
                    (y * 45 % 256) as u8,
                ]
            })
            .collect();
        assert_gpu_mips_match_cpu(&context, 1, 8, column);

        let mut seed = 0x2545_f491u32;
        for (w, h) in [(101u32, 77u32), (255, 3), (1000, 999), (513, 257)] {
            let noise: Vec<u8> = (0..w * h * 4)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    (seed >> 11) as u8
                })
                .collect();
            assert_gpu_mips_match_cpu(&context, w, h, noise);
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn many_gpu_mipmapped_images_stay_on_the_gpu() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        if matches!(context.mip_path(Format::Rgba8Unorm, 2), Mips::Cpu) {
            return;
        }
        let before = GPU_MIP_FALLBACKS.load(Ordering::Relaxed);
        let (w, h) = (512u32, 512u32);
        let images: Vec<ZenGpuImage> = (0..100u32)
            .map(|n| {
                let data: Vec<u8> = (0..w * h)
                    .flat_map(|i| {
                        let (x, y) = (i % w, i / w);
                        [(x + n) as u8, (y * 2) as u8, n as u8, 255]
                    })
                    .collect();
                let owned = context
                    .upload_image(&Image::new(w, h, data), Sampling::Smooth)
                    .expect("upload");
                assert!(matches!(owned.mips, Mips::Gpu(_)), "image {n} on the GPU");
                owned
            })
            .collect();
        assert_eq!(images.len(), 100);
        assert_eq!(
            GPU_MIP_FALLBACKS.load(Ordering::Relaxed),
            before,
            "no image fell back to CPU mips"
        );
    }

    /// Wall time of building a 4096x4096 mip chain on the CPU and on the GPU, upload call only.
    /// Run: `cargo test -p aurea-render --lib --features zengpu -- --ignored --nocapture gpu_mip_timing`.
    #[test]
    #[ignore = "timing"]
    #[allow(clippy::cast_possible_truncation)]
    fn gpu_mip_timing() {
        use std::time::Instant;
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        let (w, h) = (4096u32, 4096u32);
        let levels = mip_levels(w, h);
        let Mips::Gpu(pipeline) = context.mip_path(Format::Rgba8Unorm, levels) else {
            eprintln!("no GPU mips on this device");
            return;
        };
        let rgba: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8, 255]
            })
            .collect();
        for round in 0..10 {
            let start = Instant::now();
            let cpu =
                make_image_texture(&context, w, h, &rgba, levels, Format::Rgba8Unorm, Mips::Cpu)
                    .expect("cpu upload");
            let cpu_time = start.elapsed();
            context.destroy_image(cpu);

            let start = Instant::now();
            let gpu = make_image_texture(
                &context,
                w,
                h,
                &rgba,
                levels,
                Format::Rgba8Unorm,
                Mips::Gpu(pipeline),
            )
            .expect("gpu upload");
            let gpu_time = start.elapsed();
            context.destroy_image(gpu);
            eprintln!("round {round}: 4096^2 cpu mips {cpu_time:?}, gpu mips {gpu_time:?}");
        }
    }

    #[test]
    #[ignore = "manual comparison of per-mip waits and a batched mip chain"]
    fn gpu_mip_batch_timing() {
        use std::time::Instant;
        let context = Arc::new(ZenGpuContext::new().expect("GPU context"));
        let device = context.device();
        for side in [256u32, 1024] {
            let rgba = vec![173; side as usize * side as usize * 4];
            let levels = mip_levels(side, side);
            let Mips::Gpu(pipeline) = context.mip_path(Format::Rgba8Unorm, levels) else {
                eprintln!("no GPU mip path");
                return;
            };
            let texture = make_image_texture(
                &context,
                side,
                side,
                &rgba,
                levels,
                Format::Rgba8Unorm,
                Mips::Gpu(pipeline),
            )
            .expect("texture");
            for round in 0..5 {
                for reference in [true, false] {
                    let started = Instant::now();
                    for _ in 0..32 {
                        if reference {
                            device.upload_texture_data(texture, &rgba).expect("base");
                            let (mut w, mut h) = (side, side);
                            for mip in 1..levels {
                                let (dw, dh) = ((w >> 1).max(1), (h >> 1).max(1));
                                let images = [
                                    device.storage_level(texture, mip - 1).expect("source"),
                                    device.storage_level(texture, mip).expect("dest"),
                                ];
                                let scalars = [
                                    Scalar::F32(w as f32 / dw as f32),
                                    Scalar::F32(h as f32 / dh as f32),
                                ];
                                device
                                    .dispatch(
                                        pipeline,
                                        Bindings {
                                            images: &images,
                                            scalars: &scalars,
                                            ..Default::default()
                                        },
                                        [dw.div_ceil(8), dh.div_ceil(8), 1],
                                    )
                                    .expect("mip");
                                (w, h) = (dw, dh);
                            }
                        } else {
                            gpu_mip_chain(device, pipeline, texture, side, side, &rgba, levels)
                                .expect("batched mips")
                                .work
                                .wait(Duration::MAX)
                                .expect("mip completion");
                        }
                        device
                            .release_storage_levels(texture)
                            .expect("release level views");
                    }
                    eprintln!(
                        "{side}x{side} round {round} reference={reference}: {:.3} ms/chain",
                        started.elapsed().as_secs_f64() * 1000.0 / 32.0
                    );
                }
            }
            context.destroy_image(texture);
        }
    }

    #[test]
    fn pending_image_drop_and_polling_never_wait_for_its_submission() {
        use std::sync::atomic::AtomicBool;
        use zengpu_hal::GpuSubmission;
        struct HeldUpload(Arc<AtomicBool>);
        impl GpuSubmission for HeldUpload {
            fn cycle_id(&self) -> u64 {
                0
            }
            fn poll(&self) -> Result<SubmissionStatus, GpuError> {
                Ok(if self.0.load(Ordering::Acquire) {
                    SubmissionStatus::Complete
                } else {
                    SubmissionStatus::Pending
                })
            }
            fn wait(&self, _: Duration) -> Result<(), GpuError> {
                panic!("upload polling waited")
            }
        }
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        let owned = context
            .upload_image(&Image::new(2, 2, vec![255; 16]), Sampling::Nearest)
            .expect("image");
        context.device().wait_idle().expect("initial upload");
        context.poll_uploads().expect("collect initial upload");
        let texture = owned.texture().zengpu();
        let done = Arc::new(AtomicBool::new(false));
        context.mip_jobs.lock().expect("jobs").insert(
            texture,
            PendingMip {
                work: Box::new(HeldUpload(Arc::clone(&done))),
                dependencies: Vec::new(),
                destroy: false,
            },
        );
        assert!(!owned.upload_complete().expect("pending"));
        drop(owned);
        assert_eq!(context.poll_uploads().expect("no wait"), 1);
        assert!(context.mip_jobs.lock().expect("jobs")[&texture].destroy);
        done.store(true, Ordering::Release);
        assert_eq!(context.poll_uploads().expect("retire completed image"), 0);
        let sampler = context
            .device()
            .create_sampler(SamplerDesc::default())
            .expect("sampler");
        assert!(context.device().bind_texture(texture, sampler).is_none());
        context.device().destroy_sampler(sampler);
    }

    #[test]
    fn queued_mip_updates_are_ordered_and_keep_the_latest_pixels() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        let mut image = context
            .upload_image(&Image::new(32, 16, vec![0; 32 * 16 * 4]), Sampling::Smooth)
            .expect("image");
        for value in 0..40u8 {
            image
                .update(&Image::new(32, 16, [value, 23, 91, 255].repeat(32 * 16)))
                .expect("queued update");
        }
        for level in 0..image.levels {
            let (w, h) = ((32 >> level).max(1), (16 >> level).max(1));
            assert_eq!(
                read_mip(context.device(), image.texture().zengpu(), level, w, h),
                [39, 23, 91, 255].repeat(w as usize * h as usize)
            );
        }
        assert!(
            image
                .upload_complete()
                .expect("readback completed all earlier uploads")
        );
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
        context.device().wait_idle().expect("finish queued upload");
        context.poll_uploads().expect("retire dropped image");
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
        context.destroy_image(texture);
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

    #[test]
    fn an_unchanged_offscreen_frame_is_not_submitted_again() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let mut gpu = ZenGpuRenderer::offscreen(Arc::new(context), 64, 48, 1.0).unwrap();
        let frame = |gpu: &mut ZenGpuRenderer, x: f32| {
            {
                let mut ctx = gpu.begin_frame().unwrap();
                ctx.clear(Color::rgb(20, 24, 30)).unwrap();
                ctx.draw_rect(
                    Rect::new(x, 4.0, 22.0, 9.0),
                    &Paint::new().color(Color::rgb(200, 60, 40)),
                )
                .unwrap();
            }
            gpu.end_frame().unwrap()
        };
        assert_eq!(frame(&mut gpu, 3.0), FrameOutcome::Presented);
        let first = gpu.frame_output().expect("first pixels").words().to_vec();
        assert_eq!(frame(&mut gpu, 3.0), FrameOutcome::Unchanged);
        assert_eq!(gpu.frame_output().expect("kept pixels").words(), &first[..]);
        assert_eq!(frame(&mut gpu, 5.0), FrameOutcome::Presented);
        assert_ne!(gpu.frame_output().expect("new pixels").words(), &first[..]);
    }

    #[test]
    #[allow(clippy::cognitive_complexity)] // Compare all painter pipelines against the same clip oracle.
    fn native_path_clips_cover_every_painter_pipeline() {
        use crate::types::{Font, GradientStop, LinearGradient};
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let context = Arc::new(context);
        let image = Image::new(2, 2, [210, 160, 90, 180].repeat(4));
        let texture = context.upload_image(&image, Sampling::Nearest).unwrap();
        let mut triangle = Path::new();
        triangle.commands.extend([
            PathCommand::MoveTo(Point::new(2.3, 2.1)),
            PathCommand::LineTo(Point::new(61.7, 3.4)),
            PathCommand::LineTo(Point::new(4.1, 61.2)),
            PathCommand::Close,
        ]);
        let rounded = Path::rounded_rect(Rect::new(6.0, 5.0, 49.0, 49.0), 9.0);
        let mut coverage = vec![255u8; 64 * 64];
        for path in [&triangle, &rounded] {
            let mask =
                CpuRasterizer::path_coverage(path, &Paint::new(), Point::new(0.0, 0.0), (64, 64))
                    .unwrap();
            for (a, b) in coverage.iter_mut().zip(mask.as_chunks::<4>().0) {
                *a = u8::try_from((u32::from(*a) * u32::from(b[3]) + 127) / 255).unwrap();
            }
        }
        let draw = |ctx: &mut dyn DrawingContext, kind| {
            let rect = Rect::new(0.0, 0.0, 64.0, 64.0);
            let paint = Paint::new().color(Color::rgba(210, 160, 90, 180));
            match kind {
                0 => ctx.draw_rect(rect, &paint),
                1 => ctx.draw_round_rect(Rect::new(1.0, 1.0, 61.0, 61.0), 12.0, &paint),
                2 => ctx.fill_linear_gradient(
                    &LinearGradient {
                        start: Point::new(0.0, 0.0),
                        end: Point::new(64.0, 0.0),
                        stops: vec![
                            GradientStop {
                                offset: 0.0,
                                color: Color::rgb(240, 90, 40),
                            },
                            GradientStop {
                                offset: 1.0,
                                color: Color::rgba(60, 210, 150, 160),
                            },
                        ],
                    },
                    rect,
                ),
                3 => ctx.draw_image_rect(&image, rect),
                4 => ctx.draw_texture(texture.texture(), Rect::new(0.0, 0.0, 1.0, 1.0), rect),
                _ => ctx.draw_text_with_font(
                    "MMMM",
                    Point::new(1.0, 34.0),
                    &Font::new("Segoe UI", 28.0),
                    &paint,
                ),
            }
            .unwrap();
        };
        let mut reference = ZenGpuRenderer::offscreen(Arc::clone(&context), 64, 64, 1.0).unwrap();
        let mut clipped = ZenGpuRenderer::offscreen(context, 64, 64, 1.0).unwrap();
        for kind in 0..6 {
            {
                let mut ctx = reference.begin_frame().unwrap();
                ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                draw(&mut *ctx, kind);
            }
            reference.end_frame().unwrap();
            {
                let mut ctx = clipped.begin_frame().unwrap();
                ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                ctx.save().unwrap();
                ctx.clip_path(&triangle).unwrap();
                // Clip geometry remains in device space after changing the transform again.
                ctx.translate(6.0, 5.0).unwrap();
                ctx.clip_path(&Path::rounded_rect(Rect::new(0.0, 0.0, 49.0, 49.0), 9.0))
                    .unwrap();
                ctx.translate(-6.0, -5.0).unwrap();
                draw(&mut *ctx, kind);
                ctx.restore().unwrap();
                ctx.draw_rect(
                    Rect::new(60.0, 60.0, 4.0, 4.0),
                    &Paint::new().color(Color::rgb(255, 0, 0)),
                )
                .unwrap();
            }
            assert_eq!(clipped.end_frame().unwrap(), FrameOutcome::Presented);
            assert!(
                clipped.backend().cpu_frame.is_none(),
                "kind {kind} fell back to CPU"
            );
            let (plain, got) = (
                reference.frame_output().unwrap(),
                clipped.frame_output().unwrap(),
            );
            let mut lit = 0;
            for (i, ((&plain, &got), &mask)) in plain
                .words()
                .iter()
                .zip(got.words())
                .zip(&coverage)
                .enumerate()
            {
                if i % 64 >= 60 && i / 64 >= 60 {
                    assert_eq!(got, 0xFFFF0000, "restore must remove both clips");
                    continue;
                }
                for shift in [0, 8, 16] {
                    let expected = (((plain >> shift) & 255) * u32::from(mask) + 127) / 255;
                    let actual = (got >> shift) & 255;
                    assert!(
                        actual.abs_diff(expected) <= 2,
                        "kind {kind}, pixel {i}, channel {shift}: {actual} != {expected}"
                    );
                    lit += usize::from(actual != 0);
                }
            }
            assert!(lit > 30, "kind {kind} produced no clipped content");
        }
    }

    #[test]
    #[allow(clippy::cognitive_complexity)] // Verify pixels and storage across fallback and trimming.
    fn a_cpu_frame_reuses_its_texture_and_keeps_its_colours() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let mut gpu = ZenGpuRenderer::offscreen(Arc::new(context), 64, 48, 1.0).unwrap();
        let triangle = {
            let mut path = Path::new();
            path.commands.extend([
                PathCommand::MoveTo(Point::new(0.0, 0.0)),
                PathCommand::LineTo(Point::new(80.0, 0.0)),
                PathCommand::LineTo(Point::new(0.0, 40.0)),
                PathCommand::Close,
            ]);
            path
        };
        // Non-normal blending still selects CPU fallback; path clips now run on the GPU.
        let frame = |gpu: &mut ZenGpuRenderer, x: f32| {
            {
                let mut ctx = gpu.begin_frame().unwrap();
                ctx.clear(Color::rgb(255, 255, 255)).unwrap();
                ctx.clip_path(&triangle).unwrap();
                ctx.set_blend_mode(BlendMode::Multiply).unwrap();
                ctx.draw_rect(
                    Rect::new(x, 0.0, 40.0, 40.0),
                    &Paint::new().color(Color::rgb(200, 60, 40)),
                )
                .unwrap();
            }
            gpu.end_frame().unwrap()
        };
        let centre =
            |gpu: &ZenGpuRenderer| gpu.frame_output().expect("pixels").words()[20 * 64 + 23];
        assert_eq!(frame(&mut gpu, 3.0), FrameOutcome::Presented);
        let slot = gpu.backend().cpu_frame.as_ref().expect("cpu frame").slot;
        assert_eq!(centre(&gpu), 0xFFC8_3C28);
        assert_eq!(frame(&mut gpu, 3.0), FrameOutcome::Unchanged);
        assert_eq!(frame(&mut gpu, 5.0), FrameOutcome::Presented);
        assert_eq!(
            gpu.backend().cpu_frame.as_ref().expect("cpu frame").slot,
            slot
        );
        assert_eq!(gpu.backend().slot_textures.len(), 1);
        assert_eq!(centre(&gpu), 0xFFC8_3C28);
        let shown = gpu.frame_output().expect("shown").words().to_vec();
        gpu.trim_memory();
        assert!(gpu.backend().cpu_frame.is_none());
        assert!(gpu.backend().slot_textures.is_empty());
        assert_eq!(gpu.backend().image_buf.reserved_bytes(), 0);
        assert_eq!(gpu.frame_output().expect("kept frame").words(), shown);
        assert_eq!(frame(&mut gpu, 5.0), FrameOutcome::Unchanged);
        assert_eq!(frame(&mut gpu, 7.0), FrameOutcome::Presented);
        assert!(gpu.backend().cpu_frame.is_some());
        assert_eq!(centre(&gpu), 0xFFC8_3C28);
    }
}
