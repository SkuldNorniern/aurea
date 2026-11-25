//! Visual separator line (horizontal or vertical).

use super::traits::Element;
use crate::AureaResult;
use crate::render::{Canvas, Color, Paint, PaintStyle, Rect, RendererBackend};
use std::os::raw::c_void;

/// Orientation of the divider line.
#[derive(Debug, Clone, Copy)]
pub enum DividerOrientation {
    /// Left to right.
    Horizontal,
    /// Top to bottom.
    Vertical,
}

/// A horizontal or vertical divider line.
pub struct Divider {
    canvas: Canvas,
}

impl Divider {
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
        let w = width.max(1);
        let h = height.max(1);
        let line = line_rect(orientation, w as f32, h as f32);

        let canvas = Canvas::new(w, h, RendererBackend::Cpu)?;
        let color = Color::rgb(180, 180, 180);
        canvas.set_draw_callback(move |ctx| {
            ctx.clear(Color::rgb(255, 255, 255))?;
            let paint = Paint::new().color(color).style(PaintStyle::Fill);
            ctx.draw_rect(line, &paint)?;
            Ok(())
        })?;

        Ok(Self { canvas })
    }
}

/// A 1px line across the middle of a `w` by `h` box, running along the orientation's length.
fn line_rect(orientation: DividerOrientation, w: f32, h: f32) -> Rect {
    match orientation {
        DividerOrientation::Horizontal => Rect::new(0.0, (h - 1.0) / 2.0, w, 1.0),
        DividerOrientation::Vertical => Rect::new((w - 1.0) / 2.0, 0.0, 1.0, h),
    }
}

impl Element for Divider {
    fn handle(&self) -> *mut c_void {
        self.canvas.handle()
    }

    fn released_to_parent(&self) {
        self.canvas.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, rect: Option<Rect>) {
        use super::traits::Element;
        unsafe {
            <Canvas as Element>::invalidate_platform(&self.canvas, rect);
        }
    }
}

#[cfg(test)]
mod divider_tests {
    use crate::{AureaResult, Window};

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn dividers_can_be_created() -> AureaResult<()> {
        use crate::elements::Divider;

        let _window = Window::new("dividers", 200, 100)?;

        assert!(Divider::horizontal(100).is_ok());
        assert!(Divider::vertical(50).is_ok());
        Ok(())
    }
}
