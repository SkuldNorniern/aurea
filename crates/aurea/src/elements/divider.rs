//! Visual separator line (horizontal or vertical).
//!
//! Renders a simple line; useful for grouping UI sections.

use super::traits::Element;
use crate::render::{Canvas, Color, Paint, PaintStyle, Rect, RendererBackend};
use crate::AureaResult;
use std::os::raw::c_void;

/// Orientation of the divider line.
#[derive(Debug, Clone, Copy)]
pub enum DividerOrientation {
    Horizontal,
    Vertical,
}

/// A horizontal or vertical divider line.
pub struct Divider {
    canvas: Canvas,
    #[allow(dead_code)]
    orientation: DividerOrientation,
}

impl Divider {
    /// Default line thickness in pixels.
    const THICKNESS: u32 = 1;

    /// Create a horizontal divider (full width, 1px height).
    pub fn horizontal(width: u32) -> AureaResult<Self> {
        Self::new(DividerOrientation::Horizontal, width, Self::THICKNESS)
    }

    /// Create a vertical divider (1px width, full height).
    pub fn vertical(height: u32) -> AureaResult<Self> {
        Self::new(DividerOrientation::Vertical, Self::THICKNESS, height)
    }

    /// Create a divider with explicit dimensions.
    pub fn new(orientation: DividerOrientation, width: u32, height: u32) -> AureaResult<Self> {
        let (w, h) = match orientation {
            DividerOrientation::Horizontal => (width.max(1), height.max(1)),
            DividerOrientation::Vertical => (width.max(1), height.max(1)),
        };

        let canvas = Canvas::new(w, h, RendererBackend::Cpu)?;
        let color = Color::rgb(180, 180, 180);
        canvas.set_draw_callback(move |ctx| {
            ctx.clear(Color::rgb(255, 255, 255))?;
            let paint = Paint::new().color(color).style(PaintStyle::Fill);
            ctx.draw_rect(
                Rect::new(0.0, 0.0, w as f32, h as f32),
                &paint,
            )?;
            Ok(())
        })?;

        Ok(Self {
            canvas,
            orientation,
        })
    }
}

impl Element for Divider {
    fn handle(&self) -> *mut c_void {
        self.canvas.handle()
    }

    unsafe fn invalidate_platform(&self, rect: Option<crate::render::Rect>) {
        use super::traits::Element;
        unsafe {
            <Canvas as Element>::invalidate_platform(&self.canvas, rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider_horizontal_creates() {
        let d = Divider::horizontal(100);
        assert!(d.is_ok());
    }

    #[test]
    fn divider_vertical_creates() {
        let d = Divider::vertical(50);
        assert!(d.is_ok());
    }
}
