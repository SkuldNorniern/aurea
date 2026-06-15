//! ZenGPU 2D renderer backend (feature `zengpu`).
//!
//! Implements [`Renderer`] by recording draw calls into a [`DisplayList`] (the
//! same [`CpuDrawingContext`] the CPU rasterizer uses), then in `end_frame`
//! lowering the list to [`RenderBatches`] and presenting it through ZenGPU's
//! `Vulkan2dSurface`.
//!
//! Unlike [`crate::cpu::CpuRasterizer`], this backend presents **directly** to
//! its own swapchain on the supplied window — it does not publish a CPU
//! framebuffer for the platform to blit. It can target either a top-level
//! window or a native canvas child surface owned by the platform compositor.
//!
//! Images and gradient LUTs are uploaded to GPU textures **once**, cached by
//! their Arc-backed pixel identity, and bound into the painter's shared
//! bindless slots. The cache evicts least-recently-used entries when slots fill.

use std::collections::HashMap;
use std::sync::Arc;

use crate::batch::{ImageDraw, RenderBatches};
use crate::gpu2d::{Gpu2dBackend, Gpu2dRenderer};
use crate::types::Rect;
use aurea_foundation::{AureaError, AureaResult};

use zengpu_hal::{
    DeviceRequest, FilterMode, Format, GpuDevice, PresentMode, SamplerDesc, SamplerHandle,
    SurfaceConfig, TextureDesc, TextureHandle, TextureUsage, WindowHandles,
};
use zengpu_vulkan::instance::VulkanInstance;
use zengpu_vulkan::{SampledImageView, VulkanDevice};

use crate::zengpu_surface::{
    CircleInstance as VkCircle, DrawRef as VkDrawRef, Frame2d, GradientInstance as VkGradient,
    ImageInstance as VkImage, RectInstance as VkRect, TextInstance as VkText, Vulkan2dSurface,
};

// The batch-layer and ZenGPU instance types are `#[repr(C)]` with identical
// fields, so a frame's primitives can be reinterpreted from one to the other
// with no per-frame copy. Guard the layout assumptions.
const _: () =
    assert!(std::mem::size_of::<crate::batch::RectInstance>() == std::mem::size_of::<VkRect>());
const _: () =
    assert!(std::mem::size_of::<crate::batch::CircleInstance>() == std::mem::size_of::<VkCircle>());
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ImageKey {
    data_ptr: usize,
    data_len: usize,
    width: u32,
    height: u32,
}

impl ImageKey {
    fn from_pixels(data: &Arc<[u8]>, width: u32, height: u32) -> Self {
        Self {
            data_ptr: data.as_ptr() as usize,
            data_len: data.len(),
            width,
            height,
        }
    }
}

/// A cached GPU texture holding its bindless slot. `keepalive` retains the
/// source pixels so the pointer used as the cache key stays valid.
struct CachedImage {
    texture: TextureHandle,
    slot: u32,
    keepalive: Arc<[u8]>,
    last_used: u64,
}

struct ExternalImageDraw {
    instance: VkImage,
    slot: u32,
}

/// Shareable ZenGPU instance/device ownership for Aurea UI and engine rendering.
pub struct ZenGpuContext {
    instance: VulkanInstance,
    device: VulkanDevice,
}

impl ZenGpuContext {
    /// Create a shareable Vulkan instance/device pair with presentation support.
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

    /// Vulkan instance used to create window surfaces.
    pub fn instance(&self) -> &VulkanInstance {
        &self.instance
    }

    /// Shared logical device used by Aurea and engine-side rendering.
    pub fn device(&self) -> &VulkanDevice {
        &self.device
    }

    /// Cloneable raw graphics context for offscreen targets, depth targets,
    /// frame graphs, and other engine-side Vulkan resources.
    pub fn device_context(&self) -> zengpu_vulkan::DeviceContext {
        self.device.context()
    }
}

/// ZenGPU device backend for the shared [`Gpu2dRenderer`] core: owns the
/// swapchain surface, the image/LUT cache, bindless slots, and per-frame resolve
/// buffers. The core above it owns the display list, batching, and the
/// `Renderer` shell. The public renderer is [`ZenGpuRenderer`].
pub struct ZenGpuBackend {
    // The surface must drop before the shared context. Renderer-owned textures
    // and the sampler are explicitly released in Drop because the context may
    // outlive this renderer.
    surface: Vulkan2dSurface,
    context: Arc<ZenGpuContext>,
    /// Shared image/gradient-LUT cache keyed by source pixel identity.
    texture_cache: HashMap<ImageKey, CachedImage>,
    /// Bindless slots not currently assigned to a cached texture.
    free_slots: Vec<u32>,
    /// Shared sampler for all cached image textures.
    sampler: SamplerHandle,
    /// Monotonic frame counter for LRU eviction.
    frame_counter: u64,
    /// Reused per-frame buffer of resolved image instances.
    vk_images: Vec<VkImage>,
    /// Caller-owned GPU images appended after the ordinary display list.
    external_images: Vec<ExternalImageDraw>,
    /// Reused per-frame buffer of gradients with resolved LUT slots.
    vk_gradients: Vec<VkGradient>,
    /// Reused per-frame buffer of text masks with resolved texture slots.
    vk_texts: Vec<VkText>,
    /// Reused cross-kind painter-order stream.
    vk_order: Vec<VkDrawRef>,
}

/// Public renderer: aurea's [`Renderer`](crate::Renderer) on the ZenGPU device
/// backend. The shared [`Gpu2dRenderer`] core owns the display list and
/// batching; [`ZenGpuBackend`] owns the device-specific draw path.
pub type ZenGpuRenderer = Gpu2dRenderer<ZenGpuBackend>;

impl ZenGpuRenderer {
    /// Create a renderer presenting to the window described by `handles`.
    /// `width`/`height` are the logical surface size; `scale_factor` maps to
    /// physical pixels (matching [`CpuRasterizer`](crate::cpu::CpuRasterizer)).
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

    /// Create a renderer on a caller-owned GPU context. Multiple Aurea
    /// renderers and engine viewports can share this context and its device.
    pub fn with_context(
        handles: &WindowHandles,
        context: Arc<ZenGpuContext>,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> AureaResult<Self> {
        let scale = scale_factor.max(1.0);
        let config = SurfaceConfig {
            format: Format::Bgra8Unorm,
            width: ((width as f32 * scale).round() as u32).max(1),
            height: ((height as f32 * scale).round() as u32).max(1),
            present_mode: PresentMode::Fifo,
        };
        let surface = Vulkan2dSurface::new(context.device(), handles, config).map_err(gpu_err)?;

        // Shared sampler for image textures (linear min/mag, clamp).
        let sampler = context
            .device()
            .create_sampler(SamplerDesc {
                min_filter: FilterMode::Linear,
                mag_filter: FilterMode::Linear,
                ..SamplerDesc::default()
            })
            .map_err(gpu_err)?;
        let slots = surface.image_slot_capacity();
        let free_slots: Vec<u32> = (0..slots).rev().collect();

        let backend = ZenGpuBackend {
            surface,
            context,
            texture_cache: HashMap::new(),
            free_slots,
            sampler,
            frame_counter: 0,
            vk_images: Vec::new(),
            external_images: Vec::new(),
            vk_gradients: Vec::new(),
            vk_texts: Vec::new(),
            vk_order: Vec::new(),
        };
        Ok(Gpu2dRenderer::from_backend(backend, width, height, scale))
    }

    /// Swapchain extent in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        self.backend().size()
    }

    /// Shared context backing this renderer.
    pub fn context(&self) -> &Arc<ZenGpuContext> {
        self.backend().context()
    }

    /// Draw a caller-owned GPU image after the ordinary Aurea display list.
    ///
    /// The image must come from the same [`ZenGpuContext`], remain alive
    /// through `Renderer::end_frame`, and already be in a shader-readable layout.
    pub fn draw_sampled_image(
        &mut self,
        image: SampledImageView<'_>,
        dest: Rect,
    ) -> AureaResult<()> {
        self.backend_mut().draw_sampled_image(image, dest)
    }

    /// Remove all caller-owned sampled images from this renderer.
    pub fn clear_sampled_images(&mut self) -> AureaResult<()> {
        self.backend_mut().clear_sampled_images()
    }
}

impl ZenGpuBackend {
    /// Swapchain extent in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        self.surface.size()
    }

    /// Shared context backing this backend.
    pub fn context(&self) -> &Arc<ZenGpuContext> {
        &self.context
    }

    /// Draw a caller-owned GPU image after the ordinary Aurea display list.
    ///
    /// The image must come from the same [`ZenGpuContext`], remain alive
    /// through [`Renderer::end_frame`], and already be transitioned to
    /// `SHADER_READ_ONLY_OPTIMAL`.
    pub fn draw_sampled_image(
        &mut self,
        image: SampledImageView<'_>,
        dest: Rect,
    ) -> AureaResult<()> {
        let (slot, evicted_key) = reserve_external_slot(&self.texture_cache, &mut self.free_slots)?;
        if let Err(error) =
            self.surface
                .set_sampled_image_slot(self.context.device(), slot, image, self.sampler)
        {
            if evicted_key.is_none() {
                self.free_slots.push(slot);
            }
            return Err(gpu_err(error));
        }
        if let Some(key) = evicted_key {
            let old = self
                .texture_cache
                .remove(&key)
                .expect("external slot eviction key came from cache");
            self.context.device().destroy_texture(old.texture);
        }
        self.external_images.push(ExternalImageDraw {
            instance: VkImage {
                rect: [dest.x, dest.y, dest.width, dest.height],
                uv: [0.0, 0.0, 1.0, 1.0],
                tint: [1.0; 4],
                slot,
                _pad: [0; 3],
            },
            slot,
        });
        Ok(())
    }

    /// Remove all caller-owned sampled images from this renderer.
    ///
    /// Call this before destroying their backing targets when no subsequent
    /// [`Renderer::begin_frame`] will release the slots automatically.
    pub fn clear_sampled_images(&mut self) -> AureaResult<()> {
        self.release_external_images()
    }

    fn release_external_images(&mut self) -> AureaResult<()> {
        for draw in self.external_images.drain(..) {
            self.surface.clear_image_slot(draw.slot).map_err(gpu_err)?;
            self.free_slots.push(draw.slot);
        }
        Ok(())
    }
}

impl Gpu2dBackend for ZenGpuBackend {
    fn resize(&mut self, physical_width: u32, physical_height: u32) -> AureaResult<()> {
        self.surface
            .resize(physical_width, physical_height)
            .map_err(gpu_err)
    }

    fn begin_frame(&mut self) -> AureaResult<()> {
        self.release_external_images()
    }

    fn present(&mut self, batches: &RenderBatches) -> AureaResult<()> {
        let clear = batches.clear.map(|c| {
            [
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
                c.a as f32 / 255.0,
            ]
        });
        // Zero-copy reinterpret: layout identity is asserted at the top of the
        // module, so the batch primitives upload directly with no per-frame Vec.
        let rects: &[VkRect] = unsafe {
            std::slice::from_raw_parts(
                batches.rects.as_ptr() as *const VkRect,
                batches.rects.len(),
            )
        };
        let circles: &[VkCircle] = unsafe {
            std::slice::from_raw_parts(
                batches.circles.as_ptr() as *const VkCircle,
                batches.circles.len(),
            )
        };
        // Gradients and images need GPU textures resolved (created/cached and
        // bound), so they are built into reused Vecs rather than cast.
        self.frame_counter += 1;
        prune_dropped_images(
            self.context.device(),
            &self.surface,
            &mut self.texture_cache,
            &mut self.free_slots,
        )?;
        resolve_gradients(
            &batches.gradients,
            self.context.device(),
            &self.surface,
            self.sampler,
            &mut self.texture_cache,
            &mut self.free_slots,
            self.frame_counter,
            &mut self.vk_gradients,
        )?;
        resolve_images(
            &batches.images,
            self.context.device(),
            &self.surface,
            self.sampler,
            &mut self.texture_cache,
            &mut self.free_slots,
            self.frame_counter,
            &mut self.vk_images,
        )?;
        resolve_texts(
            &batches.texts,
            self.context.device(),
            &self.surface,
            self.sampler,
            &mut self.texture_cache,
            &mut self.free_slots,
            self.frame_counter,
            &mut self.vk_texts,
        )?;
        let external_image_base = self.vk_images.len() as u32;
        self.vk_images
            .extend(self.external_images.iter().map(|draw| draw.instance));
        self.vk_order.clear();
        self.vk_order
            .extend(batches.order.iter().map(|draw| match *draw {
                crate::batch::DrawRef::Rect(index) => VkDrawRef::Rect(index),
                crate::batch::DrawRef::Gradient(index) => VkDrawRef::Gradient(index),
                crate::batch::DrawRef::Image(index) => VkDrawRef::Image(index),
                crate::batch::DrawRef::Text(index) => VkDrawRef::Text(index),
                crate::batch::DrawRef::Circle(index) => VkDrawRef::Circle(index),
            }));
        self.vk_order.extend(
            self.external_images
                .iter()
                .enumerate()
                .map(|(index, _)| VkDrawRef::Image(external_image_base + index as u32)),
        );

        self.surface
            .present(Frame2d {
                clear,
                rects,
                gradients: &self.vk_gradients,
                images: &self.vk_images,
                texts: &self.vk_texts,
                circles,
                order: &self.vk_order,
            })
            .map_err(gpu_err)
    }
}

impl Drop for ZenGpuBackend {
    fn drop(&mut self) {
        let device = self.context.device();
        let _ = device.wait_idle();
        for (_, cached) in self.texture_cache.drain() {
            device.destroy_texture(cached.texture);
        }
        device.destroy_sampler(self.sampler);
    }
}

/// Resolve each `ImageDraw` to a bindless slot (uploading + caching its GPU
/// texture on first sight, evicting the least-recently-used entry when the
/// slots are full), and build the per-image instances into `out`.
///
/// Takes the renderer's fields individually so the borrow checker can see they
/// are disjoint (a `&mut self` helper would conflict with the `&batches`
/// read in `end_frame`).
#[allow(clippy::too_many_arguments)]
fn resolve_images(
    images: &[ImageDraw],
    device: &VulkanDevice,
    surface: &Vulkan2dSurface,
    sampler: SamplerHandle,
    cache: &mut HashMap<ImageKey, CachedImage>,
    free_slots: &mut Vec<u32>,
    frame: u64,
    out: &mut Vec<VkImage>,
) -> AureaResult<()> {
    out.clear();
    for draw in images {
        let (iw, ih) = (draw.image.width, draw.image.height);
        if iw == 0 || ih == 0 {
            continue;
        }
        let expected_len = (iw as usize).saturating_mul(ih as usize).saturating_mul(4);
        if draw.image.data.len() != expected_len {
            return Err(AureaError::RenderingFailed);
        }
        let slot = resolve_texture_slot(
            &draw.image.data,
            iw,
            ih,
            device,
            surface,
            sampler,
            cache,
            free_slots,
            frame,
        )?;

        let (iwf, ihf) = (iw as f32, ih as f32);
        out.push(VkImage {
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

#[allow(clippy::too_many_arguments)]
fn resolve_gradients(
    gradients: &[crate::batch::GradientInstance],
    device: &VulkanDevice,
    surface: &Vulkan2dSurface,
    sampler: SamplerHandle,
    cache: &mut HashMap<ImageKey, CachedImage>,
    free_slots: &mut Vec<u32>,
    frame: u64,
    out: &mut Vec<VkGradient>,
) -> AureaResult<()> {
    out.clear();
    for gradient in gradients {
        let slot = resolve_texture_slot(
            &gradient.lut,
            256,
            1,
            device,
            surface,
            sampler,
            cache,
            free_slots,
            frame,
        )?;
        out.push(VkGradient {
            rect: gradient.rect,
            a: gradient.a,
            b: gradient.b,
            slot,
            _pad: [0; 3],
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_texts(
    texts: &[crate::batch::TextDraw],
    device: &VulkanDevice,
    surface: &Vulkan2dSurface,
    sampler: SamplerHandle,
    cache: &mut HashMap<ImageKey, CachedImage>,
    free_slots: &mut Vec<u32>,
    frame: u64,
    out: &mut Vec<VkText>,
) -> AureaResult<()> {
    out.clear();
    for text in texts {
        let width = text.rect.width as u32;
        let height = text.rect.height as u32;
        if width == 0 || height == 0 {
            continue;
        }
        let slot = resolve_texture_slot(
            &text.mask, width, height, device, surface, sampler, cache, free_slots, frame,
        )?;
        out.push(VkText {
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

#[allow(clippy::too_many_arguments)]
fn resolve_texture_slot(
    pixels: &Arc<[u8]>,
    width: u32,
    height: u32,
    device: &VulkanDevice,
    surface: &Vulkan2dSurface,
    sampler: SamplerHandle,
    cache: &mut HashMap<ImageKey, CachedImage>,
    free_slots: &mut Vec<u32>,
    frame: u64,
) -> AureaResult<u32> {
    let key = ImageKey::from_pixels(pixels, width, height);
    if let Some(cached) = cache.get_mut(&key) {
        cached.last_used = frame;
        return Ok(cached.slot);
    }

    let (slot, evicted_key) = match free_slots.pop() {
        Some(slot) => (slot, None),
        None => {
            let key = *cache
                .iter()
                .filter(|(_, cached)| cached.last_used != frame)
                .min_by_key(|(_, cached)| cached.last_used)
                .map(|(key, _)| key)
                .ok_or(AureaError::RenderingFailed)?;
            (
                cache.get(&key).expect("key came from cache").slot,
                Some(key),
            )
        }
    };
    let texture = match device.create_texture(TextureDesc {
        width,
        height,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        samples: 1,
    }) {
        Ok(texture) => texture,
        Err(error) => {
            if evicted_key.is_none() {
                free_slots.push(slot);
            }
            return Err(gpu_err(error));
        }
    };
    if let Err(error) = device.upload_texture_data(texture, pixels) {
        device.destroy_texture(texture);
        if evicted_key.is_none() {
            free_slots.push(slot);
        }
        return Err(gpu_err(error));
    }
    if let Err(error) = surface.set_image_slot(device, slot, texture, sampler) {
        device.destroy_texture(texture);
        if evicted_key.is_none() {
            free_slots.push(slot);
        }
        return Err(gpu_err(error));
    }
    if let Some(evicted_key) = evicted_key {
        let old = cache
            .remove(&evicted_key)
            .expect("eviction key came from cache");
        device.destroy_texture(old.texture);
    }
    cache.insert(
        key,
        CachedImage {
            texture,
            slot,
            keepalive: Arc::clone(pixels),
            last_used: frame,
        },
    );
    Ok(slot)
}

fn reserve_external_slot(
    cache: &HashMap<ImageKey, CachedImage>,
    free_slots: &mut Vec<u32>,
) -> AureaResult<(u32, Option<ImageKey>)> {
    if let Some(slot) = free_slots.pop() {
        return Ok((slot, None));
    }
    let key = *cache
        .iter()
        .min_by_key(|(_, cached)| cached.last_used)
        .map(|(key, _)| key)
        .ok_or(AureaError::RenderingFailed)?;
    Ok((
        cache.get(&key).expect("key came from cache").slot,
        Some(key),
    ))
}

fn prune_dropped_images(
    device: &VulkanDevice,
    surface: &Vulkan2dSurface,
    cache: &mut HashMap<ImageKey, CachedImage>,
    free_slots: &mut Vec<u32>,
) -> AureaResult<()> {
    while let Some(key) = cache
        .iter()
        .find(|(_, cached)| Arc::strong_count(&cached.keepalive) == 1)
        .map(|(key, _)| *key)
    {
        let cached = cache.get(&key).expect("key came from cache");
        surface.clear_image_slot(cached.slot).map_err(gpu_err)?;
        let cached = cache.remove(&key).expect("key came from cache");
        device.destroy_texture(cached.texture);
        free_slots.push(cached.slot);
    }
    Ok(())
}

fn gpu_err(_e: zengpu_hal::GpuError) -> AureaError {
    AureaError::ElementOperationFailed
}

#[cfg(test)]
mod tests {
    use super::{reserve_external_slot, ImageKey, ZenGpuContext};
    use crate::types::Image;
    use std::collections::HashMap;

    #[test]
    fn shared_context_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ZenGpuContext>();
    }

    #[test]
    fn external_image_prefers_a_free_slot() {
        let mut free_slots = vec![7];
        assert_eq!(
            reserve_external_slot(&HashMap::new(), &mut free_slots).unwrap(),
            (7, None)
        );
    }

    #[test]
    fn image_cache_key_includes_dimensions() {
        let image = Image::new(2, 2, vec![255; 16]);
        let mut reshaped = image.clone();
        reshaped.width = 1;
        reshaped.height = 4;

        assert_ne!(
            ImageKey::from_pixels(&image.data, image.width, image.height),
            ImageKey::from_pixels(&reshaped.data, reshaped.width, reshaped.height)
        );
    }

    #[test]
    fn image_cache_key_is_stable_across_arc_clones() {
        let image = Image::new(2, 2, vec![255; 16]);
        let clone = image.clone();
        assert_eq!(
            ImageKey::from_pixels(&image.data, image.width, image.height),
            ImageKey::from_pixels(&clone.data, clone.width, clone.height)
        );
    }
}
