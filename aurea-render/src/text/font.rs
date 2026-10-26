//! Fonts and what measuring text in one gives back.

/// Font for text rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// Font family name.
    pub family: String,
    /// Size.
    pub size: f32,
    /// Weight.
    pub weight: FontWeight,
    /// Upright or italic.
    pub style: FontStyle,
    /// OpenType features turned on or off, over the font's defaults.
    pub features: Vec<FontFeature>,
    /// Positions on a variable font's axes; axes left out stay at their defaults, and a font
    /// without the axis ignores it.
    pub variations: Vec<FontVariation>,
    /// Set top to bottom, glyphs upright, as CJK text is written in columns.
    pub vertical: bool,
}

/// An OpenType feature setting, such as `liga` off or `ss01` on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontFeature {
    /// Four-byte OpenType feature tag.
    pub tag: [u8; 4],
    /// 0 turns it off, 1 on; some features pick an alternate by number.
    pub value: u32,
}

/// A position on a variable font's axis, such as `wght` 650.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontVariation {
    /// Four-byte axis tag.
    pub tag: [u8; 4],
    /// In the axis's own units: weight 100 to 900, width in percent.
    pub value: f32,
}

fn tag(name: &str) -> [u8; 4] {
    let mut out = *b"    ";
    for (o, b) in out.iter_mut().zip(name.bytes()) {
        *o = b;
    }
    out
}

impl Font {
    /// An upright, normal-weight font of `family` at `size`.
    pub fn new(family: &str, size: f32) -> Self {
        Self {
            family: family.to_string(),
            size,
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
            features: Vec::new(),
            variations: Vec::new(),
            vertical: false,
        }
    }

    /// With OpenType feature `name` set to `value`, e.g. `("liga", 0)` for no ligatures or
    /// `("tnum", 1)` for figures of equal width.
    pub fn with_feature(mut self, name: &str, value: u32) -> Self {
        let tag = tag(name);
        self.features.retain(|f| f.tag != tag);
        self.features.push(FontFeature { tag, value });
        self
    }

    /// At `value` on variable axis `name`, e.g. `("wght", 650.0)`.
    pub fn with_variation(mut self, name: &str, value: f32) -> Self {
        let tag = tag(name);
        self.variations.retain(|v| v.tag != tag);
        self.variations.push(FontVariation { tag, value });
        self
    }

    /// Set in a column, top to bottom.
    pub fn with_vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    /// Sets the weight.
    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Sets the style.
    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
}

/// Font weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontWeight {
    /// Weight 400.
    Normal,
    /// Weight 700.
    Bold,
    /// Numeric face weight, as used by CSS.
    Number(u16),
}

impl FontWeight {
    /// Numeric weight with the familiar 400/700 aliases kept canonical.
    pub const fn from_number(value: u16) -> Self {
        match value {
            400 => Self::Normal,
            700 => Self::Bold,
            0 => Self::Number(1),
            1001.. => Self::Number(1000),
            n => Self::Number(n),
        }
    }

    /// The numeric CSS weight.
    pub const fn number(self) -> u16 {
        match self {
            Self::Normal => 400,
            Self::Bold => 700,
            Self::Number(0) => 1,
            Self::Number(1001..) => 1000,
            Self::Number(n) => n,
        }
    }
}

/// Font style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    /// Upright.
    Normal,
    /// Slanted.
    Italic,
}

/// Text metrics from text measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    /// Width of the text.
    pub width: f32,
    /// Line height.
    pub height: f32,
    /// Distance above the baseline.
    pub ascent: f32,
    /// Distance below the baseline.
    pub descent: f32,
    /// Advance width.
    pub advance: f32,
}

/// A font's own metrics at its size, in pixels, as terminals and editors size rows and draw
/// underlines by.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FontMetrics {
    /// Distance above the baseline, as text is measured with.
    pub ascent: f32,
    /// Distance below the baseline, positive.
    pub descent: f32,
    /// Extra room the face asks for between lines.
    pub line_gap: f32,
    /// Height of capital letters, when the face says.
    pub cap_height: Option<f32>,
    /// Height of lowercase x, when the face says.
    pub x_height: Option<f32>,
    /// Where an underline's top sits below the baseline.
    pub underline_offset: f32,
    /// How thick an underline is.
    pub underline_thickness: f32,
    /// Whether every glyph has the same advance.
    pub monospace: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_weights_keep_aliases_and_bound_native_values() {
        assert_eq!(FontWeight::from_number(400), FontWeight::Normal);
        assert_eq!(FontWeight::from_number(700), FontWeight::Bold);
        assert_eq!(FontWeight::from_number(350).number(), 350);
        assert_eq!(FontWeight::Number(0).number(), 1);
        assert_eq!(FontWeight::Number(u16::MAX).number(), 1000);
        assert_eq!(
            Font::new("Sans", 16.0)
                .with_weight(FontWeight::from_number(600))
                .weight
                .number(),
            600
        );
    }

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

        let styled = f
            .with_feature("liga", 0)
            .with_feature("ss1", 1)
            .with_feature("liga", 1)
            .with_variation("wght", 650.0);
        assert_eq!(
            styled.features,
            [
                FontFeature {
                    tag: *b"ss1 ",
                    value: 1
                },
                FontFeature {
                    tag: *b"liga",
                    value: 1
                }
            ]
        );
        assert_eq!(styled.variations[0].tag, *b"wght");
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
