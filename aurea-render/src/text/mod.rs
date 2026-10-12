//! Text rendering for Canvas.

pub mod atlas;
pub mod font;
pub mod platform;

#[cfg(windows)]
mod directwrite_backend;
mod fontdue_backend;

pub use atlas::*;
pub use platform::*;

use std::collections::HashMap;
use std::hash::Hash;

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

    pub(crate) fn get(&mut self, k: &K) -> Option<&V> {
        if let Some((v, ts)) = self.map.get_mut(k) {
            self.clock += 1;
            *ts = self.clock;
            Some(v)
        } else {
            None
        }
    }

    pub(crate) fn insert(&mut self, k: K, v: V) {
        if self.map.len() >= self.cap
            && let Some(key) = self
                .map
                .iter()
                .min_by_key(|(_, (_, ts))| *ts)
                .map(|(k, _)| k.clone())
        {
            self.map.remove(&key);
        }
        self.clock += 1;
        self.map.insert(k, (v, self.clock));
    }
}
