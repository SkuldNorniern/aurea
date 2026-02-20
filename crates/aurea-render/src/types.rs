//! Common types for rendering operations

/// Color representation in RGBA format
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Create a color from RGB values (alpha defaults to 255)
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Create a color from RGBA values
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Convert to f32 values in range [0.0, 1.0]
    pub fn to_f32(self) -> (f32, f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        )
    }
}

/// 2D point
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Rectangle
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_points(top_left: Point, bottom_right: Point) -> Self {
        Self {
            x: top_left.x,
            y: top_left.y,
            width: bottom_right.x - top_left.x,
            height: bottom_right.y - top_left.y,
        }
    }
}

/// Paint style for drawing operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaintStyle {
    Fill,
    Stroke,
}

/// Paint configuration for drawing
#[derive(Debug, Clone)]
pub struct Paint {
    pub color: Color,
    pub style: PaintStyle,
    pub stroke_width: f32,
}

impl Paint {
    pub fn new() -> Self {
        Self {
            color: Color::rgb(0, 0, 0),
            style: PaintStyle::Fill,
            stroke_width: 1.0,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn style(mut self, style: PaintStyle) -> Self {
        self.style = style;
        self
    }

    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }
}

impl Default for Paint {
    fn default() -> Self {
        Self::new()
    }
}

/// Aurea's native rendering backend.
///
/// The framework provides its own rendering path (no external Skia/Vello).
/// Cpu uses native CPU rasterization. Gpu delegates to CPU for now; wgpu pipeline planned.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RendererBackend {
    /// CPU rasterizer (tile-based, display list, partial redraw)
    #[default]
    Cpu,
    /// GPU-accelerated backend (planned; returns error until implemented)
    Gpu,
}

/// Path command for drawing paths
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    /// Move to a point
    MoveTo(Point),
    /// Line to a point
    LineTo(Point),
    /// Quadratic curve to a point
    QuadTo(Point, Point),
    /// Cubic curve to a point
    CubicTo(Point, Point, Point),
    /// Close the path
    Close,
}

/// Path for drawing shapes
#[derive(Debug, Clone)]
pub struct Path {
    pub commands: Vec<PathCommand>,
}

impl Path {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

/// 2D transformation matrix (3x3 homogeneous coordinates)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub m11: f32,
    pub m12: f32,
    pub m13: f32,
    pub m21: f32,
    pub m22: f32,
    pub m23: f32,
    pub m31: f32,
    pub m32: f32,
    pub m33: f32,
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            m11: 1.0,
            m12: 0.0,
            m13: 0.0,
            m21: 0.0,
            m22: 1.0,
            m23: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
        }
    }

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            m11: 1.0,
            m12: 0.0,
            m13: 0.0,
            m21: 0.0,
            m22: 1.0,
            m23: 0.0,
            m31: x,
            m32: y,
            m33: 1.0,
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            m11: sx,
            m12: 0.0,
            m13: 0.0,
            m21: 0.0,
            m22: sy,
            m23: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
        }
    }

    pub fn rotate(angle: f32) -> Self {
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        Self {
            m11: cos_a,
            m12: sin_a,
            m13: 0.0,
            m21: -sin_a,
            m22: cos_a,
            m23: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
        }
    }

    pub fn multiply(self, other: Self) -> Self {
        Self {
            m11: self.m11 * other.m11 + self.m12 * other.m21 + self.m13 * other.m31,
            m12: self.m11 * other.m12 + self.m12 * other.m22 + self.m13 * other.m32,
            m13: self.m11 * other.m13 + self.m12 * other.m23 + self.m13 * other.m33,
            m21: self.m21 * other.m11 + self.m22 * other.m21 + self.m23 * other.m31,
            m22: self.m21 * other.m12 + self.m22 * other.m22 + self.m23 * other.m32,
            m23: self.m21 * other.m13 + self.m22 * other.m23 + self.m23 * other.m33,
            m31: self.m31 * other.m11 + self.m32 * other.m21 + self.m33 * other.m31,
            m32: self.m31 * other.m12 + self.m32 * other.m22 + self.m33 * other.m32,
            m33: self.m31 * other.m13 + self.m32 * other.m23 + self.m33 * other.m33,
        }
    }

    pub fn inverse(self) -> Self {
        let det = self.m11 * (self.m22 * self.m33 - self.m23 * self.m32)
            - self.m12 * (self.m21 * self.m33 - self.m23 * self.m31)
            + self.m13 * (self.m21 * self.m32 - self.m22 * self.m31);

        if det.abs() < 1e-10 {
            return Self::identity();
        }

        let inv_det = 1.0 / det;
        Self {
            m11: (self.m22 * self.m33 - self.m23 * self.m32) * inv_det,
            m12: (self.m13 * self.m32 - self.m12 * self.m33) * inv_det,
            m13: (self.m12 * self.m23 - self.m13 * self.m22) * inv_det,
            m21: (self.m23 * self.m31 - self.m21 * self.m33) * inv_det,
            m22: (self.m11 * self.m33 - self.m13 * self.m31) * inv_det,
            m23: (self.m13 * self.m21 - self.m11 * self.m23) * inv_det,
            m31: (self.m21 * self.m32 - self.m22 * self.m31) * inv_det,
            m32: (self.m12 * self.m31 - self.m11 * self.m32) * inv_det,
            m33: (self.m11 * self.m22 - self.m12 * self.m21) * inv_det,
        }
    }

    pub fn map_point(self, point: Point) -> Point {
        Point {
            x: self.m11 * point.x + self.m12 * point.y + self.m13,
            y: self.m21 * point.x + self.m22 * point.y + self.m23,
        }
    }
}

/// Font for text rendering
#[derive(Debug, Clone)]
pub struct Font {
    pub family: String,
    pub size: f32,
    pub weight: FontWeight,
    pub style: FontStyle,
}

impl Font {
    pub fn new(family: &str, size: f32) -> Self {
        Self {
            family: family.to_string(),
            size,
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
        }
    }

    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
}

/// Font weight
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontWeight {
    Normal,
    Bold,
}

/// Font style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
}

/// Text metrics from text measurement
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
    pub advance: f32,
}

/// Linear gradient
#[derive(Debug, Clone)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub stops: Vec<GradientStop>,
}

/// Radial gradient
#[derive(Debug, Clone)]
pub struct RadialGradient {
    pub center: Point,
    pub radius: f32,
    pub stops: Vec<GradientStop>,
}

/// Gradient stop
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Color,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_new_and_builders() {
        let f = Font::new("Sans", 16.0);
        assert_eq!(f.family, "Sans");
        assert!((f.size - 16.0).abs() < 1e-5);
        assert_eq!(f.weight, FontWeight::Normal);
        assert_eq!(f.style, FontStyle::Normal);

        let bold = f.clone().with_weight(FontWeight::Bold);
        assert_eq!(bold.weight, FontWeight::Bold);
        let italic = f.clone().with_style(FontStyle::Italic);
        assert_eq!(italic.style, FontStyle::Italic);
    }

    #[test]
    fn text_metrics_fields() {
        let m = TextMetrics {
            width: 100.0,
            height: 14.0,
            ascent: 11.0,
            descent: 3.0,
            advance: 100.0,
        };
        assert!((m.width - 100.0).abs() < 1e-5);
        assert!((m.ascent + m.descent - m.height).abs() < 1e-5);
    }
}

/// Blend mode for compositing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
}

/// Image for rendering
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            data,
        }
    }
}

/// Interactive element ID for hit testing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractiveId(pub u64);
