//! Text rendering for Canvas.

pub mod font;
pub mod glyph;
pub mod layout;
pub mod outline;
pub mod platform;
pub mod shape;

#[cfg(windows)]
mod directwrite_backend;
mod fontdue_backend;

pub use glyph::*;
pub use layout::*;
pub use platform::*;
pub use shape::*;

pub(crate) const GLYPH_BYTES: usize = 4 << 20;
pub(crate) const MASK_BYTES: usize = 8 << 20;
pub(crate) const SHAPED_BYTES: usize = 8 << 20;
pub(crate) const LAYOUT_BYTES: usize = 8 << 20;

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

/// Hits, misses and what a text cache holds now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextCacheStats {
    /// Lookups answered from the cache.
    pub hits: u64,
    /// Lookups that were not, and were computed.
    pub misses: u64,
    /// Entries held now.
    pub entries: usize,
    /// Bytes held now, as the cache's budget counts them.
    pub bytes: usize,
}

/// The stats of each text cache. Counts cover every text renderer alive or dropped in this
/// process, so the hit and miss counts only grow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextCacheStatsSet {
    /// Shaped runs, keyed by text, font and direction.
    pub shape: TextCacheStats,
    /// Coverage masks of text runs.
    pub mask: TextCacheStats,
    /// Glyph bitmaps, from the platform rasterizer.
    pub glyph: TextCacheStats,
    /// Laid-out paragraphs, keyed by text, font and options.
    pub layout: TextCacheStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CacheKind {
    Shape,
    Mask,
    Glyph,
    Layout,
}

const KINDS: usize = 4;

#[derive(Default)]
struct Tally {
    hits: AtomicU64,
    misses: AtomicU64,
}

pub(crate) struct LruCache<K, V> {
    map: HashMap<K, (V, u64, usize)>,
    clock: u64,
    cap: usize,
    budget: usize,
    bytes: usize,
    tally: Arc<Tally>,
}

trait Measured {
    /// Entries held and bytes they count as.
    fn measure(&self) -> (usize, usize);
}

impl<K: Eq + Hash + Clone, V> Measured for LruCache<K, V> {
    fn measure(&self) -> (usize, usize) {
        (self.map.len(), self.bytes)
    }
}

struct Registered {
    kind: CacheKind,
    tally: Arc<Tally>,
    cache: Weak<Mutex<dyn Measured + Send>>,
}

#[derive(Default)]
struct Registry {
    caches: Vec<Registered>,
    /// Hits and misses of caches that were dropped, by kind.
    retired: [(u64, u64); KINDS],
}

impl Registry {
    fn prune(&mut self) {
        let retired = &mut self.retired;
        self.caches.retain(|c| {
            if c.cache.strong_count() > 0 {
                return true;
            }
            let slot = &mut retired[c.kind as usize];
            slot.0 += c.tally.hits.load(Ordering::Relaxed);
            slot.1 += c.tally.misses.load(Ordering::Relaxed);
            false
        });
    }
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    caches: Vec::new(),
    retired: [(0, 0); KINDS],
});

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Wraps `cache` in a shared lock and lists it for [`cache_stats`].
pub(crate) fn shared<K, V>(kind: CacheKind, cache: LruCache<K, V>) -> Arc<Mutex<LruCache<K, V>>>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Send + 'static,
{
    let tally = cache.tally.clone();
    let cache = Arc::new(Mutex::new(cache));
    let source = Arc::downgrade(&cache);
    let weak: Weak<Mutex<dyn Measured + Send>> = source;
    let mut reg = registry();
    reg.prune();
    reg.caches.push(Registered {
        kind,
        tally,
        cache: weak,
    });
    cache
}

/// Hits, misses, entries and bytes of each text cache, summed over every text renderer in the
/// process (the shared one that drawing uses, and any others).
pub fn cache_stats() -> TextCacheStatsSet {
    let mut reg = registry();
    reg.prune();
    let mut out = [TextCacheStats::default(); KINDS];
    for (slot, &(hits, misses)) in out.iter_mut().zip(reg.retired.iter()) {
        slot.hits = hits;
        slot.misses = misses;
    }
    for entry in &reg.caches {
        let slot = &mut out[entry.kind as usize];
        slot.hits += entry.tally.hits.load(Ordering::Relaxed);
        slot.misses += entry.tally.misses.load(Ordering::Relaxed);
        if let Some(cache) = entry.cache.upgrade() {
            let (entries, bytes) = cache
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .measure();
            slot.entries += entries;
            slot.bytes += bytes;
        }
    }
    TextCacheStatsSet {
        shape: out[CacheKind::Shape as usize],
        mask: out[CacheKind::Mask as usize],
        glyph: out[CacheKind::Glyph as usize],
        layout: out[CacheKind::Layout as usize],
    }
}

impl<K: Eq + Hash + Clone, V> LruCache<K, V> {
    pub(crate) fn new(cap: usize) -> Self {
        Self::with_budget(cap, usize::MAX)
    }

    pub(crate) fn with_budget(cap: usize, budget: usize) -> Self {
        Self {
            map: HashMap::with_capacity(cap.min(256)),
            clock: 0,
            cap,
            budget,
            bytes: 0,
            tally: Arc::default(),
        }
    }

    pub(crate) fn get(&mut self, k: &K) -> Option<&V> {
        if let Some((v, ts, _)) = self.map.get_mut(k) {
            self.clock += 1;
            *ts = self.clock;
            self.tally.hits.fetch_add(1, Ordering::Relaxed);
            Some(v)
        } else {
            self.tally.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    pub(crate) fn insert(&mut self, k: K, v: V) {
        self.insert_sized(k, v, 0);
    }

    pub(crate) fn insert_sized(&mut self, k: K, v: V, bytes: usize) {
        if let Some((_, _, old)) = self.map.remove(&k) {
            self.bytes -= old;
        }
        while !self.map.is_empty()
            && (self.map.len() >= self.cap || self.bytes.saturating_add(bytes) > self.budget)
        {
            self.evict_oldest_batch();
        }
        self.clock += 1;
        self.bytes += bytes;
        self.map.insert(k, (v, self.clock, bytes));
    }

    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    /// Drops the oldest eighth of the entries in one pass, so a full cache pays one scan per
    /// batch rather than one per insert. Stamps are unique, so the cut removes exactly `n`.
    fn evict_oldest_batch(&mut self) {
        let mut stamps: Vec<u64> = self.map.values().map(|(_, ts, _)| *ts).collect();
        let n = (stamps.len() / 8).max(1);
        let (_, &mut cutoff, _) = stamps.select_nth_unstable(n - 1);
        let bytes = &mut self.bytes;
        self.map.retain(|_, (_, ts, size)| {
            if *ts <= cutoff {
                *bytes -= *size;
                false
            } else {
                true
            }
        });
    }
}

#[cfg(test)]
mod lru_tests {
    use super::LruCache;
    use std::time::Instant;

    #[test]
    fn entries_and_bytes_both_bound_it() {
        let mut cache = LruCache::with_budget(3, 100);
        cache.insert_sized(1, (), 40);
        cache.insert_sized(2, (), 40);
        cache.get(&1);
        cache.insert_sized(3, (), 40);
        assert!(cache.get(&2).is_none(), "the oldest went to make room");
        assert_eq!((cache.len(), cache.bytes()), (2, 80));
        cache.insert_sized(4, (), 10);
        cache.insert_sized(5, (), 10);
        assert_eq!(cache.len(), 3, "and the count is kept");
        cache.insert_sized(6, (), 500);
        assert_eq!((cache.len(), cache.bytes()), (1, 500));
        cache.insert_sized(6, (), 5);
        assert_eq!(cache.bytes(), 5, "replacing frees the old size");
    }

    #[test]
    fn full_cache_keeps_cap_and_recent_entries() {
        let mut cache = LruCache::new(16);
        for i in 0..200u32 {
            cache.insert(i, ());
            cache.get(&0);
            assert!(cache.len() <= 16);
        }
        assert!(cache.get(&0).is_some(), "touched entry survives");
        assert!(cache.get(&199).is_some(), "newest entry survives");
        assert!(cache.get(&1).is_none(), "old untouched entry is gone");
    }

    #[test]
    fn byte_budget_evicts_sized_entries() {
        let mut cache = LruCache::with_budget(512, 1000);
        for i in 0..50u32 {
            cache.insert_sized(i, (), 300);
            assert!(cache.bytes() <= 1000);
        }
        assert!(cache.get(&49).is_some(), "newest run is kept");
        assert!(
            cache.len() <= 3,
            "300-byte runs fit three to a 1000-byte budget"
        );
    }

    #[test]
    #[ignore]
    fn timing_cycling_past_cap() {
        const ROUNDS: usize = 10;
        let mut best = f64::MAX;
        for _ in 0..3 {
            let mut cache = LruCache::new(512);
            let start = Instant::now();
            for _ in 0..ROUNDS {
                for k in 0..600u32 {
                    cache.insert(k, ());
                }
            }
            let ms = start.elapsed().as_secs_f64() * 1e3 / ROUNDS as f64;
            best = best.min(ms);
        }
        println!("cycling 600 keys into cap 512: {best:.4} ms per round (best of 3)");
    }
}
