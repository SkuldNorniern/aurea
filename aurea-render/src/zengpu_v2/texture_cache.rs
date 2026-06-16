//! Texture upload/cache for the unified-API painter.
//!
//! Replaces the production painter's private 64-slot descriptor set
//! (`set_image_slot`/`set_sampled_image_slot`/`clear_image_slot`) with
//! `GpuDevice::bind_texture`, which writes directly into the device's global
//! bindless texture table. The slot index equals the texture handle's own
//! slotmap index (`TextureHandle::index()`), so no manual free-list is needed.
//! An LRU cache bounded by `MAX_CACHED` caps GPU memory usage.

use std::collections::HashMap;
use std::sync::Arc;

use aurea_foundation::{AureaError, AureaResult};
use zengpu_hal::{Format, GpuDevice, SamplerHandle, TextureDesc, TextureHandle, TextureUsage};
use zengpu_vulkan::VulkanDevice;

const MAX_CACHED: usize = 64;

/// Cache key: pixel buffer identity + dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageKey {
    data_ptr: usize,
    data_len: usize,
    width: u32,
    height: u32,
}

impl ImageKey {
    pub fn from_pixels(data: &Arc<[u8]>, width: u32, height: u32) -> Self {
        Self { data_ptr: data.as_ptr() as usize, data_len: data.len(), width, height }
    }
}

pub struct CachedTexture {
    pub texture: TextureHandle,
    pub slot: u32,
    keepalive: Arc<[u8]>,
    pub last_used: u64,
}

pub struct TextureCache {
    map: HashMap<ImageKey, CachedTexture>,
}

impl TextureCache {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    /// Upload `pixels` to a GPU texture and return its bindless slot, or return
    /// the cached slot if already present. Evicts the LRU entry when at capacity.
    pub fn resolve(
        &mut self,
        pixels: &Arc<[u8]>,
        width: u32,
        height: u32,
        device: &VulkanDevice,
        sampler: SamplerHandle,
        frame: u64,
    ) -> AureaResult<u32> {
        let key = ImageKey::from_pixels(pixels, width, height);
        if let Some(cached) = self.map.get_mut(&key) {
            cached.last_used = frame;
            return Ok(cached.slot);
        }
        if self.map.len() >= MAX_CACHED {
            self.evict_lru(device, frame)?;
        }
        let texture = device
            .create_texture(TextureDesc {
                width,
                height,
                format: Format::Rgba8Unorm,
                usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                samples: 1,
            })
            .map_err(gpu_err)?;
        if let Err(e) = device.upload_texture_data(texture, pixels) {
            device.destroy_texture(texture);
            return Err(gpu_err(e));
        }
        let slot = device.bind_texture(texture, sampler).ok_or(AureaError::RenderingFailed)?;
        self.map.insert(key, CachedTexture { texture, slot, keepalive: Arc::clone(pixels), last_used: frame });
        Ok(slot)
    }

    /// Prune textures whose source pixel buffer has been dropped (strong_count == 1 means only the cache holds it).
    pub fn prune_dropped(&mut self, device: &VulkanDevice) {
        self.map.retain(|_, cached| {
            if Arc::strong_count(&cached.keepalive) == 1 {
                device.destroy_texture(cached.texture);
                false
            } else {
                true
            }
        });
    }

    /// Release all cached textures. Call while the device is idle (Drop context).
    pub fn drain(&mut self, device: &VulkanDevice) {
        for (_, cached) in self.map.drain() {
            device.destroy_texture(cached.texture);
        }
    }

    fn evict_lru(&mut self, device: &VulkanDevice, frame: u64) -> AureaResult<()> {
        let key = self
            .map
            .iter()
            .filter(|(_, c)| c.last_used != frame)
            .min_by_key(|(_, c)| c.last_used)
            .map(|(k, _)| *k)
            .ok_or(AureaError::RenderingFailed)?;
        let old = self.map.remove(&key).expect("key came from cache");
        device.destroy_texture(old.texture);
        Ok(())
    }
}

pub fn gpu_err(_e: zengpu_hal::GpuError) -> AureaError {
    AureaError::ElementOperationFailed
}
