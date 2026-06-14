//! ZenGPU 2D renderer backend (feature `zengpu`).
//!
//! Implements [`Renderer`] by recording draw calls into a [`DisplayList`] (the
//! same [`CpuDrawingContext`] the CPU rasterizer uses), then in `end_frame`
//! lowering the list to [`RenderBatches`] and presenting it through ZenGPU's
//! `Vulkan2dSurface`.
//!
//! Unlike [`crate::cpu::CpuRasterizer`], this backend presents **directly** to
//! its own swapchain on the supplied window — it does not publish a CPU
//! framebuffer for the platform to blit. It is therefore driven at the window
//! level (the caller owns the window and its handles); wiring it into `Canvas`
//! backend selection is a follow-up that must reconcile the canvas blit path.
//!
//! Images and gradient LUTs are uploaded to GPU textures **once**, cached by
//! their Arc-backed pixel identity, and bound into the painter's shared
//! bindless slots. The cache evicts least-recently-used entries when slots fill.

use std::collections::HashMap;
use std::sync::Arc;

use crate::batch::{ImageDraw, RenderBatches};
use crate::cpu::CpuDrawingContext;
use crate::display_list::DisplayList;
use crate::renderer::{DrawingContext, Renderer};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::Rect;
use aurea_foundation::{AureaError, AureaResult};

use zengpu_hal::{
    DeviceRequest, FilterMode, Format, GpuDevice, PresentMode, SamplerDesc, SamplerHandle,
    SurfaceConfig, TextureDesc, TextureHandle, TextureUsage, WindowHandles,
};
use zengpu_vulkan::instance::VulkanInstance;
use zengpu_vulkan::{
    CircleInstance as VkCircle, Frame2d, GradientInstance as VkGradient, ImageInstance as VkImage,
    RectInstance as VkRect, Vulkan2dSurface, VulkanDevice,
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

/// A [`Renderer`] that lowers the display list to GPU primitives and presents
/// them through ZenGPU's Vulkan backend.
pub struct ZenGpuRenderer {
    // `device` and `_instance` own GPU resources the surface borrows; they must
    // outlive `surface` and are dropped after it (struct field order: surface
    // is declared first so it drops first). Cached textures live in `device`'s
    // slotmap and are freed when it drops.
    surface: Vulkan2dSurface,
    device: VulkanDevice,
    _instance: VulkanInstance,
    display_list: DisplayList,
    /// Reused across frames so steady-state `end_frame` does no allocation.
    batches: RenderBatches,
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
    /// Reused per-frame buffer of gradients with resolved LUT slots.
    vk_gradients: Vec<VkGradient>,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
}

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
        let instance = VulkanInstance::new_with_surface().map_err(gpu_err)?;
        let adapter = instance
            .request_vulkan_adapter()
            .ok_or(AureaError::ElementOperationFailed)?;
        let device = adapter
            .open_with_surface(DeviceRequest::default())
            .map_err(gpu_err)?;

        let scale = scale_factor.max(1.0);
        let config = SurfaceConfig {
            format: Format::Bgra8Unorm,
            width: ((width as f32 * scale).round() as u32).max(1),
            height: ((height as f32 * scale).round() as u32).max(1),
            present_mode: PresentMode::Fifo,
        };
        let surface = instance
            .create_2d_surface(handles, &device, config)
            .map_err(gpu_err)?;

        // Shared sampler for image textures (linear min/mag, clamp).
        let sampler = device
            .create_sampler(SamplerDesc {
                min_filter: FilterMode::Linear,
                mag_filter: FilterMode::Linear,
                ..SamplerDesc::default()
            })
            .map_err(gpu_err)?;
        let slots = surface.image_slot_capacity();
        let free_slots: Vec<u32> = (0..slots).rev().collect();

        Ok(Self {
            surface,
            device,
            _instance: instance,
            display_list: DisplayList::new(),
            batches: RenderBatches::default(),
            texture_cache: HashMap::new(),
            free_slots,
            sampler,
            frame_counter: 0,
            vk_images: Vec::new(),
            vk_gradients: Vec::new(),
            logical_width: width,
            logical_height: height,
            scale_factor: scale,
        })
    }

    /// Swapchain extent in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        self.surface.size()
    }
}

impl Renderer for ZenGpuRenderer {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.logical_width = info.width;
        self.logical_height = info.height;
        self.scale_factor = info.scale_factor.max(1.0);
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.logical_width = width;
        self.logical_height = height;
        self.display_list.clear();
        let scale = self.scale_factor;
        let pw = ((width as f32 * scale).round() as u32).max(1);
        let ph = ((height as f32 * scale).round() as u32).max(1);
        self.surface.resize(pw, ph).map_err(gpu_err)
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        self.display_list.clear();
        let mut ctx = CpuDrawingContext::new(
            &mut self.display_list as *mut DisplayList,
            self.logical_width,
            self.logical_height,
        );
        ctx.set_scale_factor(self.scale_factor);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        self.batches.lower_into(&self.display_list);
        let clear = self.batches.clear.map(|c| {
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
                self.batches.rects.as_ptr() as *const VkRect,
                self.batches.rects.len(),
            )
        };
        let circles: &[VkCircle] = unsafe {
            std::slice::from_raw_parts(
                self.batches.circles.as_ptr() as *const VkCircle,
                self.batches.circles.len(),
            )
        };
        // Gradients and images need GPU textures resolved (created/cached and
        // bound), so they are built into reused Vecs rather than cast.
        self.frame_counter += 1;
        prune_dropped_images(
            &self.device,
            &self.surface,
            &mut self.texture_cache,
            &mut self.free_slots,
        )?;
        resolve_gradients(
            &self.batches.gradients,
            &self.device,
            &self.surface,
            self.sampler,
            &mut self.texture_cache,
            &mut self.free_slots,
            self.frame_counter,
            &mut self.vk_gradients,
        )?;
        resolve_images(
            &self.batches.images,
            &self.device,
            &self.surface,
            self.sampler,
            &mut self.texture_cache,
            &mut self.free_slots,
            self.frame_counter,
            &mut self.vk_images,
        )?;

        self.surface
            .present(Frame2d {
                clear,
                rects,
                gradients: &self.vk_gradients,
                images: &self.vk_images,
                circles,
            })
            .map_err(gpu_err)
    }

    fn cleanup(&mut self) {
        self.display_list.clear();
    }

    fn set_damage(&mut self, _damage: Option<Rect>) {
        // The GPU painter redraws the full frame each present; damage is unused.
    }

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }
}

/// Resolve each `ImageDraw` to a bindless slot (uploading + caching its GPU
/// texture on first sight, evicting the least-recently-used entry when the
/// slots are full), and build the per-image instances into `out`.
///
/// Takes the renderer's fields individually so the borrow checker can see they
/// are disjoint (a `&mut self` helper would conflict with the `&self.batches`
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
    use super::ImageKey;
    use crate::types::Image;

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
