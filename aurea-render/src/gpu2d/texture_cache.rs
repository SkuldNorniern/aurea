//! Backend-agnostic LRU texture cache for the GPU-2D core.
//!
//! Maps `Arc<[u8]>` pixel-buffer identity + dimensions to a shader-visible
//! slot index returned by the backend's `upload_image`. Slot assignment,
//! GPU upload, and GPU teardown are delegated to the backend through
//! `Gpu2dBackend::upload_image` / `evict_image`; this module owns only
//! the LRU eviction policy, keepalive tracking, and dropped-image pruning.

use std::collections::HashMap;
use std::sync::Arc;

use aurea_foundation::{AureaError, AureaResult};

use super::backend::Gpu2dBackend;

/// Default maximum number of concurrently cached textures.
pub const DEFAULT_CACHE_CAP: usize = 64;

/// Cache key: Arc pointer identity + buffer length + pixel dimensions.
///
/// Two calls with the same `Arc<[u8]>` clone, same width and height, will
/// always produce the same key — no pixel comparison needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CacheKey {
    data_ptr: usize,
    data_len: usize,
    width: u32,
    height: u32,
}

impl CacheKey {
    fn from_pixels(data: &Arc<[u8]>, width: u32, height: u32) -> Self {
        Self {
            data_ptr: data.as_ptr() as usize,
            data_len: data.len(),
            width,
            height,
        }
    }
}

struct Entry {
    /// Shader-visible slot index returned by `backend.upload_image`.
    slot: u32,
    /// Keeps the pixel buffer alive so strong_count pruning works.
    keepalive: Arc<[u8]>,
    last_used: u64,
}

/// Backend-agnostic LRU texture cache.
pub struct TextureCache {
    map: HashMap<CacheKey, Entry>,
    clock: u64,
    cap: usize,
}

impl TextureCache {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CACHE_CAP)
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            clock: 0,
            cap,
        }
    }

    /// Return the shader slot for `pixels`/`width`/`height`, uploading via the
    /// backend if not already cached, and evicting the LRU entry if at capacity.
    pub fn resolve<B: Gpu2dBackend>(
        &mut self,
        pixels: &Arc<[u8]>,
        width: u32,
        height: u32,
        backend: &mut B,
        frame: u64,
    ) -> AureaResult<u32> {
        let key = CacheKey::from_pixels(pixels, width, height);
        self.clock += 1;
        let clock = self.clock;

        if let Some(entry) = self.map.get_mut(&key) {
            entry.last_used = clock;
            return Ok(entry.slot);
        }

        if self.map.len() >= self.cap {
            self.evict_lru(backend, frame)?;
        }

        let slot = backend.upload_image(width, height, pixels)?;
        self.map.insert(
            key,
            Entry {
                slot,
                keepalive: Arc::clone(pixels),
                last_used: clock,
            },
        );
        Ok(slot)
    }

    /// Release entries whose source pixel buffer has been dropped by the caller
    /// (i.e. `strong_count == 1` — only the cache's keepalive remains).
    pub fn prune_dropped<B: Gpu2dBackend>(&mut self, backend: &mut B) {
        let mut to_evict = Vec::new();
        for (key, entry) in &self.map {
            if Arc::strong_count(&entry.keepalive) == 1 {
                to_evict.push((*key, entry.slot));
            }
        }
        for (key, slot) in to_evict {
            self.map.remove(&key);
            backend.evict_image(slot);
        }
    }

    /// Release all cached entries. Call while the device is idle (Drop context).
    #[allow(dead_code)]
    pub fn drain<B: Gpu2dBackend>(&mut self, backend: &mut B) {
        for (_, entry) in self.map.drain() {
            backend.evict_image(entry.slot);
        }
    }

    fn evict_lru<B: Gpu2dBackend>(
        &mut self,
        backend: &mut B,
        _current_frame: u64,
    ) -> AureaResult<()> {
        let key = self
            .map
            .iter()
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| *k)
            .ok_or(AureaError::RenderingFailed)?;
        let entry = self.map.remove(&key).expect("key came from map");
        backend.evict_image(entry.slot);
        Ok(())
    }
}
