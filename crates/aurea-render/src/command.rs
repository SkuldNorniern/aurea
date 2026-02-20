//! Draw commands shared between display list and rasterizer.
//!
//! Boundary between display list (records commands) and raster (executes them).

use super::types::{
    Color, Font, Image, LinearGradient, Paint, Path, Point, RadialGradient, Rect, Transform,
};

#[derive(Debug, Clone)]
pub enum DrawCommand {
    Clear(Color),
    DrawRect(Rect, Paint),
    DrawCircle(Point, f32, Paint),
    #[allow(dead_code)]
    DrawPath(Path, Paint),
    #[allow(dead_code)]
    DrawText(String, Point, Paint),
    #[allow(dead_code)]
    DrawTextWithFont(String, Point, Font, Paint),
    DrawImageRect(Image, Rect),
    DrawImageRegion(Image, Rect, Rect),
    FillLinearGradient(LinearGradient, Rect),
    FillRadialGradient(RadialGradient, Rect),
    PushClip(Path),
    PopClip,
    PushTransform(Transform),
    PopTransform,
    PushOpacity(f32),
    PopOpacity,
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
