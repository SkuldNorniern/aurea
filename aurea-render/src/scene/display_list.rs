//! Display list management with cacheability metadata.

use crate::scene::command::DrawCommand;
use crate::types::{BlendMode, InteractiveId, Path, Rect};
use std::sync::Arc;

/// Where an item sits in this frame's display list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayIndex(pub u64);

impl DisplayIndex {
    /// A process-unique value, for a caller that needs one outside a frame.
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for DisplayIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache key for display items Hash of content + style + scale + font to enable caching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey(pub u64);

impl CacheKey {
    /// Compute a cache key from content hash.
    pub fn from_hash(hash: u64) -> Self {
        Self(hash)
    }

    /// Compute cache key from multiple components.
    pub fn compute(content_hash: u64, style_hash: u64, scale: f32, font_hash: u64) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content_hash.hash(&mut hasher);
        style_hash.hash(&mut hasher);
        (scale.to_bits()).hash(&mut hasher);
        font_hash.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// Display item with cacheability metadata Wraps a draw command with metadata needed for efficient
/// rendering.
#[derive(Debug, Clone)]
pub struct DisplayItem {
    /// Stable identity for this item.
    pub index: DisplayIndex,
    /// Cache key for this item (hash of content + style + scale + font).
    pub cache_key: CacheKey,
    /// Bounding rectangle for damage calculation.
    pub bounds: Rect,
    /// Whether this item is fully opaque (skips damage for covered regions).
    pub opaque: bool,
    /// Interactive ID if this shape should respond to mouse/touch events.
    pub interactive_id: Option<InteractiveId>,
    /// Blend mode when compositing this item.
    pub blend_mode: BlendMode,
    /// Opacity this item was recorded under, `0.0..=1.0`.
    pub opacity: f32,
    /// Active clip when this item was recorded, in physical pixels.
    pub clip: Option<Rect>,
    /// Non-rectangular clips this item was recorded under, in physical pixels; `clip` holds
    /// their bounds.
    pub clip_paths: Option<Arc<[Path]>>,
    /// The actual draw command.
    pub command: DrawCommand,
}

impl DisplayItem {
    /// Create a new display item.
    pub fn new(
        index: DisplayIndex,
        cache_key: CacheKey,
        bounds: Rect,
        opaque: bool,
        blend_mode: BlendMode,
        command: DrawCommand,
    ) -> Self {
        Self {
            index,
            cache_key,
            bounds,
            opaque,
            interactive_id: None,
            blend_mode,
            opacity: 1.0,
            clip: None,
            clip_paths: None,
            command,
        }
    }

    /// Sets the clip this item was recorded under.
    #[must_use]
    pub fn with_clip(mut self, clip: Option<Rect>) -> Self {
        self.clip = clip;
        self
    }

    /// Sets the non-rectangular clips this item was recorded under.
    #[must_use]
    pub fn with_clip_paths(mut self, paths: Option<Arc<[Path]>>) -> Self {
        self.clip_paths = paths;
        self
    }

    /// Sets the opacity this item was recorded under.
    #[must_use]
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Create a new interactive display item.
    pub fn new_interactive(
        index: DisplayIndex,
        cache_key: CacheKey,
        bounds: Rect,
        opaque: bool,
        interactive_id: InteractiveId,
        blend_mode: BlendMode,
        command: DrawCommand,
    ) -> Self {
        Self {
            index,
            cache_key,
            bounds,
            opaque,
            interactive_id: Some(interactive_id),
            blend_mode,
            opacity: 1.0,
            clip: None,
            clip_paths: None,
            command,
        }
    }

    /// Check if this item intersects with a damage region.
    pub fn intersects(&self, damage: &Rect) -> bool {
        self.bounds.x < damage.x + damage.width
            && self.bounds.x + self.bounds.width > damage.x
            && self.bounds.y < damage.y + damage.height
            && self.bounds.y + self.bounds.height > damage.y
    }
}

/// The draw commands for one frame, in submission order.
#[derive(Debug, Default)]
pub struct DisplayList {
    items: Vec<DisplayItem>,
    last_clear: usize,
}

impl DisplayList {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an item.
    pub fn push(&mut self, item: DisplayItem) {
        if matches!(item.command, DrawCommand::Clear(_)) {
            self.last_clear = self.items.len();
        }
        self.items.push(item);
    }

    /// The items, in submission order.
    pub fn items(&self) -> &[DisplayItem] {
        &self.items
    }

    pub(crate) fn items_since_clear(&self) -> &[DisplayItem] {
        &self.items[self.last_clear..]
    }

    /// Drops every item, keeping the allocation for the next frame.
    pub fn clear(&mut self) {
        self.items.clear();
        self.last_clear = 0;
    }

    /// How many items the list holds.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the list holds nothing.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
