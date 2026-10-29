//! Draw commands shared between display list and rasterizer.

use crate::types::{
    Color, GlyphMask, GpuTexture, Image, LinearGradient, Paint, Path, Point, RadialGradient, Rect,
    Sampling,
};

/// A single drawing operation.
#[derive(Debug, Clone)]
pub enum DrawCommand {
    /// Fills the surface with a color.
    Clear(Color),
    /// Rect filled or stroked with a paint.
    DrawRect(Rect, Paint),
    /// A rect with corners of the given radius; a stroke is centred on the outline.
    DrawRoundRect(Rect, f32, Paint),
    /// Circle at a center, with a radius and paint.
    DrawCircle(Point, f32, Paint),
    /// A rounded rect blurred by a Gaussian of the given sigma: rect, radius, sigma, colour.
    DrawShadow(Rect, f32, f32, Color),
    /// Path filled or stroked with a paint.
    DrawPath(Path, Paint),
    /// Image drawn into a destination rect.
    DrawImageRect(Image, Rect, Sampling),
    /// Image, source rect in pixels, destination.
    DrawImageRegion(Image, Rect, Rect, Sampling),
    /// GPU texture, source rect in UV (0 to 1), destination.
    DrawTexture(GpuTexture, Rect, Rect, Sampling),
    /// Image, source pixels, physical origin/right/bottom corners, sampling.
    DrawImageQuad(Image, Rect, [Point; 3], Sampling),
    /// GPU texture, source UV, physical origin/right/bottom corners, sampling.
    DrawTextureQuad(GpuTexture, Rect, [Point; 3], Sampling),
    /// Subpixel-antialiased text: coverage mask, top-left position, text colour.
    DrawGlyphMask(GlyphMask, Point, Color),
    /// Linear gradient filling a rect.
    FillLinearGradient(LinearGradient, Rect),
    /// Radial gradient filling a rect.
    FillRadialGradient(RadialGradient, Rect),
}

// Keeps the largest variant (DrawPath: Path + Paint) from growing the enum.
const _: () = assert!(size_of::<DrawCommand>() <= 96);

pub(crate) fn quad_bounds(q: &[Point; 3]) -> Rect {
    let fourth = Point::new(q[1].x + q[2].x - q[0].x, q[1].y + q[2].y - q[0].y);
    let xs = [q[0].x, q[1].x, q[2].x, fourth.x];
    let ys = [q[0].y, q[1].y, q[2].y, fourth.y];
    let x = xs.into_iter().fold(f32::INFINITY, f32::min);
    let y = ys.into_iter().fold(f32::INFINITY, f32::min);
    Rect::new(
        x,
        y,
        xs.into_iter().fold(f32::NEG_INFINITY, f32::max) - x,
        ys.into_iter().fold(f32::NEG_INFINITY, f32::max) - y,
    )
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
