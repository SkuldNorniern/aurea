//! Text rendering for Canvas.
//!
//! `shape` turns text into positioned glyphs (harfrust, bidi, per-character
//! font fallback); `platform` holds the backend seam and lays shaped glyphs
//! into masks. Concrete glyph rasterizers are modular:
//! - `directwrite_backend` — hinted ClearType via DirectWrite (Windows only).
//! - `fontdue_backend` — cross-platform fallback (fontdb/fontdue, no hinting).

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

/// Bytes of glyph coverage a rasterizer keeps; at large sizes a few hundred
/// glyphs would otherwise be tens of megabytes.
pub(crate) const GLYPH_BYTES: usize = 4 << 20;
/// Bytes of shaped text masks the text renderer keeps per cache.
pub(crate) const MASK_BYTES: usize = 8 << 20;

// ── LRU cache ────────────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::hash::Hash;

/// Clock-based LRU eviction cache, bounded by entries and, for entries
/// that say how big they are, by bytes. O(len) eviction; designed for small
/// caps where maintaining a separate doubly-linked list is overkill.
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

    /// At most `cap` entries taking at most `budget` bytes together.
    pub(crate) fn with_budget(cap: usize, budget: usize) -> Self {
        Self {
            map: HashMap::with_capacity(cap.min(256)),
            clock: 0,
            cap,
            budget,
            bytes: 0,
        }
    }

    /// Returns a shared reference to the value and bumps its recency timestamp.
    pub(crate) fn get(&mut self, k: &K) -> Option<&V> {
        if let Some((v, ts, _)) = self.map.get_mut(k) {
            self.clock += 1;
            *ts = self.clock;
            Some(v)
        } else {
            None
        }
    }

    /// Insert `k → v`, evicting the least-recently-used entry when at capacity.
    pub(crate) fn insert(&mut self, k: K, v: V) {
        self.insert_sized(k, v, 0);
    }

    /// Insert `k → v` taking `bytes`, evicting least-recently-used entries
    /// until it fits. One larger than the whole budget is still kept, alone.
    pub(crate) fn insert_sized(&mut self, k: K, v: V, bytes: usize) {
        if let Some((_, _, old)) = self.map.remove(&k) {
            self.bytes -= old;
        }
        while !self.map.is_empty()
            && (self.map.len() >= self.cap || self.bytes.saturating_add(bytes) > self.budget)
        {
            self.evict_oldest();
        }
        self.clock += 1;
        self.bytes += bytes;
        self.map.insert(k, (v, self.clock, bytes));
    }

    /// Bytes the entries take, as they were inserted.
    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    fn evict_oldest(&mut self) {
        let oldest = self
            .map
            .iter()
            .min_by_key(|(_, (_, ts, _))| *ts)
            .map(|(k, _)| k.clone());
        if let Some(key) = oldest
            && let Some((_, _, bytes)) = self.map.remove(&key)
        {
            self.bytes -= bytes;
        }
    }
}

#[cfg(test)]
mod lru_tests {
    use super::LruCache;

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
}
