//! Glyph atlas management for text rendering
//!
//! Caches rasterized glyphs in a texture atlas to avoid re-rasterizing
//! the same glyphs repeatedly. Uses bounded LRU cache.

use super::super::types::Font;
use crate::AureaResult;
use std::{
    collections::HashMap,
    collections::VecDeque,
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
    order: Mutex<VecDeque<GlyphKey>>,
    sizes: Mutex<HashMap<GlyphKey, usize>>,
    memory_budget: usize, // Bytes
    current_memory: Mutex<usize>,
}

impl GlyphAtlas {
    pub fn new(memory_budget_mb: usize) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            order: Mutex::new(VecDeque::new()),
            sizes: Mutex::new(HashMap::new()),
            memory_budget: memory_budget_mb * 1024 * 1024,
            current_memory: Mutex::new(0),
        }
    }

    /// Get a glyph from cache, or None if not cached
    pub fn get(&self, key: &GlyphKey) -> Option<Arc<GlyphBitmap>> {
        let cache = crate::sync::lock(&self.cache);
        let mut order = crate::sync::lock(&self.order);
        let glyph = cache.get(key).cloned();
        if glyph.is_some() {
            if let Some(pos) = order.iter().position(|k| k == key) {
                order.remove(pos);
            }
            order.push_back(*key);
        }
        glyph
    }

    /// Store a glyph in the cache
    pub fn put(&self, key: GlyphKey, bitmap: GlyphBitmap) -> AureaResult<()> {
        let memory_used = (bitmap.width * bitmap.height * 4) as usize;

        let mut cache = crate::sync::lock(&self.cache);
        let mut order = crate::sync::lock(&self.order);
        let mut sizes = crate::sync::lock(&self.sizes);
        let mut current_memory = crate::sync::lock(&self.current_memory);

        if memory_used > self.memory_budget {
            cache.clear();
            order.clear();
            sizes.clear();
            *current_memory = 0;
        }

        while *current_memory + memory_used > self.memory_budget && !order.is_empty() {
            if let Some(oldest) = order.pop_front() {
                if let Some(size) = sizes.remove(&oldest) {
                    *current_memory = current_memory.saturating_sub(size);
                }
                cache.remove(&oldest);
            }
        }

        cache.insert(key, Arc::new(bitmap));
        order.push_back(key);
        sizes.insert(key, memory_used);
        *current_memory += memory_used;

        Ok(())
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = crate::sync::lock(&self.cache);
        let mut order = crate::sync::lock(&self.order);
        let mut sizes = crate::sync::lock(&self.sizes);
        cache.clear();
        order.clear();
        sizes.clear();
        let mut current_memory = crate::sync::lock(&self.current_memory);
        *current_memory = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::types::Font;

    #[test]
    fn glyph_key_same_font_char_equals() {
        let font = Font::new("Sans", 16.0);
        let k1 = GlyphKey::new(&font, 'A' as u32);
        let k2 = GlyphKey::new(&font, 'A' as u32);
        assert_eq!(k1, k2);
    }

    #[test]
    fn glyph_key_different_char_different() {
        let font = Font::new("Sans", 16.0);
        let k1 = GlyphKey::new(&font, 'A' as u32);
        let k2 = GlyphKey::new(&font, 'B' as u32);
        assert_ne!(k1, k2);
    }

    #[test]
    fn glyph_key_different_font_different() {
        let f1 = Font::new("Sans", 16.0);
        let f2 = Font::new("Serif", 16.0);
        let k1 = GlyphKey::new(&f1, 'A' as u32);
        let k2 = GlyphKey::new(&f2, 'A' as u32);
        assert_ne!(k1, k2);
    }

    #[test]
    fn glyph_key_different_size_different() {
        let k1 = GlyphKey::new(&Font::new("Sans", 16.0), 'A' as u32);
        let k2 = GlyphKey::new(&Font::new("Sans", 24.0), 'A' as u32);
        assert_ne!(k1, k2);
    }
}
