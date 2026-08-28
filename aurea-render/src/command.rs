//! Draw commands shared between display list and rasterizer.
//!
//! Boundary between display list (records commands) and raster (executes them).
//!
//! Only things that draw. Clip, transform and opacity used to have push/pop
//! commands here, but nothing emitted them and nothing executed them: each item
//! carries its own resolved state, which is what partial repaint needs.

use super::types::{
    Color, Font, GlyphMask, Image, LinearGradient, Paint, Path, Point, RadialGradient, Rect,
};

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Clear(Color),
    DrawRect(Rect, Paint),
    DrawCircle(Point, f32, Paint),
    DrawPath(Path, Paint),
    DrawText(String, Point, Paint),
    DrawTextWithFont(String, Point, Font, Paint),
    DrawImageRect(Image, Rect),
    DrawImageRegion(Image, Rect, Rect),
    /// Subpixel-antialiased text: coverage mask, top-left position, text colour.
    DrawGlyphMask(GlyphMask, Point, Color),
    FillLinearGradient(LinearGradient, Rect),
    FillRadialGradient(RadialGradient, Rect),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_command_clear() {
        let cmd = DrawCommand::Clear(Color::rgb(255, 0, 0));
        assert!(matches!(cmd, DrawCommand::Clear(c) if c.r == 255));
    }

    #[test]
    fn draw_command_rect_bounds() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        let paint = Paint::new();
        let cmd = DrawCommand::DrawRect(rect, paint);
        assert!(matches!(cmd, DrawCommand::DrawRect(r, _) if r.width == 10.0));
    }
}
