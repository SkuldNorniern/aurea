//! Keys and shared stores for cached text runs.

use super::FontRef;
use crate::text::fonts;
use crate::text::layout::{TextAlign, WordBreak};
use crate::text::{CacheKind, LruCache, budget, shared};
use aurea_foundation::lock;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

/// What a cached run or layout was built from beyond its text and font.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Mode {
    Run(Option<bool>),
    Layout {
        max_width: Option<u32>,
        align: TextAlign,
        line_height: u32,
        word_break: WordBreak,
    },
}

/// Identity of a cached run. `key` is the lookup; `font` is a second hash of the font and mode,
/// seeded apart, that a hit must also match.
#[derive(Clone, Copy)]
pub(super) struct Ident {
    pub(super) key: u64,
    pub(super) font: u64,
}

impl Ident {
    pub(super) fn new(text: &str, font: FontRef, mode: Mode) -> Self {
        let font = font_check(font, mode);
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        font.hash(&mut hasher);
        Self {
            key: hasher.finish(),
            font,
        }
    }
}

fn font_check(font: FontRef, mode: Mode) -> u64 {
    let mut hasher = DefaultHasher::new();
    0x6175_7265_6166_6f6eu64.hash(&mut hasher);
    mode.hash(&mut hasher);
    hash_font(font, &mut hasher);
    hasher.finish()
}

fn hash_font(font: FontRef, hasher: &mut impl Hasher) {
    for byte in font.family.as_bytes() {
        byte.to_ascii_lowercase().hash(hasher);
    }
    font.size.to_bits().hash(hasher);
    font.weight.hash(hasher);
    font.style.hash(hasher);
    font.features.hash(hasher);
    font.variation_hash().hash(hasher);
    font.vertical.hash(hasher);
    fonts::generation().hash(hasher);
}

/// A cached value and the text it was built from.
struct Entry<V> {
    text: Box<str>,
    font: u64,
    value: V,
}

/// A shared LRU of text runs, bounded by entries and bytes.
pub(super) struct Memo<V> {
    lru: Arc<Mutex<LruCache<u64, Entry<V>>>>,
}

impl<V: Clone + Send + 'static> Memo<V> {
    pub(super) fn new(kind: CacheKind, cap: usize, default_bytes: usize) -> Self {
        let lru = LruCache::with_budget(cap, budget(kind, default_bytes));
        Self {
            lru: shared(kind, lru),
        }
    }

    /// The value stored under `id`, when its text and font also match.
    pub(super) fn get(&self, id: Ident, text: &str) -> Option<V> {
        let mut lru = lock(&self.lru);
        let entry = lru.get(&id.key)?;
        if entry.font == id.font && *entry.text == *text {
            Some(entry.value.clone())
        } else {
            None
        }
    }

    /// Stores `value`, which takes `bytes` beside the text it was built from.
    pub(super) fn put(&self, id: Ident, text: &str, value: V, bytes: usize) {
        let entry = Entry {
            text: text.into(),
            font: id.font,
            value,
        };
        lock(&self.lru).insert_sized(id.key, entry, bytes + text.len());
    }

    #[cfg(test)]
    pub(super) fn bytes(&self) -> usize {
        lock(&self.lru).bytes()
    }
}
