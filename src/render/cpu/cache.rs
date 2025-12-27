//! Bounded LRU cache for expensive rendering operations
//!
//! This cache stores pre-rendered content (glyphs, complex paths, etc.)
//! with a hard memory budget. When the budget is exceeded, least-recently-used
//! items are evicted.

use std::collections::HashMap;
use std::hash::Hash;
use super::super::display_list::CacheKey;

/// Cache entry with LRU tracking
struct CacheEntry<T> {
    value: T,
    size_bytes: usize,
    last_used: u64,
}

/// Bounded LRU cache with memory budget
pub struct BoundedCache<T> {
    entries: HashMap<CacheKey, CacheEntry<T>>,
    total_size: usize,
    max_size_bytes: usize,
    access_counter: u64,
}

impl<T> BoundedCache<T> {
    /// Create a new cache with a memory budget in bytes
    pub fn new(max_size_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            total_size: 0,
            max_size_bytes,
            access_counter: 0,
        }
    }
    
    /// Get an item from the cache
    pub fn get(&mut self, key: &CacheKey) -> Option<&T> {
        if let Some(entry) = self.entries.get_mut(key) {
            self.access_counter += 1;
            entry.last_used = self.access_counter;
            Some(&entry.value)
        } else {
            None
        }
    }
    
    /// Insert an item into the cache
    pub fn insert(&mut self, key: CacheKey, value: T, size_bytes: usize) {
        // Evict if necessary
        while self.total_size + size_bytes > self.max_size_bytes && !self.entries.is_empty() {
            self.evict_lru();
        }
        
        // Remove existing entry if present
        if let Some(old_entry) = self.entries.remove(&key) {
            self.total_size -= old_entry.size_bytes;
        }
        
        // Insert new entry
        self.access_counter += 1;
        self.entries.insert(key, CacheEntry {
            value,
            size_bytes,
            last_used: self.access_counter,
        });
        self.total_size += size_bytes;
    }
    
    /// Remove an item from the cache
    pub fn remove(&mut self, key: &CacheKey) -> Option<T> {
        if let Some(entry) = self.entries.remove(key) {
            self.total_size -= entry.size_bytes;
            Some(entry.value)
        } else {
            None
        }
    }
    
    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.total_size = 0;
    }
    
    /// Get current memory usage
    pub fn current_size(&self) -> usize {
        self.total_size
    }
    
    /// Get memory budget
    pub fn max_size(&self) -> usize {
        self.max_size_bytes
    }
    
    /// Evict the least recently used entry
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

