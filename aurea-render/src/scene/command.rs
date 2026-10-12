//! Draw commands shared between display list and rasterizer.

use crate::types::{
    Color, Font, GlyphMask, GpuTexture, Image, LinearGradient, Paint, Path, Point, RadialGradient,
    Rect, Sampling,
};

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Clear(Color),
    DrawRect(Rect, Paint),
    /// A rect with corners of the given radius; a stroke is centred on the outline.
    DrawRoundRect(Rect, f32, Paint),
    DrawCircle(Point, f32, Paint),
    DrawPath(Path, Paint),
    DrawText(String, Point, Paint),
    DrawTextWithFont(String, Point, Font, Paint),
    DrawImageRect(Image, Rect, Sampling),
    /// Image, source rect in pixels, destination.
    DrawImageRegion(Image, Rect, Rect, Sampling),
    /// GPU texture, source rect in UV (0 to 1), destination.
    DrawTexture(GpuTexture, Rect, Rect, Sampling),
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
