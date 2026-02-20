//! Viewport and scrolling support for browser rendering

use super::types::{Point, Rect, Transform};

/// Viewport configuration for managing scrollable content
#[derive(Debug, Clone)]
pub struct Viewport {
    /// Content size (the full scrollable area)
    content_size: (f32, f32),
    /// Viewport size (visible area)
    viewport_size: (f32, f32),
    /// Scroll offset
    scroll_offset: (f32, f32),
    /// Maximum scroll offset
    max_scroll: (f32, f32),
}

impl Viewport {
    /// Create a new viewport
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            content_size: (viewport_width, viewport_height),
            viewport_size: (viewport_width, viewport_height),
            scroll_offset: (0.0, 0.0),
            max_scroll: (0.0, 0.0),
        }
    }

    /// Set the content size (scrollable area)
    pub fn set_content_size(&mut self, width: f32, height: f32) {
        self.content_size = (width, height);
        self.update_max_scroll();
    }

    /// Set the viewport size (visible area)
    pub fn set_viewport_size(&mut self, width: f32, height: f32) {
        self.viewport_size = (width, height);
        self.update_max_scroll();
    }

    /// Get the viewport rectangle
    pub fn viewport_rect(&self) -> Rect {
        Rect::new(0.0, 0.0, self.viewport_size.0, self.viewport_size.1)
    }

    /// Get the content rectangle
    pub fn content_rect(&self) -> Rect {
        Rect::new(0.0, 0.0, self.content_size.0, self.content_size.1)
    }

    /// Get the visible content rectangle (in content coordinates)
    pub fn visible_content_rect(&self) -> Rect {
        Rect::new(
            self.scroll_offset.0,
            self.scroll_offset.1,
            self.viewport_size.0,
            self.viewport_size.1,
        )
    }

    /// Scroll by a delta amount
    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll_to(self.scroll_offset.0 + dx, self.scroll_offset.1 + dy);
    }

    /// Scroll to a specific position
    pub fn scroll_to(&mut self, x: f32, y: f32) {
        self.scroll_offset.0 = x.clamp(0.0, self.max_scroll.0);
        self.scroll_offset.1 = y.clamp(0.0, self.max_scroll.1);
    }

    /// Get the current scroll offset
    pub fn scroll_offset(&self) -> (f32, f32) {
        self.scroll_offset
    }

    /// Get the maximum scroll offset
    pub fn max_scroll(&self) -> (f32, f32) {
        self.max_scroll
    }

    /// Get the transform to apply for viewport scrolling
    pub fn scroll_transform(&self) -> Transform {
        Transform::translate(-self.scroll_offset.0, -self.scroll_offset.1)
    }

    /// Convert viewport coordinates to content coordinates
    pub fn viewport_to_content(&self, point: Point) -> Point {
        Point::new(
            point.x + self.scroll_offset.0,
            point.y + self.scroll_offset.1,
        )
    }

    /// Convert content coordinates to viewport coordinates
    pub fn content_to_viewport(&self, point: Point) -> Point {
        Point::new(
            point.x - self.scroll_offset.0,
            point.y - self.scroll_offset.1,
        )
    }

    /// Check if a content rectangle is visible in the viewport
    pub fn is_visible(&self, rect: Rect) -> bool {
        let visible = self.visible_content_rect();
        rect.x + rect.width > visible.x
            && rect.x < visible.x + visible.width
            && rect.y + rect.height > visible.y
            && rect.y < visible.y + visible.height
    }

    /// Get the intersection of a content rectangle with the visible area
    pub fn clip_to_visible(&self, rect: Rect) -> Option<Rect> {
        let visible = self.visible_content_rect();

        let x = rect.x.max(visible.x);
        let y = rect.y.max(visible.y);
        let right = (rect.x + rect.width).min(visible.x + visible.width);
        let bottom = (rect.y + rect.height).min(visible.y + visible.height);

        if right > x && bottom > y {
            Some(Rect::new(x, y, right - x, bottom - y))
        } else {
            None
        }
    }

    /// Update maximum scroll values based on content and viewport sizes
    fn update_max_scroll(&mut self) {
        self.max_scroll.0 = (self.content_size.0 - self.viewport_size.0).max(0.0);
        self.max_scroll.1 = (self.content_size.1 - self.viewport_size.1).max(0.0);

        // Clamp current scroll to new max
        self.scroll_to(self.scroll_offset.0, self.scroll_offset.1);
    }

    /// Check if content can scroll horizontally
    pub fn can_scroll_horizontal(&self) -> bool {
        self.max_scroll.0 > 0.0
    }

    /// Check if content can scroll vertically
    pub fn can_scroll_vertical(&self) -> bool {
        self.max_scroll.1 > 0.0
    }

    /// Get the content size
    pub fn content_size(&self) -> (f32, f32) {
        self.content_size
    }

    /// Get the viewport size
    pub fn viewport_size(&self) -> (f32, f32) {
        self.viewport_size
    }
}
