//! Bounded LRU cache for rendering.
//!
//! Stores values by cache key with a fixed memory budget. When over budget,
//! the least recently used entry is evicted.

use super::super::display_list::CacheKey;
use std::collections::HashMap;

struct CacheEntry<T> {
    value: T,
    size_bytes: usize,
    last_used: u64,
}

/// LRU cache with a maximum size in bytes.
pub struct BoundedCache<T> {
    entries: HashMap<CacheKey, CacheEntry<T>>,
    total_size: usize,
    max_size_bytes: usize,
    access_counter: u64,
}

impl<T> BoundedCache<T> {
    /// Creates a cache that will evict when total size exceeds the given bytes.
    pub fn new(max_size_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            total_size: 0,
            max_size_bytes,
            access_counter: 0,
        }
    }

    /// Returns the value for the key if present and bumps its LRU time.
    pub fn get(&mut self, key: &CacheKey) -> Option<&T> {
        if let Some(entry) = self.entries.get_mut(key) {
            self.access_counter += 1;
            entry.last_used = self.access_counter;
            Some(&entry.value)
        } else {
            None
        }
    }

    /// Inserts a value for the key; evicts LRU entries until the new entry fits.
    pub fn insert(&mut self, key: CacheKey, value: T, size_bytes: usize) {
        while self.total_size + size_bytes > self.max_size_bytes && !self.entries.is_empty() {
            self.evict_lru();
        }

        if let Some(old_entry) = self.entries.remove(&key) {
            self.total_size -= old_entry.size_bytes;
        }

        self.access_counter += 1;
        self.entries.insert(
            key,
            CacheEntry {
                value,
                size_bytes,
                last_used: self.access_counter,
            },
        );
        self.total_size += size_bytes;
    }

    /// Removes the entry for the key and returns its value.
    pub fn remove(&mut self, key: &CacheKey) -> Option<T> {
        if let Some(entry) = self.entries.remove(key) {
            self.total_size -= entry.size_bytes;
            Some(entry.value)
        } else {
            None
        }
    }

    /// Removes all entries and resets size to zero.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.total_size = 0;
    }

    /// Returns the total size in bytes of all entries.
    pub fn current_size(&self) -> usize {
        self.total_size
    }

    /// Returns the maximum size in bytes before eviction.
    pub fn max_size(&self) -> usize {
        self.max_size_bytes
    }

    fn evict_lru(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let mut lru_key = None;
        let mut lru_time = u64::MAX;

        for (key, entry) in &self.entries {
            if entry.last_used < lru_time {
                lru_time = entry.last_used;
                lru_key = Some(*key);
            }
        }

        if let Some(key) = lru_key {
            if let Some(entry) = self.entries.remove(&key) {
                self.total_size -= entry.size_bytes;
            }
        }
    }
}
