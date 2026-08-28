//! Display list management with cacheability metadata
//!
//! This module provides the foundation for efficient rendering by adding
//! a display index, cache key, bounds, clip, opacity and blend mode to each
//! item, which is what partial redraw, caching and damage tracking work from.

use super::types::Rect;

/// Where an item sits in this frame's display list.
///
/// Positional, not persistent. The recorder counts from zero each frame, so an
/// item keeps its index only as long as it keeps its place in submission order:
/// inserting one shape at the front renumbers everything after it. That is what
/// the positional damage diff wants, and it is enough for it.
///
/// It is deliberately not called `NodeId`. When Aurea grows a retained widget
/// tree it will want identities that survive reordering — a widget id, and a
/// visual node id — and quietly promoting this one would give them a meaning it
/// has never had.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayIndex(pub u64);

impl DisplayIndex {
    /// A process-unique value, for a caller that needs one outside a frame.
    ///
    /// Not what the recorder uses: it numbers from zero per frame, because the
    /// diff compares positions.
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

/// Cache key for display items
/// Hash of content + style + scale + font to enable caching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey(pub u64);

impl CacheKey {
    /// Compute a cache key from content hash
    pub fn from_hash(hash: u64) -> Self {
        Self(hash)
    }

    /// Compute cache key from multiple components
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

/// Display item with cacheability metadata
/// Wraps a draw command with metadata needed for efficient rendering
#[derive(Debug, Clone)]
pub struct DisplayItem {
    /// Stable identity for this item
    pub index: DisplayIndex,
    /// Cache key for this item (hash of content + style + scale + font)
    pub cache_key: CacheKey,
    /// Bounding rectangle for damage calculation
    pub bounds: Rect,
    /// Whether this item is fully opaque (skips damage for covered regions)
    pub opaque: bool,
    /// Interactive ID if this shape should respond to mouse/touch events
    pub interactive_id: Option<super::types::InteractiveId>,
    /// Blend mode when compositing this item
    pub blend_mode: super::types::BlendMode,
    /// Opacity this item was recorded under, `0.0..=1.0`.
    ///
    /// Resolved at record time for the same reason as [`Self::clip`], and
    /// applied per item: `set_alpha` is drawing state, not a layer, so two
    /// overlapping shapes drawn at half alpha composite against each other
    /// rather than as one group.
    pub opacity: f32,
    /// Active clip when this item was recorded, in physical pixels.
    ///
    /// Resolved at record time rather than replayed as push/pop commands:
    /// partial repaint renders an arbitrary subset of the list, so state that
    /// only exists between a push and a pop would go missing precisely when
    /// the frame is repainted in pieces.
    pub clip: Option<Rect>,
    /// The actual draw command
    pub command: super::command::DrawCommand,
}

impl DisplayItem {
    /// Create a new display item
    pub fn new(
        index: DisplayIndex,
        cache_key: CacheKey,
        bounds: Rect,
        opaque: bool,
        blend_mode: super::types::BlendMode,
        command: super::command::DrawCommand,
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
            command,
        }
    }

    /// Sets the clip this item was recorded under.
    #[must_use]
    pub fn with_clip(mut self, clip: Option<Rect>) -> Self {
        self.clip = clip;
        self
    }

    /// Sets the opacity this item was recorded under.
    #[must_use]
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Create a new interactive display item
    pub fn new_interactive(
        index: DisplayIndex,
        cache_key: CacheKey,
        bounds: Rect,
        opaque: bool,
        interactive_id: super::types::InteractiveId,
        blend_mode: super::types::BlendMode,
        command: super::command::DrawCommand,
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
            command,
        }
    }

    /// Check if this item intersects with a damage region
    pub fn intersects(&self, damage: &Rect) -> bool {
        self.bounds.x < damage.x + damage.width
            && self.bounds.x + self.bounds.width > damage.x
            && self.bounds.y < damage.y + damage.height
            && self.bounds.y + self.bounds.height > damage.y
    }
}

/// The draw commands for one frame, in submission order.
///
/// Just the items: the clip, transform and opacity stacks that used to live
/// here were written by `save`/`restore` and read by nothing. Each item now
/// carries its own resolved clip, transform and opacity, which is what partial
/// repaint needs — it renders an arbitrary subset of the list, so state that
/// only exists between a push and a pop would be missing exactly when a frame
/// is repainted in pieces.
#[derive(Debug, Default)]
pub struct DisplayList {
    items: Vec<DisplayItem>,
}

impl DisplayList {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an item.
    pub fn push(&mut self, item: DisplayItem) {
        self.items.push(item);
    }

    /// The items, in submission order.
    pub fn items(&self) -> &[DisplayItem] {
        &self.items
    }

    /// Drops every item, keeping the allocation for the next frame.
    pub fn clear(&mut self) {
        self.items.clear();
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
