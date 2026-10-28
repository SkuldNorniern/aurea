//! Backend-agnostic LRU texture cache for the GPU-2D core.

use std::collections::HashMap;
use std::sync::Arc;

use aurea_foundation::AureaResult;

use crate::types::GpuTexture;

use super::backend::Gpu2dBackend;

/// Default maximum number of concurrently cached textures.
pub const DEFAULT_CACHE_CAP: usize = 64;

/// Default maximum GPU bytes of cached textures. A texture pinned in the current frame may take
/// the cache past this, since it cannot be dropped while the frame draws it.
pub const DEFAULT_CACHE_BYTES: usize = 256 << 20;

/// Past this many remembered app-texture bindings the map starts over, costing one rebind each.
const BOUND_CAP: usize = 1024;

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
    mipmapped: bool,
    bytes: usize,
}

/// GPU bytes of a texture: RGBA8 pixels, plus a third more for the mip chain when mipmapped.
fn texture_bytes(width: u32, height: u32, mipmapped: bool) -> usize {
    let base = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if mipmapped {
        base.saturating_add(base / 3)
    } else {
        base
    }
}

/// Backend-agnostic LRU texture cache, bounded by entry count and by bytes.
pub struct TextureCache {
    map: HashMap<CacheKey, Entry>,
    retired: Vec<(u32, u64)>,
    clock: u64,
    cap: usize,
    budget: usize,
    bytes: usize,
    bound: HashMap<(u32, u32), u32>,
}

impl TextureCache {
    pub fn new() -> Self {
        Self::with_budget(DEFAULT_CACHE_CAP, DEFAULT_CACHE_BYTES)
    }

    #[cfg(test)]
    pub fn with_capacity(cap: usize) -> Self {
        Self::with_budget(cap, DEFAULT_CACHE_BYTES)
    }

    pub fn with_budget(cap: usize, budget: usize) -> Self {
        Self {
            map: HashMap::new(),
            retired: Vec::new(),
            clock: 0,
            cap,
            budget,
            bytes: 0,
            bound: HashMap::new(),
        }
    }

    /// The shader slot for an app-made `texture`. Binding writes a descriptor, so each texture
    /// (keyed by its table index and generation) is bound once and its slot reused after that.
    pub fn bind_texture<B: Gpu2dBackend>(
        &mut self,
        texture: GpuTexture,
        backend: &mut B,
    ) -> AureaResult<u32> {
        let key = (texture.index, texture.generation);
        if let Some(&slot) = self.bound.get(&key) {
            return Ok(slot);
        }
        if self.bound.len() >= BOUND_CAP {
            self.bound.clear();
        }
        let slot = backend.bind_texture(texture)?;
        self.bound.insert(key, slot);
        Ok(slot)
    }

    /// Return the shader slot for `pixels`/`width`/`height`, uploading via the backend if not
    /// already cached, and evicting unpinned LRU entries until it fits the count and byte caps.
    pub fn resolve<B: Gpu2dBackend>(
        &mut self,
        pixels: &Arc<[u8]>,
        (width, height): (u32, u32),
        mipmapped: bool,
        backend: &mut B,
        frame: u64,
    ) -> AureaResult<u32> {
        let key = CacheKey::from_pixels(pixels, width, height);
        self.clock += 1;
        let clock = self.clock;
        self.free_retired(backend, frame);

        if let Some(entry) = self.map.get_mut(&key) {
            entry.last_used = clock;
            let drawn_this_frame = entry.frame == frame;
            entry.frame = frame;
            if entry.mipmapped || !mipmapped {
                return Ok(entry.slot);
            }
            let plain = entry.slot;
            entry.slot = backend.upload_image_mipmapped(width, height, pixels)?;
            entry.mipmapped = true;
            let grown = texture_bytes(width, height, true);
            self.bytes = self.bytes - entry.bytes + grown;
            entry.bytes = grown;
            let slot = entry.slot;
            if drawn_this_frame {
                self.retired.push((plain, frame));
            } else {
                backend.evict_image(plain);
            }
            self.trim(backend, frame, 0, 0);
            return Ok(slot);
        }

        let bytes = texture_bytes(width, height, mipmapped);
        self.trim(backend, frame, 1, bytes);

        let slot = if mipmapped {
            backend.upload_image_mipmapped(width, height, pixels)?
        } else {
            backend.upload_image(width, height, pixels)?
        };
        self.bytes += bytes;
        self.map.insert(
            key,
            Entry {
                slot,
                keepalive: Arc::clone(pixels),
                last_used: clock,
                frame,
                mipmapped,
                bytes,
            },
        );
        Ok(slot)
    }

    /// Release entries whose source pixel buffer has been dropped by the caller (i.e. `strong_count
    /// == 1` — only the cache's keepalive remains).
    pub fn prune_dropped<B: Gpu2dBackend>(&mut self, backend: &mut B) {
        let bytes = &mut self.bytes;
        self.map.retain(|_, entry| {
            if Arc::strong_count(&entry.keepalive) == 1 {
                *bytes -= entry.bytes;
                backend.evict_image(entry.slot);
                false
            } else {
                true
            }
        });
    }

    /// Finish cache housekeeping even when this frame uploaded no textures.
    pub fn finish_frame<B: Gpu2dBackend>(&mut self, backend: &mut B, frame: u64) {
        self.free_retired(backend, frame);
        self.trim(backend, frame, 0, 0);
    }

    /// Release all cached entries.
    #[allow(dead_code)]
    pub fn drain<B: Gpu2dBackend>(&mut self, backend: &mut B) {
        for (_, entry) in self.map.drain() {
            backend.evict_image(entry.slot);
        }
        self.bytes = 0;
        for (slot, _) in self.retired.drain(..) {
            backend.evict_image(slot);
        }
    }

    fn over_limit(&self, extra_entries: usize, extra_bytes: usize) -> bool {
        self.map.len() + extra_entries > self.cap
            || self.bytes.saturating_add(extra_bytes) > self.budget
    }

    /// Evict least-recently-used entries not pinned by `frame` until `extra_entries` and
    /// `extra_bytes` more would fit. Stops when only pinned entries remain, so a frame may
    /// exceed the caps.
    fn trim<B: Gpu2dBackend>(
        &mut self,
        backend: &mut B,
        frame: u64,
        extra_entries: usize,
        extra_bytes: usize,
    ) {
        if !self.over_limit(extra_entries, extra_bytes) {
            return;
        }
        let mut candidates: Vec<_> = self
            .map
            .iter()
            .filter(|(_, entry)| entry.frame != frame)
            .map(|(key, entry)| (*key, entry.last_used))
            .collect();
        candidates.sort_unstable_by_key(|(_, used)| *used);
        for (key, _) in candidates {
            if !self.over_limit(extra_entries, extra_bytes) {
                break;
            }
            if let Some(entry) = self.map.remove(&key) {
                self.bytes -= entry.bytes;
                backend.evict_image(entry.slot);
            }
        }
    }

    fn free_retired<B: Gpu2dBackend>(&mut self, backend: &mut B, frame: u64) {
        self.retired.retain(|&(slot, used)| {
            if used < frame {
                backend.evict_image(slot);
                false
            } else {
                true
            }
        });
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.map.len()
    }

    #[cfg(test)]
    fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::GpuFeatures;
    use crate::gpu::batch::{RectInstance, ShapeInstance};
    use crate::gpu::frame_plan::FramePlan;
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

        fn features(&self) -> GpuFeatures {
            GpuFeatures::ALL
        }

        fn evict_image(&mut self, slot: u32) {
            self.live.retain(|s| *s != slot);
            self.evicted.push(slot);
        }

        fn present_frame(
            &mut self,
            _: &FramePlan,
            _: &[RectInstance],
            _: &[ShapeInstance],
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
            .map(|p| {
                cache
                    .resolve(p, (1, 1), false, &mut backend, 1)
                    .expect("resolve")
            })
            .collect();
        assert!(
            backend.evicted.is_empty(),
            "nothing this frame uses is evicted"
        );
        for slot in &slots {
            assert!(backend.live.contains(slot));
        }
        assert_eq!(cache.len(), 3, "the frame runs over the capacity");
        cache.finish_frame(&mut backend, 1);
        assert!(backend.evicted.is_empty());
    }

    #[test]
    fn cache_hit_only_frames_trim_excess_older_textures() {
        let (mut cache, mut backend) = (TextureCache::with_capacity(2), Slots::default());
        let images = [pixels(), pixels(), pixels(), pixels()];
        for image in &images {
            cache
                .resolve(image, (1, 1), false, &mut backend, 1)
                .expect("resolve");
        }
        cache.finish_frame(&mut backend, 1);
        assert_eq!(cache.len(), 4);
        let kept = cache
            .resolve(&images[0], (1, 1), false, &mut backend, 2)
            .expect("cache hit");
        cache.finish_frame(&mut backend, 2);
        assert_eq!(cache.len(), 2);
        assert_eq!(backend.next, 4, "no new uploads");
        assert_eq!(backend.evicted, [2, 3], "oldest unpinned entries leave");
        assert!(backend.live.contains(&kept));
    }

    #[test]
    fn texture_free_frames_release_retired_slots() {
        let (mut cache, mut backend) = (TextureCache::new(), Slots::default());
        let image = pixels();
        let plain = cache
            .resolve(&image, (1, 1), false, &mut backend, 1)
            .expect("plain");
        cache
            .resolve(&image, (1, 1), true, &mut backend, 1)
            .expect("mips");
        cache.finish_frame(&mut backend, 1);
        assert!(backend.live.contains(&plain));
        cache.finish_frame(&mut backend, 2);
        assert_eq!(backend.evicted, [plain]);
        assert!(cache.retired.is_empty());
    }

    #[test]
    fn a_later_frame_evicts_what_it_no_longer_uses() {
        let (mut cache, mut backend) = (TextureCache::with_capacity(2), Slots::default());
        let images = [pixels(), pixels(), pixels()];
        for p in &images {
            cache
                .resolve(p, (1, 1), false, &mut backend, 1)
                .expect("resolve");
        }
        let fresh = pixels();
        let slot = cache
            .resolve(&fresh, (1, 1), false, &mut backend, 2)
            .expect("resolve");
        assert_eq!(
            backend.evicted,
            vec![1, 2],
            "the oldest from the last frame go, until the count fits"
        );
        assert!(backend.live.contains(&slot));
    }

    #[test]
    fn a_reused_texture_is_pinned_for_its_new_frame() {
        let (mut cache, mut backend) = (TextureCache::with_capacity(2), Slots::default());
        let (a, b, c) = (pixels(), pixels(), pixels());
        cache
            .resolve(&a, (1, 1), false, &mut backend, 1)
            .expect("resolve");
        cache
            .resolve(&b, (1, 1), false, &mut backend, 1)
            .expect("resolve");
        cache
            .resolve(&a, (1, 1), false, &mut backend, 2)
            .expect("resolve");
        cache
            .resolve(&c, (1, 1), false, &mut backend, 2)
            .expect("resolve");
        assert_eq!(
            backend.evicted,
            vec![2],
            "`b` goes, not the `a` frame 2 holds"
        );
    }

    #[test]
    fn smooth_draws_get_a_mipmapped_upload_once() {
        let mut cache = TextureCache::new();
        let mut backend = Slots::default();
        let image = pixels();
        let plain = cache
            .resolve(&image, (1, 1), false, &mut backend, 1)
            .expect("resolve");
        let smooth = cache
            .resolve(&image, (1, 1), true, &mut backend, 1)
            .expect("resolve");
        assert_ne!(plain, smooth);
        assert!(
            backend.evicted.is_empty(),
            "this frame still draws the plain one"
        );
        let again = cache
            .resolve(&image, (1, 1), true, &mut backend, 2)
            .expect("resolve");
        assert_eq!(
            backend.evicted,
            vec![plain],
            "freed once the next frame starts"
        );
        let linear = cache
            .resolve(&image, (1, 1), false, &mut backend, 2)
            .expect("resolve");
        assert_eq!((again, linear), (smooth, smooth));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn mipmaps_add_a_third_to_the_base_bytes() {
        assert_eq!(texture_bytes(4, 4, false), 64);
        assert_eq!(texture_bytes(4, 4, true), 85);
    }

    #[test]
    fn oversized_inserts_evict_oldest_unpinned_and_keep_pinned() {
        // Each 4x4 texture is 64 bytes, so the budget holds two.
        let (mut cache, mut backend) = (TextureCache::with_budget(16, 150), Slots::default());
        let (a, b, c, d) = (pixels(), pixels(), pixels(), pixels());
        for image in [&a, &b, &c] {
            cache
                .resolve(image, (4, 4), false, &mut backend, 1)
                .expect("resolve");
        }
        cache.finish_frame(&mut backend, 1);
        assert!(
            backend.evicted.is_empty(),
            "everything is pinned in frame 1"
        );
        assert_eq!(cache.bytes(), 192, "pinned bytes may exceed the budget");

        cache
            .resolve(&a, (4, 4), false, &mut backend, 2)
            .expect("hit pins `a`");
        cache
            .resolve(&d, (4, 4), false, &mut backend, 2)
            .expect("resolve");
        assert_eq!(
            backend.evicted,
            [2, 3],
            "`b` and `c` go, oldest first; `a` is pinned"
        );
        assert_eq!((cache.len(), cache.bytes()), (2, 128));
    }

    #[test]
    fn a_frame_alone_over_budget_is_kept_whole() {
        let (mut cache, mut backend) = (TextureCache::with_budget(16, 100), Slots::default());
        let (a, b) = (pixels(), pixels());
        cache
            .resolve(&a, (4, 4), false, &mut backend, 1)
            .expect("resolve");
        cache
            .resolve(&b, (4, 4), false, &mut backend, 1)
            .expect("resolve");
        cache.finish_frame(&mut backend, 1);
        assert!(backend.evicted.is_empty());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn pruned_and_mip_upgraded_entries_keep_the_byte_count() {
        let (mut cache, mut backend) = (TextureCache::with_budget(16, 1000), Slots::default());
        let image = Arc::<[u8]>::from(vec![0u8; 64]);
        cache
            .resolve(&image, (4, 4), false, &mut backend, 1)
            .expect("plain");
        cache
            .resolve(&image, (4, 4), true, &mut backend, 2)
            .expect("mips");
        assert_eq!(cache.bytes(), 85, "the upgrade replaces the plain size");
        drop(image);
        cache.prune_dropped(&mut backend);
        assert_eq!((cache.len(), cache.bytes()), (0, 0));
    }
}
