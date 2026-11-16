//! A segmented control: one [`Surface`], a sliding highlight, and a part per option that takes
//! clicks.

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{Component, Label, Surface, TextSize};
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a segmented control looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentedStyle {
    /// Fill behind the options.
    pub background: Color,
    /// Fill of the sliding highlight.
    pub highlight: Color,
    /// Colours of unpicked options, by state.
    pub text: StateColors,
    /// Colour of the picked option.
    pub picked_text: Color,
    /// Corner radius in pixels.
    pub radius: f32,
    /// Space between the highlight and the control's edge.
    pub inset: f32,
    /// Size of the option text.
    pub text_size: TextSize,
}

impl SegmentedStyle {
    /// Creates the default style from `theme`.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            background: p.raised,
            highlight: p.accent,
            text: StateColors::new(p.dim, p.text, p.text),
            picked_text: p.on_accent,
            radius: theme.radius,
            inset: 2.0,
            text_size: TextSize::Strong,
        }
    }
}

/// A row of options, one picked, the highlight sliding between them.
#[derive(Debug)]
pub struct Segmented<'v> {
    options: &'v [&'v str],
    selected: &'v mut usize,
    style: Option<SegmentedStyle>,
    enabled: bool,
}

impl<'v> Segmented<'v> {
    /// Creates a control over `options` editing `selected`.
    pub fn new(options: &'v [&'v str], selected: &'v mut usize) -> Self {
        Self {
            options,
            selected,
            style: None,
            enabled: true,
        }
    }

    /// Sets the style.
    pub fn style(mut self, style: SegmentedStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Sets whether the control takes input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Shows the control as `id` at `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

/// One option of a [`Segmented`], for its painter.
#[derive(Clone, Copy, Debug)]
pub struct Segment<'v> {
    /// The option's text.
    pub label: &'v str,
    /// Its cell.
    pub rect: Rect,
    /// Its focus, hover and press, eased.
    pub state: Eased,
    /// How far the highlight sits over it, 0 away to 1 over it.
    pub picked: f32,
}

/// What a [`Segmented`] draws, from [`Segmented::look`].
///
/// Its options are one [`Look`] between them: the highlight is shared, so each option is a
/// [`Segment`] in `segments`, from the left, and [`Look::paint_segment`] draws one.
#[derive(Clone, Debug)]
pub struct SegmentedParts<'v> {
    /// Where the highlight is, in options: 0 over the first, one more for each option along.
    pub slide: f32,
    /// Whether it takes input; the default look fades when not.
    pub enabled: bool,
    /// The style it draws with, the theme's unless one was given.
    pub style: SegmentedStyle,
    /// The options, from the left.
    pub segments: Vec<Segment<'v>>,
}

impl<'v> Segmented<'v> {
    /// Takes the input as [`Segmented::show`] does and works out what it would draw, without
    /// drawing: the app draws it, or [`Look::paint_default`] does, or its pieces
    /// [`Look::paint_track`] and [`Look::paint_segment`]. With no options there is nothing to draw.
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<SegmentedParts<'v>> {
        let mut response = Response {
            rect,
            ..Response::default()
        };
        let n = self.options.len();
        let style = self.style.unwrap_or_else(|| SegmentedStyle::new(ui.theme));
        #[allow(clippy::cast_precision_loss)]
        let target = (*self.selected).min(n.saturating_sub(1)) as f32;
        let slide = ui.animate(id, u16::MAX, Channel::Value, target, ui.theme.motion.slide);
        let mut segments = Vec::with_capacity(n);
        for (i, option) in self.options.iter().enumerate() {
            let part = u16::try_from(i).unwrap_or(u16::MAX - 1);
            let cell = rect.column(i, n, 0.0);
            let input = ui.interact(id, part, cell, Sense::Click, self.enabled);
            #[cfg(feature = "accessibility")]
            {
                let on = *self.selected == i;
                ui.accessible_description(id, part, Role::RadioButton, option, None);
                ui.with_accessible(id, part, |node| node.set_toggled(on.into()));
            }
            let state = ui.eased(id, part, input);
            if input.clicked && *self.selected != i {
                *self.selected = i;
                response.changed = true;
                ui.mark_changed();
            }
            response.hovered |= input.hovered;
            response.focused |= input.focused;
            #[allow(clippy::cast_precision_loss)]
            let picked = (1.0 - (slide - i as f32).abs()).clamp(0.0, 1.0);
            segments.push(Segment {
                label: option,
                rect: cell,
                state,
                picked,
            });
        }
        Look {
            rect,
            // no input of its own: the control is as enabled as it was asked to be
            state: Eased {
                enabled: f32::from(u8::from(self.enabled)),
                ..Eased::default()
            },
            held_at: None,
            response,
            parts: SegmentedParts {
                slide,
                enabled: self.enabled,
                style,
                segments,
            },
        }
    }
}

impl Look<SegmentedParts<'_>> {
    /// The control's own look: the background and highlight, then each option. Nothing with no
    /// options.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        if self.parts.segments.is_empty() {
            return Ok(());
        }
        self.paint_track(ui)?;
        for i in 0..self.parts.segments.len() {
            self.paint_segment(ui, i)?;
        }
        Ok(())
    }

    /// The fade for a control that takes no input.
    fn fade<Id: WidgetId>(&self, ui: &Ui<'_, Id>) -> f32 {
        if self.parts.enabled {
            1.0
        } else {
            ui.theme.disabled_alpha
        }
    }

    /// The rect the highlight is in, at `slide`.
    pub fn highlight(&self) -> Rect {
        let n = self.parts.segments.len().max(1);
        let width = self.rect.column(0, n, 0.0).width;
        Rect::new(
            self.rect.x + width * self.parts.slide,
            self.rect.y,
            width,
            self.rect.height,
        )
    }

    /// The background behind the options, and the highlight sliding over them.
    pub fn paint_track<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let style = &self.parts.style;
        let fade = self.fade(ui);
        Surface::new(style.background.fade(fade))
            .radius(style.radius)
            .paint(ui, self.rect)?;
        let inner = (style.radius - style.inset).max(0.0);
        Surface::new(style.highlight.fade(fade))
            .radius(inner)
            .paint(ui, self.highlight().inset(style.inset))
    }

    /// Option `i`'s focus ring and label, its text picked as the highlight lands on it.
    pub fn paint_segment<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, i: usize) -> AureaResult<()> {
        let Some(segment) = self.parts.segments.get(i) else {
            return Ok(());
        };
        let style = &self.parts.style;
        let fade = self.fade(ui);
        FocusRing::new(ui.theme, style.radius, segment.state.focus).paint(ui, segment.rect)?;
        let color = style
            .text
            .at(segment.state)
            .mix(style.picked_text.fade(fade), segment.picked);
        Label::new(segment.label, color)
            .size(style.text_size)
            .centered()
            .paint(ui, segment.rect.inset_xy(6.0, 0.0))
    }
}

impl<Id: WidgetId> Widget<Id> for Segmented<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let look = self.look(ui, id, rect);
        look.paint_default(ui)?;
        Ok(look.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    #[test]
    fn look_draws_the_apps_look_and_still_picks() {
        let mut rig = Rig::new(200, 40);
        let at = Rect::new(0.0, 0.0, 200.0, 40.0);
        let mut picked = 0;
        let mut seen = 0;
        let draw = |rig: &mut Rig, picked: &mut usize, seen: &mut usize| {
            rig.frame(|ui| {
                let control = Segmented::new(&["One", "Two", "Three"], picked).look(ui, 1, at);
                *seen = control.parts.segments.len();
                ui.fill(control.rect, Color::rgb(0, 200, 0)).expect("draw");
            });
        };
        draw(&mut rig, &mut picked, &mut seen);
        assert_eq!(seen, 3, "one look for all three options");
        assert_eq!(rig.pixel(100, 20), [0, 200, 0, 255], "the app's look");
        rig.at(170, 20);
        draw(&mut rig, &mut picked, &mut seen);
        rig.button(170, 20, true);
        draw(&mut rig, &mut picked, &mut seen);
        rig.button(170, 20, false);
        draw(&mut rig, &mut picked, &mut seen);
        assert_eq!(picked, 2, "the click still picks the last");
    }
}
