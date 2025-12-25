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
        Self { x, y, width, height }
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaintStyle {
    Fill,
    Stroke,
}

/// Paint configuration for drawing
#[derive(Debug, Clone)]
pub struct Paint {
    pub fill: PaintFill,
    pub style: PaintStyle,
    pub stroke_width: f32,
    pub alpha: f32,
    pub blend_mode: BlendMode,
}

impl Paint {
    pub fn new() -> Self {
        Self {
            fill: PaintFill::Color(Color::rgb(0, 0, 0)),
            style: PaintStyle::Fill,
            stroke_width: 1.0,
            alpha: 1.0,
            blend_mode: BlendMode::Normal,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.fill = PaintFill::Color(color);
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

    pub fn get_color(&self) -> Color {
        match &self.fill {
            PaintFill::Color(c) => *c,
            _ => Color::rgb(0, 0, 0),
        }
    }
}

impl Default for Paint {
    fn default() -> Self {
        Self::new()
    }
}

/// Rendering backend selection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RendererBackend {
    /// Skia rendering backend
    Skia,
    /// Vello rendering backend
    Vello,
}

/// Path command for building complex shapes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point), // Control point, end point
    CurveTo(Point, Point, Point), // Control point 1, control point 2, end point
    Close,
}

/// Path representing a complex shape
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub commands: Vec<PathCommand>,
}

impl Path {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn move_to(mut self, point: Point) -> Self {
        self.commands.push(PathCommand::MoveTo(point));
        self
    }

    pub fn line_to(mut self, point: Point) -> Self {
        self.commands.push(PathCommand::LineTo(point));
        self
    }

    pub fn quad_to(mut self, control: Point, end: Point) -> Self {
        self.commands.push(PathCommand::QuadTo(control, end));
        self
    }

    pub fn curve_to(mut self, control1: Point, control2: Point, end: Point) -> Self {
        self.commands.push(PathCommand::CurveTo(control1, control2, end));
        self
    }

    pub fn close(mut self) -> Self {
        self.commands.push(PathCommand::Close);
        self
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

/// 2D transformation matrix
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub m11: f32, pub m12: f32, pub m13: f32,
    pub m21: f32, pub m22: f32, pub m23: f32,
    pub m31: f32, pub m32: f32, pub m33: f32,
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            m11: 1.0, m12: 0.0, m13: 0.0,
            m21: 0.0, m22: 1.0, m23: 0.0,
            m31: 0.0, m32: 0.0, m33: 1.0,
        }
    }

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            m11: 1.0, m12: 0.0, m13: 0.0,
            m21: 0.0, m22: 1.0, m23: 0.0,
            m31: x,   m32: y,   m33: 1.0,
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            m11: sx,  m12: 0.0, m13: 0.0,
            m21: 0.0, m22: sy,  m23: 0.0,
            m31: 0.0, m32: 0.0, m33: 1.0,
        }
    }

    pub fn rotate(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            m11: cos, m12: sin, m13: 0.0,
            m21: -sin, m22: cos, m23: 0.0,
            m31: 0.0, m32: 0.0, m33: 1.0,
        }
    }

    pub fn multiply(self, other: Transform) -> Self {
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

    pub fn transform_point(self, point: Point) -> Point {
        Point {
            x: self.m11 * point.x + self.m21 * point.y + self.m31,
            y: self.m12 * point.x + self.m22 * point.y + self.m32,
        }
    }
}

/// Font weight
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    Normal,
    Bold,
}

/// Font style
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
}

/// Font configuration
#[derive(Debug, Clone, PartialEq)]
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

    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    pub fn style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
}

/// Text measurement information
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
    pub advance: f32,
}

/// Color stop for gradients
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorStop {
    pub position: f32, // 0.0 to 1.0
    pub color: Color,
}

/// Linear gradient
#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub stops: Vec<ColorStop>,
}

impl LinearGradient {
    pub fn new(start: Point, end: Point) -> Self {
        Self {
            start,
            end,
            stops: Vec::new(),
        }
    }

    pub fn add_stop(mut self, position: f32, color: Color) -> Self {
        self.stops.push(ColorStop { position, color });
        self.stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());
        self
    }
}

/// Radial gradient
#[derive(Debug, Clone, PartialEq)]
pub struct RadialGradient {
    pub center: Point,
    pub radius: f32,
    pub stops: Vec<ColorStop>,
}

impl RadialGradient {
    pub fn new(center: Point, radius: f32) -> Self {
        Self {
            center,
            radius,
            stops: Vec::new(),
        }
    }

    pub fn add_stop(mut self, position: f32, color: Color) -> Self {
        self.stops.push(ColorStop { position, color });
        self.stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());
        self
    }
}

/// Blend mode for compositing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Image representation
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA data
}

impl Image {
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self { width, height, data }
    }
}

/// Paint fill type
#[derive(Debug, Clone, PartialEq)]
pub enum PaintFill {
    Color(Color),
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
}


// Update Paint to support PaintFill (but keep backward compatibility)
// We'll add a fill field but make it optional for now

