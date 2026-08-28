//! Visual separator line (horizontal or vertical).
//!
//! Renders a simple line; useful for grouping UI sections.

use super::traits::Element;
use crate::AureaResult;
use crate::render::{Canvas, Color, Paint, PaintStyle, Rect, RendererBackend};
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
    pub fn new(_orientation: DividerOrientation, width: u32, height: u32) -> AureaResult<Self> {
        let w = width.max(1);
        let h = height.max(1);

        let canvas = Canvas::new(w, h, RendererBackend::Cpu)?;
        let color = Color::rgb(180, 180, 180);
        canvas.set_draw_callback(move |ctx| {
            ctx.clear(Color::rgb(255, 255, 255))?;
            let paint = Paint::new().color(color).style(PaintStyle::Fill);
            ctx.draw_rect(Rect::new(0.0, 0.0, w as f32, h as f32), &paint)?;
            Ok(())
        })?;

        Ok(Self { canvas })
    }
}

impl Element for Divider {
    fn handle(&self) -> *mut c_void {
        self.canvas.handle()
    }

    fn released_to_parent(&self) {
        // The canvas underneath owns the native element.
        self.canvas.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, rect: Option<Rect>) {
        use super::traits::Element;
        unsafe {
            <Canvas as Element>::invalidate_platform(&self.canvas, rect);
        }
    }
}
