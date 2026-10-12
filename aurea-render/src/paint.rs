//! Colours, paints, gradients and blend modes.

use crate::numeric::f32_to_u8_clamped;
use crate::types::Point;

/// Color representation in RGBA format.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Create a color from RGB values (alpha defaults to 255).
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Create a color from RGBA values.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Convert to f32 values in range [0.0, 1.0].
    pub fn to_f32(self) -> (f32, f32, f32, f32) {
        (
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
            f32::from(self.a) / 255.0,
        )
    }

    /// This colour with alpha `a`.
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// This colour with its alpha scaled by `factor`, kept to `0..=1`.
    pub fn fade(self, factor: f32) -> Self {
        Self {
            a: lerp_u8(0, self.a, factor),
            ..self
        }
    }

    /// The colour `t` of the way from this one to `other`, `t` kept to `0..=1`, alpha included.
    pub fn mix(self, other: Self, t: f32) -> Self {
        Self {
            r: lerp_u8(self.r, other.r, t),
            g: lerp_u8(self.g, other.g, t),
            b: lerp_u8(self.b, other.b, t),
            a: lerp_u8(self.a, other.a, t),
        }
    }
}

fn lerp_u8(from: u8, to: u8, t: f32) -> u8 {
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    let v = f32::from(from) + (f32::from(to) - f32::from(from)) * t;
    f32_to_u8_clamped(v.round())
}

/// Paint style for drawing operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaintStyle {
    Fill,
    Stroke,
}

/// Paint configuration for drawing.
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

/// Linear gradient.
#[derive(Debug, Clone)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub stops: Vec<GradientStop>,
}

/// Radial gradient.
#[derive(Debug, Clone)]
pub struct RadialGradient {
    pub center: Point,
    pub radius: f32,
    pub stops: Vec<GradientStop>,
}

/// Gradient stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Color,
}

/// Blend mode for compositing.
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
