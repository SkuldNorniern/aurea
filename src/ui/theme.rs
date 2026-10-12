//! How the canvas widgets look: colours, type, shape and motion.

use crate::render::{Color, Font, FontWeight};

/// The colours widgets are drawn in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// Behind everything.
    pub background: Color,
    /// Panels and cards on the background.
    pub surface: Color,
    /// Controls at rest: buttons, tracks, fields.
    pub raised: Color,
    /// A control under the pointer.
    pub hover: Color,
    /// A control held down.
    pub pressed: Color,
    pub border: Color,
    pub text: Color,
    /// Secondary text: values, hints.
    pub dim: Color,
    /// Placeholders and disabled text.
    pub faint: Color,
    /// The main action, a picked option, an on switch.
    pub accent: Color,
    /// Text and marks drawn on the accent.
    pub on_accent: Color,
    /// Something that cannot be undone.
    pub danger: Color,
    pub success: Color,
    /// The ring around what the keyboard is on.
    pub focus: Color,
    /// Selected text behind the glyphs.
    pub selection: Color,
    /// The moving part of a switch or a slider.
    pub knob: Color,
    /// Under what floats: knobs, tooltips.
    pub shadow: Color,
}

/// How fast eased values move, per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub hover: f32,
    pub press: f32,
    pub release: f32,
    /// Switches, segment highlights and knobs sliding.
    pub slide: f32,
    pub focus: f32,
}

/// Everything a widget needs to know to draw itself.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub palette: Palette,
    /// The font family for every widget's text.
    pub font_family: String,
    /// Body text, in logical pixels.
    pub text_size: f32,
    /// Values, hints and tooltips.
    pub small_size: f32,
    /// A control's height when the layout does not say.
    pub control_height: f32,
    /// Corner radius of buttons, fields and cards.
    pub radius: f32,
    /// Space between a control's edge and its content.
    pub padding: f32,
    pub border_width: f32,
    pub focus_width: f32,
    /// How opaque a disabled widget is drawn.
    pub disabled_alpha: f32,
    pub motion: Motion,
    /// Whether a text field's caret blinks.
    pub caret_blink: bool,
}

impl Palette {
    /// Light text on near-black, a soft blue accent.
    pub const fn dark() -> Self {
        Self {
            background: Color::rgb(15, 17, 21),
            surface: Color::rgb(24, 27, 33),
            raised: Color::rgb(34, 38, 46),
            hover: Color::rgb(46, 51, 61),
            pressed: Color::rgb(56, 62, 74),
            border: Color::rgb(52, 57, 68),
            text: Color::rgb(226, 230, 236),
            dim: Color::rgb(134, 141, 153),
            faint: Color::rgb(86, 92, 104),
            accent: Color::rgb(118, 146, 255),
            on_accent: Color::rgb(12, 16, 32),
            danger: Color::rgb(238, 112, 104),
            success: Color::rgb(110, 205, 140),
            focus: Color::rgba(118, 146, 255, 170),
            selection: Color::rgba(118, 146, 255, 90),
            knob: Color::rgb(240, 242, 246),
            shadow: Color::rgba(0, 0, 0, 110),
        }
    }

    /// Dark text on white, a deeper blue accent.
    pub const fn light() -> Self {
        Self {
            background: Color::rgb(246, 247, 249),
            surface: Color::rgb(255, 255, 255),
            raised: Color::rgb(236, 238, 242),
            hover: Color::rgb(225, 228, 234),
            pressed: Color::rgb(212, 216, 224),
            border: Color::rgb(208, 212, 220),
            text: Color::rgb(24, 27, 33),
            dim: Color::rgb(92, 99, 112),
            faint: Color::rgb(150, 156, 168),
            accent: Color::rgb(56, 92, 230),
            on_accent: Color::rgb(255, 255, 255),
            danger: Color::rgb(206, 58, 50),
            success: Color::rgb(38, 150, 82),
            focus: Color::rgba(56, 92, 230, 150),
            selection: Color::rgba(56, 92, 230, 64),
            knob: Color::rgb(255, 255, 255),
            shadow: Color::rgba(16, 24, 40, 60),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// The default sizes and motion around `palette`.
    pub fn new(palette: Palette) -> Self {
        Self {
            palette,
            font_family: String::new(),
            text_size: 13.0,
            small_size: 11.0,
            control_height: 30.0,
            radius: 7.0,
            padding: 10.0,
            border_width: 1.0,
            focus_width: 2.0,
            disabled_alpha: 0.45,
            motion: Motion {
                hover: 18.0,
                press: 40.0,
                release: 8.0,
                slide: 14.0,
                focus: 20.0,
            },
            caret_blink: true,
        }
    }

    pub fn dark() -> Self {
        Self::new(Palette::dark())
    }

    pub fn light() -> Self {
        Self::new(Palette::light())
    }

    /// This theme with `accent` as its accent, the focus ring and the selection following it.
    pub fn with_accent(mut self, accent: Color, on_accent: Color) -> Self {
        self.palette.accent = accent;
        self.palette.on_accent = on_accent;
        self.palette.focus = accent.with_alpha(self.palette.focus.a);
        self.palette.selection = accent.with_alpha(self.palette.selection.a);
        self
    }

    /// The theme's font at `size`.
    pub fn font(&self, size: f32) -> Font {
        Font::new(self.family(), size)
    }

    /// The theme's font at `size`, bold.
    pub fn bold(&self, size: f32) -> Font {
        Font::new(self.family(), size).with_weight(FontWeight::Bold)
    }

    fn family(&self) -> &str {
        if self.font_family.is_empty() {
            DEFAULT_FAMILY
        } else {
            &self.font_family
        }
    }
}

const DEFAULT_FAMILY: &str = "Sans";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_accent_carries_into_focus_and_selection() {
        let accent = Color::rgb(200, 80, 160);
        let theme = Theme::dark().with_accent(accent, Color::rgb(0, 0, 0));
        assert_eq!(theme.palette.focus, accent.with_alpha(170));
        assert_eq!(theme.palette.selection, accent.with_alpha(90));
    }

    #[test]
    fn the_palettes_differ_but_the_shape_is_shared() {
        let (dark, light) = (Theme::dark(), Theme::light());
        assert_ne!(dark.palette, light.palette);
        assert_eq!(dark.radius, light.radius);
        assert_eq!(dark.font(13.0).family, light.font(13.0).family);
    }
}
