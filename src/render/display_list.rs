//! Display list management with cacheability metadata
//!
//! This module provides the foundation for efficient rendering by adding
//! stable node IDs, cache keys, bounds tracking, and opacity flags to
//! display items. This enables partial redraw, caching, and damage tracking.

use super::types::Rect;
use crate::AureaResult;

/// Stable node identifier for display items
/// Used for cache invalidation and tracking changes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

impl NodeId {
    /// Generate a new unique node ID
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
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
    pub node_id: NodeId,
    /// Cache key for this item (hash of content + style + scale + font)
    pub cache_key: CacheKey,
    /// Bounding rectangle for damage calculation
    pub bounds: Rect,
    /// Whether this item is fully opaque (skips damage for covered regions)
    pub opaque: bool,
    /// Interactive ID if this shape should respond to mouse/touch events
    pub interactive_id: Option<super::types::InteractiveId>,
    /// The actual draw command
    pub command: super::renderer::DrawCommand,
}

impl DisplayItem {
    /// Create a new display item
    pub fn new(
        node_id: NodeId,
        cache_key: CacheKey,
        bounds: Rect,
        opaque: bool,
        command: super::renderer::DrawCommand,
    ) -> Self {
        Self {
            node_id,
            cache_key,
            bounds,
            opaque,
            interactive_id: None,
            command,
        }
    }

    /// Create a new interactive display item
    pub fn new_interactive(
        node_id: NodeId,
        cache_key: CacheKey,
        bounds: Rect,
        opaque: bool,
        interactive_id: super::types::InteractiveId,
        command: super::renderer::DrawCommand,
    ) -> Self {
        Self {
            node_id,
            cache_key,
            bounds,
            opaque,
            interactive_id: Some(interactive_id),
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

/// Display list with metadata support
#[derive(Debug, Default)]
pub struct DisplayList {
    items: Vec<DisplayItem>,
    clip_stack: Vec<super::types::Path>,
    transform_stack: Vec<super::types::Transform>,
    opacity_stack: Vec<f32>,
}

impl DisplayList {
    /// Create a new empty display list
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a display item to the list
    pub fn push(&mut self, item: DisplayItem) {
        self.items.push(item);
    }

    /// Get all items
    pub fn items(&self) -> &[DisplayItem] {
        &self.items
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.items.clear();
        self.clip_stack.clear();
        self.transform_stack.clear();
        self.opacity_stack.clear();
    }

    /// Push a clip path onto the stack
    pub fn push_clip(&mut self, path: super::types::Path) {
        self.clip_stack.push(path);
    }

    /// Pop a clip path from the stack
    pub fn pop_clip(&mut self) -> Option<super::types::Path> {
        self.clip_stack.pop()
    }

    /// Get current clip stack depth
    pub fn clip_depth(&self) -> usize {
        self.clip_stack.len()
    }

    /// Push a transform onto the stack
    pub fn push_transform(&mut self, transform: super::types::Transform) {
        self.transform_stack.push(transform);
    }

    /// Pop a transform from the stack
    pub fn pop_transform(&mut self) -> Option<super::types::Transform> {
        self.transform_stack.pop()
    }

    /// Get current transform stack depth
    pub fn transform_depth(&self) -> usize {
        self.transform_stack.len()
    }

    /// Push an opacity value onto the stack
    pub fn push_opacity(&mut self, opacity: f32) {
        self.opacity_stack.push(opacity);
    }

    /// Pop an opacity value from the stack
    pub fn pop_opacity(&mut self) -> Option<f32> {
        self.opacity_stack.pop()
    }

    /// Get current opacity stack depth
    pub fn opacity_depth(&self) -> usize {
        self.opacity_stack.len()
    }

    /// Compute the effective opacity from the stack
    pub fn effective_opacity(&self) -> f32 {
        self.opacity_stack.iter().product()
    }
}
