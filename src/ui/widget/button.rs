//! A push button: a [`Surface`], an optional [`IconMark`], a [`Label`] and a [`FocusRing`].

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Font, Rect, glyph_coverage, shape_text};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Align, Response, Ui};
use crate::ui::paint::{Component, IconMark, IconSource, Label, Surface, TextSize};
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

/// How a button looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonStyle {
    /// Fill colours by state.
    pub face: StateColors,
    /// Label colours by state.
    pub text: StateColors,
    /// The outline, faded out as the face lights up; `None` for none.
    pub border: Option<Color>,
    /// Corner radius.
    pub radius: f32,
    /// Label font size.
    pub text_size: TextSize,
    /// Space each side of the label.
    pub padding: f32,
    /// How far it sinks when pressed, in pixels.
    pub sink: f32,
}

impl ButtonStyle {
    /// A button style for `tone` from the theme.
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
    icon: Option<IconSource<'t>>,
    tone: Tone,
    style: Option<ButtonStyle>,
    enabled: bool,
    selected: bool,
    font: Option<&'t Font>,
}

impl<'t> Button<'t> {
    /// A button showing `label`.
    pub fn new(label: &'t str) -> Self {
        Self {
            label,
            icon: None,
            tone: Tone::Normal,
            style: None,
            enabled: true,
            selected: false,
            font: None,
        }
    }

    /// A button showing only an icon; give it a tooltip.
    ///
    /// ```rust,no_run
    /// # use aurea::ui::{Button, Ui};
    /// # use aurea::render::Icon;
    /// # fn f(ui: &mut Ui<'_, u8>, brush: bool) -> aurea::AureaResult<()> {
    /// let at = aurea::Rect::new(0.0, 0.0, 32.0, 32.0);
    /// let r = Button::icon_only(Icon::Brush).quiet().selected(brush).show(ui, 1, at)?;
    /// ui.tooltip(&r, "Brush (B)");
    /// # Ok(())
    /// # }
    /// ```
    pub fn icon_only(icon: impl Into<IconSource<'t>>) -> Self {
        Self::new("").icon(icon)
    }

    /// An icon before the label.
    pub fn icon(mut self, icon: impl Into<IconSource<'t>>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Drawn held down, for the current tool in a tool strip.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Sets the tone.
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Uses the primary tone.
    pub fn primary(self) -> Self {
        self.tone(Tone::Primary)
    }

    /// Uses the quiet tone.
    pub fn quiet(self) -> Self {
        self.tone(Tone::Quiet)
    }

    /// Uses the danger tone.
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

    /// The width the icon and label need, with the style's padding each side.
    pub fn width<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> f32 {
        let style = self
            .style
            .unwrap_or_else(|| ButtonStyle::new(ui.theme, self.tone));
        let text = if self.label.is_empty() {
            0.0
        } else {
            self.label(&style, Color::rgb(0, 0, 0)).width(ui)
        };
        let icon = match self.icon {
            Some(_) if text > 0.0 => icon_size(ui) + style.padding * 0.5,
            Some(_) => icon_size(ui),
            None => 0.0,
        };
        icon + text + style.padding * 2.0
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

    /// Draws it in `rect` and reports the input.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

fn icon_size<Id: WidgetId>(ui: &Ui<'_, Id>) -> f32 {
    (ui.theme.text_size * 1.35).round()
}

impl<Id: WidgetId> Widget<Id> for Button<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let input = ui.interact(id, 0, rect, Sense::Click, self.enabled);
        #[cfg(feature = "accessibility")]
        ui.accessible_description(id, 0, Role::Button, self.label, None);
        let mut state = ui.eased(id, 0, input);
        let mut style = self
            .style
            .unwrap_or_else(|| ButtonStyle::new(ui.theme, self.tone));
        if self.selected {
            state.press = state.press.max(1.0);
            style.sink = 0.0;
            style.text = StateColors::same(ui.theme.palette.accent);
        }
        let body = rect.inset(state.press * style.sink);
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
        let mut surface = Surface::new(style.face.at(state)).radius(style.radius);
        if let Some(border) = style.border {
            let shown = border.fade(state.enabled * (1.0 - state.hover));
            surface = surface.border(ui.theme.border_width, shown);
        }
        surface.paint(ui, body)?;
        let ink = style.text.at(state);
        let inner = body.inset_xy(style.padding * 0.5, 0.0);
        let Some(icon) = self.icon else {
            let nudge = symbol_nudge(ui, self.label, self.font, style.text_size);
            self.label(&style, ink).paint(
                ui,
                Rect::new(inner.x, inner.y + nudge, inner.width, inner.height),
            )?;
            return Ok(Response::new(rect, input));
        };
        let size = icon_size(ui).min(body.height - 4.0).max(0.0);
        let label_w = if self.label.is_empty() {
            0.0
        } else {
            self.label(&style, ink).width(ui) + style.padding * 0.5
        };
        let x = body.x + ((body.width - size - label_w) / 2.0).max(style.padding * 0.5);
        let y = body.y + (body.height - size) / 2.0;
        IconMark::new(icon, ink).paint(ui, Rect::new(x, y, size, size))?;
        if label_w > 0.0 {
            let start = x + size + style.padding * 0.5;
            let text = Rect::new(
                start,
                body.y,
                (body.x + body.width - start).max(0.0),
                body.height,
            );
            self.label(&style, ink)
                .align(Align::Start)
                .paint(ui, text)?;
        }
        Ok(Response::new(rect, input))
    }
}

/// How far to move a label of one symbol, such as `×` or `↑`, so its ink rather than its line
/// sits in the middle: symbols sit around the middle of lowercase letters, below the line's.
fn symbol_nudge<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    label: &str,
    font: Option<&Font>,
    size: TextSize,
) -> f32 {
    let mut chars = label.chars();
    let (Some(symbol), None) = (chars.next(), chars.next()) else {
        return 0.0;
    };
    if symbol.is_alphanumeric() {
        return 0.0;
    }
    let fonts = ui.fonts();
    let font = font.unwrap_or_else(|| fonts.of(size));
    let Ok(shaped) = shape_text(label, font) else {
        return 0.0;
    };
    let [glyph] = shaped.glyphs.as_slice() else {
        return 0.0;
    };
    let Ok(ink) = glyph_coverage(glyph.face, glyph.id, font.size) else {
        return 0.0;
    };
    if ink.height == 0 {
        return 0.0;
    }
    // the baseline sits (ascent - descent) / 2 below the line's middle; the ink's middle sits
    // this far above the baseline
    let ink_middle = -(ink.top as f32 + ink.height as f32 / 2.0);
    ink_middle - (shaped.ascent - shaped.descent) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Icon;
    use crate::ui::rig::Rig;

    #[test]
    fn icons_draw_and_take_room() {
        let mut rig = Rig::new(120, 40);
        let (alone, labelled) = rig.frame(|ui| {
            let alone = Button::icon_only(Icon::Plus).width(ui);
            let labelled = Button::new("Add").icon(Icon::Plus).width(ui);
            Button::icon_only(Icon::Plus)
                .quiet()
                .show(ui, 1, Rect::new(0.0, 0.0, 40.0, 40.0))
                .expect("draw");
            (alone, labelled)
        });
        assert!(labelled > alone + 10.0, "{labelled} vs {alone}");
        assert!(
            rig.pixel(20, 20)[0] > 128,
            "plus drawn in the dim text colour"
        );
        assert_eq!(rig.pixel(4, 4)[0], 0, "nothing in the corner");
    }

    #[test]
    fn one_symbol_labels_sit_in_the_middle() {
        for symbol in ["\u{d7}", "\u{2191}", "+", "A"] {
            let mut rig = Rig::new(30, 30);
            rig.frame(|ui| {
                Button::new(symbol)
                    .quiet()
                    .show(ui, 1, Rect::new(0.0, 0.0, 30.0, 30.0))
                    .expect("draw");
            });
            let inked: Vec<u32> = (0..30)
                .filter(|&y| (0..30).any(|x| rig.pixel(x, y)[1] > 60))
                .collect();
            let (top, bottom) = (inked[0], inked[inked.len() - 1] + 1);
            let off = (top + bottom) as f32 / 2.0 - 15.0;
            // letters keep the line's middle, which fonts place a pixel apart
            let room = if symbol == "A" { 1.0 } else { 0.5 };
            assert!(
                off.abs() <= room,
                "{symbol} ink {top}..{bottom}, {off} off centre"
            );
        }
    }
}
