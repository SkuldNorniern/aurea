//! Colours, paints, gradients and blend modes.

use std::fmt;

use crate::numeric::f32_to_u8_clamped;
use crate::types::Point;

/// Color representation in RGBA format.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
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

    /// Hue, saturation and value, for a picker to move through.
    pub fn to_hsv(self) -> Hsv {
        let (r, g, b, a) = self.to_f32();
        let max = r.max(g).max(b);
        let span = max - r.min(g).min(b);
        let h = if span <= 0.0 {
            0.0
        } else if max == r {
            60.0 * ((g - b) / span).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / span + 2.0)
        } else {
            60.0 * ((r - g) / span + 4.0)
        };
        let s = if max <= 0.0 { 0.0 } else { span / max };
        Hsv { h, s, v: max, a }
    }

    /// `#rgb`, `#rrggbb` or `#rrggbbaa`, the `#` optional.
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.trim().strip_prefix('#').unwrap_or(text.trim());
        if !digits.is_ascii() {
            return None;
        }
        let byte = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
        let short = |at: usize| {
            let nibble = u8::from_str_radix(digits.get(at..=at)?, 16).ok()?;
            Some(nibble * 17)
        };
        match digits.len() {
            3 => Some(Self::rgb(short(0)?, short(1)?, short(2)?)),
            6 => Some(Self::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Self::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// Writes as `#rrggbb`, or `#rrggbbaa` when not opaque, without allocating until the caller
    /// asks for a `String`.
    pub fn hex(self) -> Hex {
        Hex(self)
    }
}

/// A colour by hue (degrees, `0..360`), saturation and value (`0..=1`) and alpha (`0..=1`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    /// Hue in degrees, `0..360`.
    pub h: f32,
    /// Saturation, `0..=1`.
    pub s: f32,
    /// Value, `0..=1`.
    pub v: f32,
    /// Alpha, `0..=1`.
    pub a: f32,
}

impl Hsv {
    /// Creates a colour from hue, saturation, value and alpha.
    pub const fn new(h: f32, s: f32, v: f32, a: f32) -> Self {
        Self { h, s, v, a }
    }

    /// The colour, rounded to 8 bits a channel.
    pub fn to_color(self) -> Color {
        let h = self.h.rem_euclid(360.0) / 60.0;
        let s = self.s.clamp(0.0, 1.0);
        let v = self.v.clamp(0.0, 1.0);
        let c = v * s;
        let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
        let (r, g, b) = match h {
            h if h < 1.0 => (c, x, 0.0),
            h if h < 2.0 => (x, c, 0.0),
            h if h < 3.0 => (0.0, c, x),
            h if h < 4.0 => (0.0, x, c),
            h if h < 5.0 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = v - c;
        let unit = |t: f32| f32_to_u8_clamped(((t + m) * 255.0).round());
        Color::rgba(
            unit(r),
            unit(g),
            unit(b),
            f32_to_u8_clamped((self.a.clamp(0.0, 1.0) * 255.0).round()),
        )
    }
}

/// A colour written as hex, from [`Color::hex`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hex(Color);

impl fmt::Display for Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Color { r, g, b, a } = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}")?;
        if a < 255 {
            write!(f, "{a:02x}")?;
        }
        Ok(())
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
    /// Fills the shape.
    Fill,
    /// Strokes the outline.
    Stroke,
}

/// How an open stroke ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineCap {
    /// Flat, at the end point.
    #[default]
    Butt,
    /// A half disc past the end point.
    Round,
    /// Flat, half the width past the end point.
    Square,
}

/// How a stroke turns a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineJoin {
    /// A sharp point, cut to a bevel past [`Paint::miter_limit`].
    #[default]
    Miter,
    /// Rounded.
    Round,
    /// Cut straight across.
    Bevel,
}

/// A dash pattern: on and off lengths in turn, as SVG's `stroke-dasharray`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dash {
    lengths: [f32; Dash::MAX],
    count: u8,
    /// How far into the pattern the stroke starts.
    pub offset: f32,
}

impl Dash {
    /// The most lengths a pattern holds; more are dropped.
    pub const MAX: usize = 8;

    /// `lengths` from `offset` into them.
    pub fn new(lengths: &[f32], offset: f32) -> Option<Self> {
        let mut dash = Self {
            lengths: [0.0; Self::MAX],
            count: 0,
            offset,
        };
        let take = lengths.len().min(Self::MAX);
        let doubled = if take % 2 == 1 { take * 2 } else { take }.min(Self::MAX);
        for i in 0..doubled {
            dash.lengths[i] = lengths[i % take].max(0.0);
        }
        dash.count = u8::try_from(doubled).unwrap_or(0);
        (dash.lengths().iter().sum::<f32>() > 0.0).then_some(dash)
    }

    /// The on and off lengths, starting with on.
    pub fn lengths(&self) -> &[f32] {
        &self.lengths[..usize::from(self.count)]
    }

    /// The same pattern `factor` times larger, for a scaled drawing.
    pub fn scaled(mut self, factor: f32) -> Self {
        for length in &mut self.lengths {
            *length *= factor;
        }
        self.offset *= factor;
        self
    }
}

/// Paint configuration for drawing.
#[derive(Debug, Clone)]
pub struct Paint {
    /// Fill or stroke colour.
    pub color: Color,
    /// Fill or stroke.
    pub style: PaintStyle,
    /// Stroke width.
    pub stroke_width: f32,
    /// End style of open strokes.
    pub line_cap: LineCap,
    /// Corner style.
    pub line_join: LineJoin,
    /// How long a miter may be, in stroke widths, before it is beveled.
    pub miter_limit: f32,
    /// `None` strokes solid.
    pub dash: Option<Dash>,
}

impl Paint {
    /// A black fill, width 1, butt caps and miter joins.
    pub fn new() -> Self {
        Self {
            color: Color::rgb(0, 0, 0),
            style: PaintStyle::Fill,
            stroke_width: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: 4.0,
            dash: None,
        }
    }

    /// Sets the end style of open strokes.
    pub fn line_cap(mut self, cap: LineCap) -> Self {
        self.line_cap = cap;
        self
    }

    /// Sets the corner style.
    pub fn line_join(mut self, join: LineJoin) -> Self {
        self.line_join = join;
        self
    }

    /// Sets the miter limit, at least 1.
    pub fn miter_limit(mut self, limit: f32) -> Self {
        self.miter_limit = limit.max(1.0);
        self
    }

    /// Dashes the stroke: `lengths` on and off in turn, from `offset` into them.
    pub fn dash(mut self, lengths: &[f32], offset: f32) -> Self {
        self.dash = Dash::new(lengths, offset);
        self
    }

    /// Whether only the general path stroker draws this stroke as asked, not the fast rect, circle
    /// and line paths.
    pub fn needs_path_stroke(&self) -> bool {
        self.style == PaintStyle::Stroke
            && (self.dash.is_some()
                || self.line_cap != LineCap::Butt
                || self.line_join != LineJoin::Miter)
    }

    /// Sets the colour.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets fill or stroke.
    pub fn style(mut self, style: PaintStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the stroke width.
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
    /// Start point.
    pub start: Point,
    /// End point.
    pub end: Point,
    /// Colour stops, by offset.
    pub stops: Vec<GradientStop>,
}

impl LinearGradient {
    /// The axis moved half a pixel in at each end, so the pixels at the ends take the end colours.
    pub(crate) fn pixel_axis(&self) -> (Point, Point) {
        let (dx, dy) = (self.end.x - self.start.x, self.end.y - self.start.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len.is_nan() || len <= 1.0 {
            return (self.start, self.end);
        }
        let (ix, iy) = (dx / len * 0.5, dy / len * 0.5);
        (
            Point::new(self.start.x + ix, self.start.y + iy),
            Point::new(self.end.x - ix, self.end.y - iy),
        )
    }
}

/// Radial gradient.
#[derive(Debug, Clone)]
pub struct RadialGradient {
    /// Centre point.
    pub center: Point,
    /// Radius.
    pub radius: f32,
    /// Colour stops, by offset.
    pub stops: Vec<GradientStop>,
}

/// Gradient stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    /// Position along the gradient, `0..=1`.
    pub offset: f32,
    /// Colour at this offset.
    pub color: Color,
}

/// Blend mode for compositing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// Source over destination.
    Normal,
    /// Multiplies the channels.
    Multiply,
    /// Inverts, multiplies and inverts again.
    Screen,
    /// Multiplies or screens, by the destination.
    Overlay,
    /// Keeps the darker channel.
    Darken,
    /// Keeps the lighter channel.
    Lighten,
    /// Brightens the destination by the source.
    ColorDodge,
    /// Darkens the destination by the source.
    ColorBurn,
    /// Overlay with the roles of source and destination swapped.
    HardLight,
    /// A gentler hard light.
    SoftLight,
    /// Absolute difference of the channels.
    Difference,
    /// Difference with lower contrast.
    Exclusion,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_every_primary_and_grey() {
        for color in [
            Color::rgb(255, 0, 0),
            Color::rgb(0, 255, 0),
            Color::rgb(0, 0, 255),
            Color::rgb(255, 255, 0),
            Color::rgb(12, 200, 99),
            Color::rgb(128, 128, 128),
            Color::rgba(0, 0, 0, 40),
        ] {
            assert_eq!(color.to_hsv().to_color(), color);
        }
    }

    #[test]
    fn hsv_reads_the_usual_way() {
        let hsv = Color::rgb(0, 255, 255).to_hsv();
        assert!((hsv.h - 180.0).abs() < 1e-3 && hsv.s == 1.0 && hsv.v == 1.0);
        assert_eq!(
            Hsv::new(360.0, 1.0, 1.0, 1.0).to_color(),
            Color::rgb(255, 0, 0)
        );
        assert_eq!(
            Hsv::new(-120.0, 1.0, 1.0, 1.0).to_color(),
            Color::rgb(0, 0, 255)
        );
    }

    #[test]
    fn hex_reads_and_writes() {
        assert_eq!(Color::from_hex("#ff8000"), Some(Color::rgb(255, 128, 0)));
        assert_eq!(Color::from_hex("f80"), Some(Color::rgb(255, 136, 0)));
        assert_eq!(
            Color::from_hex(" #11223344 "),
            Some(Color::rgba(17, 34, 51, 68))
        );
        assert_eq!(Color::from_hex("#12345"), None);
        assert_eq!(Color::from_hex("#gg0000"), None);
        assert_eq!(Color::from_hex("#\u{e9}e9e9"), None, "not ascii");
        assert_eq!(Color::rgb(255, 128, 0).hex().to_string(), "#ff8000");
        assert_eq!(Color::rgba(1, 2, 3, 4).hex().to_string(), "#01020304");
    }
}
