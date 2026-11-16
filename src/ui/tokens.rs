//! Design tokens: the values an app sets once so its drawing follows one design system.

use super::frame::Eased;
use super::paint::Shadow;
use crate::render::Color;

/// A corner radius from the scale; [`Tokens::shape`] gives its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// The smallest radius.
    Small,
    /// The middle radius.
    Medium,
    /// The largest rounded radius.
    Large,
    /// Half the shorter side: a capsule.
    Full,
}

/// A text size from the type scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    /// The biggest text, for titles on a screen.
    Display,
    /// Section headings.
    Headline,
    /// Smaller headings and dialog titles.
    Title,
    /// Most text.
    Body,
    /// Captions and small labels.
    Label,
}

const LEVELS: usize = 6;

/// A shadow of `alpha` black, `dy` down and `blur` wide.
const fn level(alpha: u8, dy: f32, blur: f32) -> Shadow {
    Shadow::new(Color::rgba(0, 0, 0, alpha))
        .offset(0.0, dy)
        .blur(blur)
}

/// The look an app sets once with [`Ui::set_tokens`](crate::ui::Ui::set_tokens): state layers,
/// corner radii, elevation shadows and type sizes. The defaults are Material 3's.
///
/// ```rust,no_run
/// # use aurea::ui::{Component, Panel, Shape, Ui};
/// # use aurea::render::Color;
/// # fn f(ui: &mut Ui<'_, u8>) -> aurea::AureaResult<()> {
/// let tokens = ui.tokens();
/// Panel::new()
///     .fill(Color::rgb(40, 44, 52))
///     .shape(&tokens, Shape::Medium)
///     .elevation(&tokens, 2)
///     .paint(ui, aurea::Rect::new(20.0, 20.0, 200.0, 120.0))
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    hover: f32,
    focus: f32,
    pressed: f32,
    dragged: f32,
    small: f32,
    medium: f32,
    large: f32,
    elevation: [Shadow; LEVELS],
    text: [f32; 5],
}

impl Default for Tokens {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokens {
    /// The Material 3 defaults.
    pub const fn new() -> Self {
        Self {
            hover: 0.08,
            focus: 0.12,
            pressed: 0.12,
            dragged: 0.16,
            small: 4.0,
            medium: 12.0,
            large: 28.0,
            elevation: [
                level(0, 0.0, 0.0),
                level(40, 1.0, 3.0),
                level(48, 2.0, 6.0),
                level(56, 4.0, 8.0),
                level(64, 6.0, 12.0),
                level(70, 8.0, 16.0),
            ],
            text: [36.0, 28.0, 22.0, 14.0, 12.0],
        }
    }

    /// Opacities of the hover, focus and pressed state layers.
    #[must_use]
    pub const fn with_state_layers(mut self, hover: f32, focus: f32, pressed: f32) -> Self {
        self.hover = hover;
        self.focus = focus;
        self.pressed = pressed;
        self
    }

    /// Opacity of the dragged state layer.
    #[must_use]
    pub const fn with_dragged(mut self, dragged: f32) -> Self {
        self.dragged = dragged;
        self
    }

    /// Radii of the small, medium and large shapes.
    #[must_use]
    pub const fn with_shapes(mut self, small: f32, medium: f32, large: f32) -> Self {
        self.small = small;
        self.medium = medium;
        self.large = large;
        self
    }

    /// The shadow for `level`; levels above 5 are ignored.
    #[must_use]
    pub const fn with_elevation(mut self, level: usize, shadow: Shadow) -> Self {
        if level < LEVELS {
            self.elevation[level] = shadow;
        }
        self
    }

    /// The size of `role`'s text.
    #[must_use]
    pub const fn with_text_size(mut self, role: TextRole, size: f32) -> Self {
        self.text[role as usize] = size;
        self
    }

    /// Opacity of the hover state layer.
    pub const fn hover(&self) -> f32 {
        self.hover
    }

    /// Opacity of the focus state layer.
    pub const fn focus(&self) -> f32 {
        self.focus
    }

    /// Opacity of the pressed state layer.
    pub const fn pressed(&self) -> f32 {
        self.pressed
    }

    /// Opacity of the dragged state layer.
    pub const fn dragged(&self) -> f32 {
        self.dragged
    }

    /// The radius of `shape`. [`Shape::Full`] is infinity: drawing calls clamp it to a capsule.
    pub const fn shape(&self, shape: Shape) -> f32 {
        match shape {
            Shape::Small => self.small,
            Shape::Medium => self.medium,
            Shape::Large => self.large,
            Shape::Full => f32::INFINITY,
        }
    }

    /// The shadow for `level`, clamped to 5.
    pub const fn elevation(&self, level: usize) -> Shadow {
        self.elevation[if level < LEVELS { level } else { LEVELS - 1 }]
    }

    /// The size of `role`'s text.
    pub const fn text_size(&self, role: TextRole) -> f32 {
        self.text[role as usize]
    }

    /// The one state layer to draw for `state`: pressed over focus over hover, each scaled by how
    /// far it has eased in.
    pub const fn state_layer(&self, state: Eased) -> f32 {
        if state.press > 0.0 {
            self.pressed * state.press
        } else if state.focus > 0.0 {
            self.focus * state.focus
        } else {
            self.hover * state.hover
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    fn eased(hover: f32, press: f32, focus: f32) -> Eased {
        Eased {
            hover,
            press,
            focus,
            enabled: 1.0,
        }
    }

    #[test]
    fn defaults_are_material_three() {
        let t = Tokens::default();
        assert_eq!(
            [t.hover(), t.focus(), t.pressed(), t.dragged()],
            [0.08, 0.12, 0.12, 0.16]
        );
        assert_eq!(
            [
                t.shape(Shape::Small),
                t.shape(Shape::Medium),
                t.shape(Shape::Large)
            ],
            [4.0, 12.0, 28.0]
        );
        assert!(t.shape(Shape::Full).is_infinite());
        assert_eq!(t.elevation(0).color.a, 0);
        assert_eq!(
            [
                TextRole::Display,
                TextRole::Headline,
                TextRole::Title,
                TextRole::Body,
                TextRole::Label
            ]
            .map(|role| t.text_size(role)),
            [36.0, 28.0, 22.0, 14.0, 12.0]
        );
        assert_eq!(Tokens::new(), t);
    }

    #[test]
    fn builders_set_what_getters_read() {
        let shadow = Shadow::new(Color::rgba(1, 2, 3, 200)).blur(9.0);
        let t = Tokens::new()
            .with_state_layers(0.1, 0.2, 0.3)
            .with_dragged(0.4)
            .with_shapes(1.0, 2.0, 3.0)
            .with_elevation(2, shadow)
            .with_text_size(TextRole::Body, 15.0);
        assert_eq!(
            [t.hover(), t.focus(), t.pressed(), t.dragged()],
            [0.1, 0.2, 0.3, 0.4]
        );
        assert_eq!(t.shape(Shape::Medium), 2.0);
        assert_eq!(t.elevation(2), shadow);
        assert_eq!(t.text_size(TextRole::Body), 15.0);
        assert_eq!(t.text_size(TextRole::Label), 12.0, "others keep defaults");
    }

    #[test]
    fn elevation_clamps_above_five_and_ignores_such_builds() {
        let t = Tokens::new();
        assert_eq!(t.elevation(9), t.elevation(5));
        assert_eq!(t.elevation(0).color.a, 0);
        assert!(t.elevation(1).color.a > 0);
        let shadow = Shadow::new(Color::rgba(9, 9, 9, 9));
        assert_eq!(Tokens::new().with_elevation(9, shadow), Tokens::new());
    }

    #[test]
    fn state_layer_prefers_pressed_then_focus_then_hover() {
        let t = Tokens::new();
        assert_eq!(t.state_layer(eased(1.0, 0.0, 0.0)), 0.08);
        assert_eq!(t.state_layer(eased(1.0, 0.0, 1.0)), 0.12);
        assert_eq!(t.state_layer(eased(1.0, 0.5, 1.0)), 0.06);
        assert_eq!(t.state_layer(eased(0.5, 0.0, 0.0)), 0.04, "half eased");
        assert_eq!(t.state_layer(eased(0.0, 0.0, 0.0)), 0.0);
    }

    #[test]
    fn ui_round_trips_tokens_and_keeps_them_next_frame() {
        let mut rig: Rig = Rig::new(20, 20);
        let tokens = Tokens::new().with_shapes(2.0, 6.0, 9.0);
        rig.frame(|ui| ui.set_tokens(tokens));
        rig.frame(|ui| assert_eq!(ui.tokens(), tokens));
        rig.frame(|ui| assert_eq!(ui.tokens(), tokens, "still set on the next frame"));
    }
}
