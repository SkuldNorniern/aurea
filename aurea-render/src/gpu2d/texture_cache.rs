//! Backend-agnostic LRU texture cache for the GPU-2D core.

use std::collections::HashMap;
use std::sync::Arc;

use aurea_foundation::AureaResult;

use super::backend::Gpu2dBackend;

/// Default maximum number of concurrently cached textures.
pub const DEFAULT_CACHE_CAP: usize = 64;

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
    slot: u32,
    keepalive: Arc<[u8]>,
    last_used: u64,
    frame: u64,
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

    /// Return the shader slot for `pixels`/`width`/`height`, uploading via the backend if not
    /// already cached, and evicting the LRU entry if at capacity.
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
            entry.frame = frame;
            return Ok(entry.slot);
        }

        if self.map.len() >= self.cap {
            self.evict_lru(backend, frame);
        }

        let slot = backend.upload_image(width, height, pixels)?;
        self.map.insert(
            key,
            Entry {
                slot,
                keepalive: Arc::clone(pixels),
                last_used: clock,
                frame,
            },
        );
        Ok(slot)
    }

    /// Release entries whose source pixel buffer has been dropped by the caller (i.e. `strong_count
    /// == 1` — only the cache's keepalive remains).
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

    /// Release all cached entries.
    #[allow(dead_code)]
    pub fn drain<B: Gpu2dBackend>(&mut self, backend: &mut B) {
        for (_, entry) in self.map.drain() {
            backend.evict_image(entry.slot);
        }
    }

    fn evict_lru<B: Gpu2dBackend>(&mut self, backend: &mut B, current_frame: u64) {
        let Some(key) = self
            .map
            .iter()
            .filter(|(_, e)| e.frame != current_frame)
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| *k)
        else {
            return;
        };
        if let Some(entry) = self.map.remove(&key) {
            backend.evict_image(entry.slot);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::{CircleInstance, RectInstance};
    use crate::gpu2d::frame_plan::FramePlan;
    use crate::renderer::FrameOutcome;

    #[derive(Default)]
    struct Slots {
        next: u32,
        live: Vec<u32>,
        evicted: Vec<u32>,
    }

    impl Gpu2dBackend for Slots {
        fn resize(&mut self, _: u32, _: u32) -> AureaResult<()> {
            Ok(())
        }

        fn upload_image(&mut self, _: u32, _: u32, _: &[u8]) -> AureaResult<u32> {
            self.next += 1;
            self.live.push(self.next);
            Ok(self.next)
        }

        fn evict_image(&mut self, slot: u32) {
            self.live.retain(|s| *s != slot);
            self.evicted.push(slot);
        }

        fn present_frame(
            &mut self,
            _: &FramePlan,
            _: &[RectInstance],
            _: &[CircleInstance],
        ) -> AureaResult<FrameOutcome> {
            Ok(FrameOutcome::Presented)
        }
    }

    fn pixels() -> Arc<[u8]> {
        Arc::from(vec![0u8; 4])
    }

    #[test]
    fn a_frame_keeps_every_texture_it_uses() {
        let (mut cache, mut backend) = (TextureCache::with_capacity(2), Slots::default());
        let images = [pixels(), pixels(), pixels()];
        let slots: Vec<u32> = images
            .iter()
            .map(|p| cache.resolve(p, 1, 1, &mut backend, 1).expect("resolve"))
            .collect();
        assert!(
            backend.evicted.is_empty(),
            "nothing this frame uses is evicted"
        );
        for slot in &slots {
            assert!(backend.live.contains(slot));
        }
        assert_eq!(cache.len(), 3, "the frame runs over the capacity");
    }

    #[test]
    fn a_later_frame_evicts_what_it_no_longer_uses() {
        let (mut cache, mut backend) = (TextureCache::with_capacity(2), Slots::default());
        let images = [pixels(), pixels(), pixels()];
        for p in &images {
            cache.resolve(p, 1, 1, &mut backend, 1).expect("resolve");
        }
        let fresh = pixels();
        let slot = cache
            .resolve(&fresh, 1, 1, &mut backend, 2)
            .expect("resolve");
        assert_eq!(
            backend.evicted,
            vec![1],
            "the oldest from the last frame goes"
        );
        assert!(backend.live.contains(&slot));
    }

    #[test]
    fn a_reused_texture_is_pinned_for_its_new_frame() {
        let (mut cache, mut backend) = (TextureCache::with_capacity(2), Slots::default());
        let (a, b, c) = (pixels(), pixels(), pixels());
        cache.resolve(&a, 1, 1, &mut backend, 1).expect("resolve");
        cache.resolve(&b, 1, 1, &mut backend, 1).expect("resolve");
        cache.resolve(&a, 1, 1, &mut backend, 2).expect("resolve");
        cache.resolve(&c, 1, 1, &mut backend, 2).expect("resolve");
        assert_eq!(
            backend.evicted,
            vec![2],
            "`b` goes, not the `a` frame 2 holds"
        );
    }
}
