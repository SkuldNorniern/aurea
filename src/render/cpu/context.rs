//! CPU drawing context that builds a display list
//!
//! This context collects drawing commands into a display list with metadata
//! (node IDs, cache keys, bounds, opacity flags) for efficient rendering.

use crate::AureaResult;
use super::super::renderer::DrawingContext;
use super::super::types::*;
use super::super::display_list::{DisplayList, DisplayItem, NodeId, CacheKey};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Drawing state for save/restore
struct DrawingState {
    transform: Transform,
    opacity: f32,
    clip: Option<Path>,
}

/// Drawing context that builds a display list for CPU rasterization
pub struct CpuDrawingContext {
    display_list: *mut DisplayList,
    current_node_id: NodeId,
    state_stack: Vec<DrawingState>,
    current_transform: Transform,
    current_opacity: f32,
    current_clip: Option<Path>,
    scale_factor: f32,
}

impl CpuDrawingContext {
    /// Create a new CPU drawing context that writes to a display list
    pub fn new(display_list: *mut DisplayList) -> Self {
        Self {
            display_list,
            current_node_id: NodeId::new(),
            state_stack: Vec::new(),
            current_transform: Transform::identity(),
            current_opacity: 1.0,
            current_clip: None,
            scale_factor: 1.0,
        }
    }
    
    /// Set the scale factor for cache key computation
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale;
    }
    
    /// Get mutable reference to display list (unsafe but necessary for this design)
    unsafe fn display_list_mut(&mut self) -> &mut DisplayList {
        &mut *self.display_list
    }
    
    /// Compute a cache key for a draw command
    fn compute_cache_key(&self, command: &super::super::renderer::DrawCommand) -> CacheKey {
        let mut hasher = DefaultHasher::new();
        // Hash the command type and parameters
        match command {
            super::super::renderer::DrawCommand::Clear(color) => {
                "Clear".hash(&mut hasher);
                color.r.hash(&mut hasher);
                color.g.hash(&mut hasher);
                color.b.hash(&mut hasher);
                color.a.hash(&mut hasher);
            }
            super::super::renderer::DrawCommand::DrawRect(rect, paint) => {
                "DrawRect".hash(&mut hasher);
                rect.x.to_bits().hash(&mut hasher);
                rect.y.to_bits().hash(&mut hasher);
                rect.width.to_bits().hash(&mut hasher);
                rect.height.to_bits().hash(&mut hasher);
                paint.color.r.hash(&mut hasher);
                paint.color.g.hash(&mut hasher);
                paint.color.b.hash(&mut hasher);
                paint.color.a.hash(&mut hasher);
                paint.style.hash(&mut hasher);
                paint.stroke_width.to_bits().hash(&mut hasher);
            }
            super::super::renderer::DrawCommand::DrawCircle(center, radius, paint) => {
                "DrawCircle".hash(&mut hasher);
                center.x.to_bits().hash(&mut hasher);
                center.y.to_bits().hash(&mut hasher);
                radius.to_bits().hash(&mut hasher);
                paint.color.r.hash(&mut hasher);
                paint.color.g.hash(&mut hasher);
                paint.color.b.hash(&mut hasher);
                paint.color.a.hash(&mut hasher);
                paint.style.hash(&mut hasher);
                paint.stroke_width.to_bits().hash(&mut hasher);
            }
            _ => {
                // For other commands, use a simple hash
                format!("{:?}", command).hash(&mut hasher);
            }
        }
        // Include transform, opacity, and scale factor in cache key
        // Hash transform components
        self.current_transform.m11.to_bits().hash(&mut hasher);
        self.current_transform.m12.to_bits().hash(&mut hasher);
        self.current_transform.m13.to_bits().hash(&mut hasher);
        self.current_transform.m21.to_bits().hash(&mut hasher);
        self.current_transform.m22.to_bits().hash(&mut hasher);
        self.current_transform.m23.to_bits().hash(&mut hasher);
        self.current_transform.m31.to_bits().hash(&mut hasher);
        self.current_transform.m32.to_bits().hash(&mut hasher);
        self.current_transform.m33.to_bits().hash(&mut hasher);
        self.current_opacity.to_bits().hash(&mut hasher);
        self.scale_factor.to_bits().hash(&mut hasher);
        
        CacheKey::from_hash(hasher.finish())
    }
    
    /// Apply transform to a point
    fn transform_point(&self, point: Point) -> Point {
        let x = self.current_transform.m11 * point.x + self.current_transform.m12 * point.y + self.current_transform.m13;
        let y = self.current_transform.m21 * point.x + self.current_transform.m22 * point.y + self.current_transform.m23;
        Point::new(x, y)
    }
    
    /// Apply transform to a rectangle (returns bounding box of transformed rect)
    fn transform_rect(&self, rect: Rect) -> Rect {
        // Transform all four corners
        let top_left = self.transform_point(Point::new(rect.x, rect.y));
        let top_right = self.transform_point(Point::new(rect.x + rect.width, rect.y));
        let bottom_left = self.transform_point(Point::new(rect.x, rect.y + rect.height));
        let bottom_right = self.transform_point(Point::new(rect.x + rect.width, rect.y + rect.height));
        
        // Find bounding box
        let min_x = top_left.x.min(top_right.x).min(bottom_left.x).min(bottom_right.x);
        let max_x = top_left.x.max(top_right.x).max(bottom_left.x).max(bottom_right.x);
        let min_y = top_left.y.min(top_right.y).min(bottom_left.y).min(bottom_right.y);
        let max_y = top_left.y.max(top_right.y).max(bottom_left.y).max(bottom_right.y);
        
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }
    
    /// Compute bounds for a draw command (with transform applied)
    fn compute_bounds(&self, command: &super::super::renderer::DrawCommand) -> Rect {
        match command {
            super::super::renderer::DrawCommand::Clear(_) => {
                // Clear affects entire canvas - use a large rect
                Rect::new(0.0, 0.0, f32::MAX, f32::MAX)
            }
            super::super::renderer::DrawCommand::DrawRect(rect, paint) => {
                let mut bounds = *rect;
                // For stroke, expand bounds by stroke width
                if paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0 {
                    let half_stroke = paint.stroke_width / 2.0;
                    bounds.x -= half_stroke;
                    bounds.y -= half_stroke;
                    bounds.width += paint.stroke_width;
                    bounds.height += paint.stroke_width;
                }
                // Apply transform
                self.transform_rect(bounds)
            }
            super::super::renderer::DrawCommand::DrawCircle(center, radius, paint) => {
                let mut bounds = Rect::new(
                    center.x - radius,
                    center.y - radius,
                    radius * 2.0,
                    radius * 2.0,
                );
                // For stroke, expand bounds by stroke width
                if paint.style == PaintStyle::Stroke && paint.stroke_width > 0.0 {
                    let half_stroke = paint.stroke_width / 2.0;
                    bounds.x -= half_stroke;
                    bounds.y -= half_stroke;
                    bounds.width += paint.stroke_width;
                    bounds.height += paint.stroke_width;
                }
                // Apply transform
                self.transform_rect(bounds)
            }
            _ => {
                // Default bounds
                Rect::new(0.0, 0.0, 0.0, 0.0)
            }
        }
    }
    
    /// Check if a command is opaque
    fn is_opaque(&self, command: &super::super::renderer::DrawCommand) -> bool {
        match command {
            super::super::renderer::DrawCommand::Clear(color) => color.a == 255,
            super::super::renderer::DrawCommand::DrawRect(_, paint) => {
                paint.color.a == 255 && paint.style == PaintStyle::Fill
            }
            super::super::renderer::DrawCommand::DrawCircle(_, _, paint) => {
                paint.color.a == 255 && paint.style == PaintStyle::Fill
            }
            _ => false,
        }
    }
    
    /// Add a draw command to the display list
    fn add_command(&mut self, command: super::super::renderer::DrawCommand) {
        let cache_key = self.compute_cache_key(&command);
        let bounds = self.compute_bounds(&command);
        let opaque = self.is_opaque(&command) && self.current_opacity >= 1.0;
        
        let item = DisplayItem::new(
            self.current_node_id,
            cache_key,
            bounds,
            opaque,
            command,
        );
        
        unsafe {
            self.display_list_mut().push(item);
        }
        self.current_node_id = NodeId::new();
    }
}

impl DrawingContext for CpuDrawingContext {
    fn clear(&mut self, color: Color) -> AureaResult<()> {
        self.add_command(super::super::renderer::DrawCommand::Clear(color));
        Ok(())
    }
    
    fn draw_rect(&mut self, rect: Rect, paint: &Paint) -> AureaResult<()> {
        self.add_command(super::super::renderer::DrawCommand::DrawRect(rect, paint.clone()));
        Ok(())
    }
    
    fn draw_circle(&mut self, center: Point, radius: f32, paint: &Paint) -> AureaResult<()> {
        self.add_command(super::super::renderer::DrawCommand::DrawCircle(center, radius, paint.clone()));
        Ok(())
    }
    
    fn draw_path(&mut self, _path: &Path, _paint: &Paint) -> AureaResult<()> {
        // TODO: Implement path drawing
        Ok(())
    }
    
    fn draw_text(&mut self, _text: &str, _point: Point, _paint: &Paint) -> AureaResult<()> {
        // TODO: Implement text drawing
        Ok(())
    }
    
    fn draw_text_with_font(&mut self, _text: &str, _point: Point, _font: &Font, _paint: &Paint) -> AureaResult<()> {
        // TODO: Implement text drawing with font
        Ok(())
    }
    
    fn draw_image(&mut self, _image: &Image, _position: Point) -> AureaResult<()> {
        // TODO: Implement image drawing
        Ok(())
    }
    
    fn draw_image_rect(&mut self, _image: &Image, _dest: Rect) -> AureaResult<()> {
        // TODO: Implement image drawing with rect
        Ok(())
    }
    
    fn draw_image_region(&mut self, _image: &Image, _src: Rect, _dest: Rect) -> AureaResult<()> {
        // TODO: Implement image region drawing
        Ok(())
    }
    
    fn measure_text(&mut self, _text: &str, _font: &Font) -> AureaResult<super::super::types::TextMetrics> {
        // TODO: Implement text measurement
        Ok(super::super::types::TextMetrics {
            width: 0.0,
            height: 0.0,
            ascent: 0.0,
            descent: 0.0,
            advance: 0.0,
        })
    }
    
    fn save(&mut self) -> AureaResult<()> {
        // Copy current state before borrowing
        let transform = self.current_transform;
        let opacity = self.current_opacity;
        let clip = self.current_clip.clone();
        
        // Save current state to stack
        self.state_stack.push(DrawingState {
            transform,
            opacity,
            clip: clip.clone(),
        });
        
        // Also push to display list for rendering
        unsafe {
            self.display_list_mut().push_transform(transform);
            self.display_list_mut().push_opacity(opacity);
            if let Some(ref clip_path) = clip {
                self.display_list_mut().push_clip(clip_path.clone());
            }
        }
        Ok(())
    }
    
    fn restore(&mut self) -> AureaResult<()> {
        // Restore state from stack
        if let Some(state) = self.state_stack.pop() {
            self.current_transform = state.transform;
            self.current_opacity = state.opacity;
            self.current_clip = state.clip;
        }
        
        // Also pop from display list
        unsafe {
            let _ = self.display_list_mut().pop_transform();
            let _ = self.display_list_mut().pop_opacity();
            let _ = self.display_list_mut().pop_clip();
        }
        Ok(())
    }
    
    fn transform(&mut self, transform: Transform) -> AureaResult<()> {
        self.current_transform = self.current_transform.multiply(transform);
        Ok(())
    }
    
    fn clip_rect(&mut self, rect: Rect) -> AureaResult<()> {
        // Convert rect to path
        let mut path = Path::new();
        path.commands.push(super::super::types::PathCommand::MoveTo(Point::new(rect.x, rect.y)));
        path.commands.push(super::super::types::PathCommand::LineTo(Point::new(rect.x + rect.width, rect.y)));
        path.commands.push(super::super::types::PathCommand::LineTo(Point::new(rect.x + rect.width, rect.y + rect.height)));
        path.commands.push(super::super::types::PathCommand::LineTo(Point::new(rect.x, rect.y + rect.height)));
        path.commands.push(super::super::types::PathCommand::Close);
        self.current_clip = Some(path);
        Ok(())
    }
    
    fn clip_path(&mut self, path: &Path) -> AureaResult<()> {
        self.current_clip = Some(path.clone());
        Ok(())
    }
    
    fn set_alpha(&mut self, alpha: f32) -> AureaResult<()> {
        self.current_opacity = alpha;
        Ok(())
    }
    
    fn set_blend_mode(&mut self, _mode: BlendMode) -> AureaResult<()> {
        // TODO: Implement blend modes
        Ok(())
    }
    
    fn fill_linear_gradient(&mut self, _gradient: &LinearGradient, _rect: Rect) -> AureaResult<()> {
        // TODO: Implement gradients
        Ok(())
    }
    
    fn fill_radial_gradient(&mut self, _gradient: &RadialGradient, _rect: Rect) -> AureaResult<()> {
        // TODO: Implement gradients
        Ok(())
    }
    
    fn hit_test_path(&mut self, _path: &Path, _point: Point) -> AureaResult<bool> {
        // TODO: Implement hit testing
        Ok(false)
    }
}

