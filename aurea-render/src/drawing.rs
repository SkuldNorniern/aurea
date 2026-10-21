//! The drawing contract: what a draw callback can ask of a canvas.

use crate::text::TextLayout;
use crate::types::{
    BlendMode, Color, Font, GpuTexture, Image, LinearGradient, Paint, Path, PathCommand, Point,
    RadialGradient, Rect, Sampling, TextMetrics, Transform,
};
use aurea_foundation::AureaResult;

/// The canvas a draw callback records into.
pub trait DrawingContext {
    /// Get the width of the drawing area.
    fn width(&self) -> u32;

    /// Get the height of the drawing area.
    fn height(&self) -> u32;

    /// Clear the canvas with a color.
    fn clear(&mut self, color: Color) -> AureaResult<()>;

    /// Draw a rectangle.
    fn draw_rect(&mut self, rect: Rect, paint: &Paint) -> AureaResult<()>;

    /// Draw a rectangle with rounded corners, `radius` kept to half the shorter side.
    fn draw_round_rect(&mut self, rect: Rect, radius: f32, paint: &Paint) -> AureaResult<()> {
        self.draw_path(&Path::rounded_rect(rect, radius), paint)
    }

    /// Draw a circle.
    fn draw_circle(&mut self, center: Point, radius: f32, paint: &Paint) -> AureaResult<()>;

    /// A soft shadow: `rect` with corner `radius`, blurred by a Gaussian of sigma `blur`.
    fn draw_shadow(&mut self, rect: Rect, radius: f32, blur: f32, color: Color) -> AureaResult<()> {
        let _ = blur;
        self.draw_round_rect(rect, radius, &Paint::new().color(color))
    }

    /// Draw text at a position.
    fn draw_text(&mut self, text: &str, position: Point, paint: &Paint) -> AureaResult<()>;

    /// Draw a line between two points.
    fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, paint: &Paint) -> AureaResult<()> {
        let mut path = Path::new();
        path.commands.push(PathCommand::MoveTo(Point::new(x1, y1)));
        path.commands.push(PathCommand::LineTo(Point::new(x2, y2)));
        self.draw_path(&path, paint)
    }

    /// Draw a path.
    fn draw_path(&mut self, path: &Path, paint: &Paint) -> AureaResult<()>;

    /// Draw an image at a position.
    fn draw_image(&mut self, image: &Image, position: Point) -> AureaResult<()>;

    /// Draw an image with scaling.
    fn draw_image_rect(&mut self, image: &Image, dest: Rect) -> AureaResult<()>;

    /// Draw part of an image (source rect) to a destination rect.
    fn draw_image_region(&mut self, image: &Image, src: Rect, dest: Rect) -> AureaResult<()>;

    /// How the image and texture draws that follow are filtered.
    fn set_sampling(&mut self, sampling: Sampling) -> AureaResult<()>;

    /// Whether the rects and straight lines that follow land on whole device pixels.
    fn set_pixel_snap(&mut self, snap: bool) -> AureaResult<()> {
        let _ = snap;
        Ok(())
    }

    /// Draw part of an app-made GPU texture, `src` in UV (0 to 1), in painter order with everything
    /// else.
    fn draw_texture(&mut self, texture: GpuTexture, src: Rect, dest: Rect) -> AureaResult<()>;

    /// Draw text with font configuration.
    fn draw_text_with_font(
        &mut self,
        text: &str,
        position: Point,
        font: &Font,
        paint: &Paint,
    ) -> AureaResult<()>;

    /// Draw one line of a paragraph read right to left when `rtl`, so a wrapped line starting in
    /// the other direction keeps its paragraph's order.
    fn draw_text_directed(
        &mut self,
        text: &str,
        position: Point,
        font: &Font,
        paint: &Paint,
        rtl: Option<bool>,
    ) -> AureaResult<()> {
        let _ = rtl;
        self.draw_text_with_font(text, position, font, paint)
    }

    /// Draw `layout`, laid out in `font`, with its top-left at `origin`.
    fn draw_text_layout(
        &mut self,
        layout: &TextLayout,
        origin: Point,
        font: &Font,
        paint: &Paint,
    ) -> AureaResult<()> {
        for line in &layout.lines {
            let at = if layout.vertical {
                let inset = (line.height - font.size) / 2.0;
                Point::new(origin.x + line.x + inset, origin.y + line.top)
            } else {
                Point::new(origin.x + line.x, origin.y + line.baseline)
            };
            let text = &layout.text[line.range.clone()];
            self.draw_text_directed(text, at, font, paint, Some(line.rtl))?;
        }
        Ok(())
    }

    /// Measure text dimensions.
    fn measure_text(&mut self, text: &str, font: &Font) -> AureaResult<TextMetrics>;

    /// Save the current transformation matrix.
    fn save(&mut self) -> AureaResult<()>;

    /// Restore the previously saved transformation matrix.
    fn restore(&mut self) -> AureaResult<()>;

    /// Apply a transformation matrix.
    fn transform(&mut self, transform: Transform) -> AureaResult<()>;

    /// Translate the coordinate system.
    fn translate(&mut self, x: f32, y: f32) -> AureaResult<()> {
        self.transform(Transform::translate(x, y))
    }

    /// Scale the coordinate system.
    fn scale(&mut self, sx: f32, sy: f32) -> AureaResult<()> {
        self.transform(Transform::scale(sx, sy))
    }

    /// Rotate the coordinate system (angle in radians).
    fn rotate(&mut self, angle: f32) -> AureaResult<()> {
        self.transform(Transform::rotate(angle))
    }

    /// Set a clipping rectangle.
    fn clip_rect(&mut self, rect: Rect) -> AureaResult<()>;

    /// Set a clipping path.
    fn clip_path(&mut self, path: &Path) -> AureaResult<()>;

    /// Set the global alpha (opacity).
    fn set_alpha(&mut self, alpha: f32) -> AureaResult<()>;

    /// Set the blend mode.
    fn set_blend_mode(&mut self, mode: BlendMode) -> AureaResult<()>;

    /// Fill with a linear gradient.
    fn fill_linear_gradient(&mut self, gradient: &LinearGradient, rect: Rect) -> AureaResult<()>;

    /// Fill with a radial gradient.
    fn fill_radial_gradient(&mut self, gradient: &RadialGradient, rect: Rect) -> AureaResult<()>;

    /// Check if a point is inside a path (hit testing).
    fn hit_test_path(&mut self, path: &Path, point: Point) -> AureaResult<bool>;

    /// Check if a point is inside a rectangle (hit testing).
    fn hit_test_rect(&mut self, rect: Rect, point: Point) -> bool {
        point.x >= rect.x
            && point.x <= rect.x + rect.width
            && point.y >= rect.y
            && point.y <= rect.y + rect.height
    }
}
