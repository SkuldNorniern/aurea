//! Text rendering for Canvas.
//!
//! `platform` orchestrates the backend seam and run shaping. Concrete glyph
//! rasterizers are modular:
//! - `directwrite_backend` — hinted ClearType via DirectWrite (Windows only).
//! - `fontdue_backend` — cross-platform fallback (fontdb/fontdue, no hinting).

pub mod atlas;
pub mod platform;

#[cfg(windows)]
mod directwrite_backend;
mod fontdue_backend;

pub use atlas::*;
pub use platform::*;

// ── LRU cache ────────────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::hash::Hash;

/// Clock-based LRU eviction cache. O(cap) eviction; designed for small, fixed
/// caps where maintaining a separate doubly-linked list is overkill.
pub(crate) struct LruCache<K, V> {
    map: HashMap<K, (V, u64)>,
    clock: u64,
    cap: usize,
}

impl<K: Eq + Hash + Clone, V> LruCache<K, V> {
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            map: HashMap::with_capacity(cap.min(256)),
            clock: 0,
            cap,
        }
    }

    /// Returns a shared reference to the value and bumps its recency timestamp.
    pub(crate) fn get(&mut self, k: &K) -> Option<&V> {
        if let Some((v, ts)) = self.map.get_mut(k) {
            self.clock += 1;
            *ts = self.clock;
            Some(v)
        } else {
            None
        }
    }

    /// Insert `k → v`, evicting the least-recently-used entry when at capacity.
    pub(crate) fn insert(&mut self, k: K, v: V) {
        if self.map.len() >= self.cap {
            if let Some(key) = self
                .map
                .iter()
                .min_by_key(|(_, (_, ts))| *ts)
                .map(|(k, _)| k.clone())
            {
                self.map.remove(&key);
            }
        }
        self.clock += 1;
        self.map.insert(k, (v, self.clock));
    }
}
