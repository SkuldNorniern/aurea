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

use std::collections::HashMap;
use std::hash::Hash;

pub(crate) struct LruCache<K, V> {
    map: HashMap<K, (V, u64, usize)>,
    clock: u64,
    cap: usize,
    budget: usize,
    bytes: usize,
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
        }
    }

    pub(crate) fn get(&mut self, k: &K) -> Option<&V> {
        if let Some((v, ts, _)) = self.map.get_mut(k) {
            self.clock += 1;
            *ts = self.clock;
            Some(v)
        } else {
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
