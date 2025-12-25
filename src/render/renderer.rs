//! Renderer trait for different rendering backends

use crate::AureaResult;
use super::surface::{Surface, SurfaceInfo};
use super::types::{Color, Rect, Point, Paint};

/// Drawing context for rendering operations
pub trait DrawingContext {
    /// Clear the canvas with a color
    fn clear(&mut self, color: Color) -> AureaResult<()>;

    /// Draw a rectangle
    fn draw_rect(&mut self, rect: Rect, paint: &Paint) -> AureaResult<()>;

    /// Draw a circle
    fn draw_circle(&mut self, center: Point, radius: f32, paint: &Paint) -> AureaResult<()>;

    /// Draw text at a position
    fn draw_text(&mut self, text: &str, position: Point, paint: &Paint) -> AureaResult<()>;
}

/// Renderer trait for different backends
pub trait Renderer: Send + Sync {
    /// Initialize the renderer with a native surface
    fn init(&mut self, surface: Surface, info: SurfaceInfo) -> AureaResult<()>;

    /// Resize the rendering surface
    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()>;

    /// Begin a new frame and get a drawing context
    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>>;

    /// End the current frame and present
    fn end_frame(&mut self) -> AureaResult<()>;

    /// Cleanup resources
    fn cleanup(&mut self);
}

/// Placeholder renderer implementation
/// This will be replaced with actual Skia/Vello implementations
pub struct PlaceholderRenderer {
    initialized: bool,
}

impl PlaceholderRenderer {
    pub fn new() -> Self {
        Self {
            initialized: false,
        }
    }
}

impl Renderer for PlaceholderRenderer {
    fn init(&mut self, _surface: Surface, _info: SurfaceInfo) -> AureaResult<()> {
        self.initialized = true;
        Ok(())
    }

    fn resize(&mut self, _width: u32, _height: u32) -> AureaResult<()> {
        Ok(())
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        Ok(Box::new(PlaceholderDrawingContext))
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        Ok(())
    }

    fn cleanup(&mut self) {
        self.initialized = false;
    }
}

struct PlaceholderDrawingContext;

impl DrawingContext for PlaceholderDrawingContext {
    fn clear(&mut self, _color: Color) -> AureaResult<()> {
        Ok(())
    }

    fn draw_rect(&mut self, _rect: Rect, _paint: &Paint) -> AureaResult<()> {
        Ok(())
    }

    fn draw_circle(&mut self, _center: Point, _radius: f32, _paint: &Paint) -> AureaResult<()> {
        Ok(())
    }

    fn draw_text(&mut self, _text: &str, _position: Point, _paint: &Paint) -> AureaResult<()> {
        Ok(())
    }
}

