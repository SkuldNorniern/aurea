//! Fonts and what measuring text in one gives back.

/// Font for text rendering
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
