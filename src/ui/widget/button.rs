//! A push button: a [`Surface`], a [`Label`] and a [`FocusRing`].

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Font, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Surface, TextSize};
use crate::ui::state::{Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a button stands out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    /// Raised, for most actions.
    #[default]
    Normal,
    /// Filled with the accent: the action a view is for.
    Primary,
    /// No fill until hovered, for actions that should not compete.
    Quiet,
    /// Filled red: it cannot be undone.
    Danger,
}

/// How a button looks. [`ButtonStyle::new`] makes one from the theme;
/// change any field after.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonStyle {
    pub face: StateColors,
    pub text: StateColors,
    /// The outline, faded out as the face lights up; `None` for none.
    pub border: Option<Color>,
    pub radius: f32,
    pub text_size: TextSize,
    /// Space each side of the label.
    pub padding: f32,
    /// How far it sinks when pressed, in pixels.
    pub sink: f32,
}

impl ButtonStyle {
    pub fn new(theme: &Theme, tone: Tone) -> Self {
        let p = theme.palette;
        let lit = |c: Color| c.mix(Color::rgba(255, 255, 255, c.a), 0.12);
        let deep = |c: Color| c.mix(Color::rgba(0, 0, 0, c.a), 0.18);
        let (face, text, border) = match tone {
            Tone::Normal => (
                StateColors::new(p.raised, p.hover, p.pressed),
                StateColors::same(p.text),
                Some(p.border),
            ),
            Tone::Primary => (
                StateColors::new(p.accent, lit(p.accent), deep(p.accent)),
                StateColors::same(p.on_accent),
                None,
            ),
            Tone::Quiet => (
                StateColors::new(p.hover.with_alpha(0), p.hover, p.pressed),
                StateColors::new(p.dim, p.text, p.text),
                None,
            ),
            Tone::Danger => (
                StateColors::new(p.danger, lit(p.danger), deep(p.danger)),
                StateColors::same(p.on_accent),
                None,
            ),
        };
        Self {
            face,
            text,
            border,
            radius: theme.radius,
            text_size: TextSize::Strong,
            padding: theme.padding * 1.2,
            sink: 0.5,
        }
    }
}

/// A button with a label.
///
/// ```rust,no_run
/// # use aurea::ui::{Button, Ui};
/// # fn f(ui: &mut Ui<'_, u8>) -> aurea::AureaResult<()> {
/// let at = aurea::Rect::new(0.0, 0.0, 90.0, 30.0);
/// if Button::new("Delete").danger().show(ui, 1, at)?.clicked {
///     // delete
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Button<'t> {
    label: &'t str,
    tone: Tone,
    style: Option<ButtonStyle>,
    enabled: bool,
    font: Option<&'t Font>,
}

impl<'t> Button<'t> {
    pub fn new(label: &'t str) -> Self {
        Self {
            label,
            tone: Tone::Normal,
            style: None,
            enabled: true,
            font: None,
        }
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn primary(self) -> Self {
        self.tone(Tone::Primary)
    }

    pub fn quiet(self) -> Self {
        self.tone(Tone::Quiet)
    }

    pub fn danger(self) -> Self {
        self.tone(Tone::Danger)
    }

    /// A look of its own instead of the tone's.
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// A disabled button is drawn faded and takes no input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The label's font instead of the style's.
    pub fn font(mut self, font: &'t Font) -> Self {
        self.font = Some(font);
        self
    }

    /// The width the label needs, with the style's padding each side.
    pub fn width<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> f32 {
        let style = self
            .style
            .unwrap_or_else(|| ButtonStyle::new(ui.theme, self.tone));
        self.label(&style, Color::rgb(0, 0, 0)).width(ui) + style.padding * 2.0
    }

    fn label(&self, style: &ButtonStyle, color: Color) -> Label<'t> {
        let label = Label::new(self.label, color)
            .size(style.text_size)
            .centered();
        match self.font {
            Some(font) => label.font(font),
            None => label,
        }
    }

    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

impl<Id: WidgetId> Widget<Id> for Button<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let input = ui.interact(id, 0, rect, Sense::Click, self.enabled);
        let state = ui.eased(id, 0, input);
        let style = self
            .style
            .unwrap_or_else(|| ButtonStyle::new(ui.theme, self.tone));
        let body = rect.inset(state.press * style.sink);
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
        let mut surface = Surface::new(style.face.at(state)).radius(style.radius);
        if let Some(border) = style.border {
            let shown = border.fade(state.enabled * (1.0 - state.hover));
            surface = surface.border(ui.theme.border_width, shown);
        }
        surface.paint(ui, body)?;
        let inner = body.inset_xy(style.padding * 0.5, 0.0);
        self.label(&style, style.text.at(state)).paint(ui, inner)?;
        Ok(Response::new(rect, input))
    }
}
