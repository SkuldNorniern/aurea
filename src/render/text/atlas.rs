//! Glyph atlas management for text rendering
//!
//! Caches rasterized glyphs in a texture atlas to avoid re-rasterizing
//! the same glyphs repeatedly. Uses bounded LRU cache.

use super::super::types::{Color, Font};
use crate::AureaResult;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Glyph identifier (font + size + character)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    font_family_hash: u64,
    size_bits: u32, // f32 as bits for hashing
    char_code: u32,
}

impl GlyphKey {
    pub fn new(font: &Font, char_code: u32) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        font.family.hash(&mut hasher);
        font.weight.hash(&mut hasher);
        font.style.hash(&mut hasher);
        let font_family_hash = hasher.finish();

        Self {
            font_family_hash,
            size_bits: font.size.to_bits(), // Convert f32 to u32 bits for hashing
            char_code,
        }
    }
}

/// Rasterized glyph data
#[derive(Debug, Clone)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
}

/// Bounded glyph atlas cache
pub struct GlyphAtlas {
    cache: Mutex<HashMap<GlyphKey, Arc<GlyphBitmap>>>,
    memory_budget: usize, // Bytes
    current_memory: Mutex<usize>,
}

impl GlyphAtlas {
    pub fn new(memory_budget_mb: usize) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            memory_budget: memory_budget_mb * 1024 * 1024,
            current_memory: Mutex::new(0),
        }
    }

    /// Get a glyph from cache, or None if not cached
    pub fn get(&self, key: &GlyphKey) -> Option<Arc<GlyphBitmap>> {
        let cache = self.cache.lock().unwrap();
        cache.get(key).cloned()
    }

    /// Store a glyph in the cache
    pub fn put(&self, key: GlyphKey, bitmap: GlyphBitmap) -> AureaResult<()> {
        let memory_used = (bitmap.width * bitmap.height * 4) as usize;

        let mut cache = self.cache.lock().unwrap();
        let mut current_memory = self.current_memory.lock().unwrap();

        // Simple eviction: if over budget, clear cache
        // TODO: Implement proper LRU eviction
        if *current_memory + memory_used > self.memory_budget {
            cache.clear();
            *current_memory = 0;
        }

        cache.insert(key, Arc::new(bitmap));
        *current_memory += memory_used;

        Ok(())
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
        let mut current_memory = self.current_memory.lock().unwrap();
        *current_memory = 0;
    }
}
