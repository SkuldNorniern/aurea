//! A switch and a checkbox: a [`Track`](crate::ui::Track) or box, a [`Knob`](crate::ui::Knob) or
//! tick, and a [`Label`].

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape, Shadow, Surface};
#[cfg(feature = "accessibility")]
use crate::ui::state::Key;
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a switch looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToggleStyle {
    /// The track when off; on, it turns to `on`.
    pub off: StateColors,
    /// Track colour when on.
    pub on: Color,
    /// Knob colour.
    pub knob: Color,
    /// Shadow under the knob.
    pub knob_shadow: Option<Shadow>,
    /// Track outline.
    pub border: Color,
    /// Label text colour.
    pub label: Color,
    /// The switch's height; it is 1.7 times as wide.
    pub height: f32,
}

impl ToggleStyle {
    /// Default style from the theme.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            off: StateColors::new(p.raised, p.hover, p.hover),
            on: p.accent,
            knob: p.knob,
            knob_shadow: Some(Shadow::new(p.shadow)),
            border: p.border,
            label: p.text,
            height: 22.0,
        }
    }
}

/// An on-off switch with an optional label before it.
#[derive(Debug)]
pub struct Toggle<'v> {
    on: &'v mut bool,
    label: Option<&'v str>,
    style: Option<ToggleStyle>,
    enabled: bool,
}

impl<'v> Toggle<'v> {
    /// Creates a switch bound to `on`.
    pub fn new(on: &'v mut bool) -> Self {
        Self {
            on,
            label: None,
            style: None,
            enabled: true,
        }
    }

    /// Text at the row's start; the switch goes at its end.
    pub fn label(mut self, label: &'v str) -> Self {
        self.label = Some(label);
        self
    }

    /// Sets the look; defaults to the theme's.
    pub fn style(mut self, style: ToggleStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Whether the switch takes input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws the switch in `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

/// What a [`Toggle`] draws, for a painter given to [`Toggle::show_with`].
#[derive(Clone, Copy, Debug)]
pub struct ToggleParts<'v> {
    /// Whether it is on, after this frame's click.
    pub on: bool,
    /// How far the knob has slid, 0 off to 1 on.
    pub progress: f32,
    /// The label, if any.
    pub label: Option<&'v str>,
    /// Whether it takes input; the default look fades when not.
    pub enabled: bool,
    /// The style it draws with, the theme's unless one was given.
    pub style: ToggleStyle,
}

impl<'v> Toggle<'v> {
    /// Takes the input and works out what it would draw, without drawing.
    fn run<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> (Response, Look<ToggleParts<'v>>) {
        let input = ui.interact(id, 0, rect, Sense::Click, self.enabled);
        let mut response = Response::new(rect, input);
        if input.clicked {
            *self.on = !*self.on;
            response.changed = true;
            ui.mark_changed();
        }
        let state = ui.eased(id, 0, input);
        #[cfg(feature = "accessibility")]
        {
            ui.accessible_description(id, 0, Role::Switch, self.label.unwrap_or(""), None);
            if let Some(bridge) = &mut ui.state.accessibility
                && let Some(node) = bridge.node(Key::new(id, 0))
            {
                node.set_toggled((*self.on).into());
            }
        }
        let slide = ui.theme.motion.slide;
        let progress = ui.animate(id, 0, Channel::Value, f32::from(u8::from(*self.on)), slide);
        let style = self.style.unwrap_or_else(|| ToggleStyle::new(ui.theme));
        let look = Look {
            rect,
            state,
            held_at: ui.held_at(id, 0),
            parts: ToggleParts {
                on: *self.on,
                progress,
                label: self.label,
                enabled: self.enabled,
                style,
            },
        };
        (response, look)
    }

    /// Takes the input as [`Toggle::show`] does, then has `paint` draw it instead of its own look.
    /// [`Look::paint_default`], [`Look::paint_track`] and [`Look::paint_label`] draw the parts of
    /// the usual look.
    pub fn show_with<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        paint: impl FnOnce(&mut Ui<'_, Id>, &Look<ToggleParts<'v>>) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        let (response, look) = self.run(ui, id, rect);
        paint(ui, &look)?;
        Ok(response)
    }
}

impl Look<ToggleParts<'_>> {
    /// The switch's own look: the label, then the track and knob.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_label(ui)?;
        self.paint_track(ui)
    }

    /// The track at the row's end with the knob sliding across it, and its focus ring.
    pub fn paint_track<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style, t) = (self.state, &self.parts.style, self.parts.progress);
        let (track, _) = self.split(ui);
        let h = track.height;
        FocusRing::new(ui.theme, h / 2.0, state.focus).paint(ui, track)?;
        let fill = style.off.at(state).mix(style.on.fade(state.enabled), t);
        let border = style.border.fade(state.enabled * (1.0 - t));
        Surface::new(fill)
            .pill()
            .border(ui.theme.border_width, border)
            .paint(ui, track)?;
        let pad = 3.0;
        let d = h - pad * 2.0;
        let stretch = d * 0.25 * state.press;
        let travel = track.width - pad * 2.0 - d - stretch;
        let knob = Rect::new(track.x + pad + travel * t, track.y + pad, d + stretch, d);
        let mut surface = Surface::new(style.knob.fade(state.enabled)).pill();
        if let Some(shadow) = style.knob_shadow {
            surface = surface.shadow(Shadow {
                color: shadow.color.fade(state.enabled),
                ..shadow
            });
        }
        surface.paint(ui, knob)
    }

    /// The label before the track, faded when disabled.
    pub fn paint_label<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (_, text) = self.split(ui);
        if let Some(label) = self.parts.label {
            let color = self.parts.style.label.fade(self.state.enabled);
            return Label::new(label, color).paint(ui, text);
        }
        Ok(())
    }

    /// The track and the rect the label is left in, from the row.
    pub fn split<Id: WidgetId>(&self, ui: &Ui<'_, Id>) -> (Rect, Rect) {
        let h = self.rect.height.min(self.parts.style.height);
        let (track, text) = self.rect.split_right(h * 1.7, ui.theme.padding);
        (track.centered(track.width, h), text)
    }
}

impl<Id: WidgetId> Widget<Id> for Toggle<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        self.show_with(ui, id, rect, |ui, look| look.paint_default(ui))
    }
}

/// How a checkbox looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckboxStyle {
    /// Box fill by state.
    pub empty: StateColors,
    /// Fill when ticked.
    pub checked: Color,
    /// Tick colour.
    pub mark: Color,
    /// Box outline.
    pub border: Color,
    /// Label text colour.
    pub label: Color,
    /// Box side length.
    pub size: f32,
    /// Corner radius of the box.
    pub radius: f32,
}

impl CheckboxStyle {
    /// Default style from the theme.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            empty: StateColors::new(p.raised, p.hover, p.pressed),
            checked: p.accent,
            mark: p.on_accent,
            border: p.border,
            label: p.text,
            size: 18.0,
            radius: theme.radius.min(5.0),
        }
    }
}

/// A box to tick, with an optional label after it.
#[derive(Debug)]
pub struct Checkbox<'v> {
    checked: &'v mut bool,
    label: Option<&'v str>,
    style: Option<CheckboxStyle>,
    enabled: bool,
}

impl<'v> Checkbox<'v> {
    /// Creates a box bound to `checked`.
    pub fn new(checked: &'v mut bool) -> Self {
        Self {
            checked,
            label: None,
            style: None,
            enabled: true,
        }
    }

    /// Text after the box.
    pub fn label(mut self, label: &'v str) -> Self {
        self.label = Some(label);
        self
    }

    /// Sets the look; defaults to the theme's.
    pub fn style(mut self, style: CheckboxStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Whether the box takes input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws the box in `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

/// What a [`Checkbox`] draws, for a painter given to [`Checkbox::show_with`].
#[derive(Clone, Copy, Debug)]
pub struct CheckboxParts<'v> {
    /// Whether it is ticked, after this frame's click.
    pub checked: bool,
    /// How far the tick has drawn in, 0 unticked to 1 ticked.
    pub progress: f32,
    /// The label, if any.
    pub label: Option<&'v str>,
    /// Whether it takes input; the default look fades when not.
    pub enabled: bool,
    /// The style it draws with, the theme's unless one was given.
    pub style: CheckboxStyle,
}

impl<'v> Checkbox<'v> {
    /// Takes the input and works out what it would draw, without drawing.
    fn run<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> (Response, Look<CheckboxParts<'v>>) {
        let input = ui.interact(id, 0, rect, Sense::Click, self.enabled);
        let mut response = Response::new(rect, input);
        if input.clicked {
            *self.checked = !*self.checked;
            response.changed = true;
            ui.mark_changed();
        }
        let state = ui.eased(id, 0, input);
        let rate = ui.theme.motion.slide * 1.5;
        let progress = ui.animate(
            id,
            0,
            Channel::Value,
            f32::from(u8::from(*self.checked)),
            rate,
        );
        let style = self.style.unwrap_or_else(|| CheckboxStyle::new(ui.theme));
        let look = Look {
            rect,
            state,
            held_at: ui.held_at(id, 0),
            parts: CheckboxParts {
                checked: *self.checked,
                progress,
                label: self.label,
                enabled: self.enabled,
                style,
            },
        };
        (response, look)
    }

    /// Takes the input as [`Checkbox::show`] does, then has `paint` draw it instead of its own
    /// look. [`Look::paint_default`], [`Look::paint_box`] and [`Look::paint_label`] draw the parts
    /// of the usual look.
    pub fn show_with<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        paint: impl FnOnce(&mut Ui<'_, Id>, &Look<CheckboxParts<'v>>) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        let (response, look) = self.run(ui, id, rect);
        paint(ui, &look)?;
        Ok(response)
    }
}

impl Look<CheckboxParts<'_>> {
    /// The checkbox's own look: the box, then the label.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_box(ui)?;
        self.paint_label(ui)
    }

    /// The box at the row's start with its focus ring, fill, outline and tick.
    pub fn paint_box<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style, t) = (self.state, &self.parts.style, self.parts.progress);
        let (mark, _) = self.split(ui);
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, mark)?;
        let fill = style
            .empty
            .at(state)
            .mix(style.checked.fade(state.enabled), t);
        let border = style.border.fade(state.enabled * (1.0 - t));
        Surface::new(fill)
            .radius(style.radius)
            .border(ui.theme.border_width, border)
            .paint(ui, mark)?;
        if t > 0.01 {
            Mark::new(MarkShape::Check, style.mark.fade(state.enabled))
                .progress(t)
                .paint(ui, mark)?;
        }
        Ok(())
    }

    /// The label after the box, faded when disabled.
    pub fn paint_label<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (_, text) = self.split(ui);
        if let Some(label) = self.parts.label {
            let color = self.parts.style.label.fade(self.state.enabled);
            return Label::new(label, color).paint(ui, text);
        }
        Ok(())
    }

    /// The box, and the rect right of it the label goes in.
    pub fn split<Id: WidgetId>(&self, ui: &Ui<'_, Id>) -> (Rect, Rect) {
        let s = self.rect.height.min(self.parts.style.size);
        let (cell, text) = self.rect.split_left(s, ui.theme.padding * 0.8);
        (cell.centered(s, s), text)
    }
}

impl<Id: WidgetId> Widget<Id> for Checkbox<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        self.show_with(ui, id, rect, |ui, look| look.paint_default(ui))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    fn draw_toggle(rig: &mut Rig, on: &mut bool, at: Rect) {
        rig.frame(|ui| {
            Toggle::new(on)
                .show_with(ui, 1, at, |ui, look| {
                    ui.fill(look.rect, Color::rgb(0, 200, 0))
                })
                .expect("draw");
        });
    }

    fn draw_checkbox(rig: &mut Rig, checked: &mut bool, at: Rect) {
        rig.frame(|ui| {
            Checkbox::new(checked)
                .show_with(ui, 1, at, |ui, look| {
                    ui.fill(look.rect, Color::rgb(0, 200, 0))
                })
                .expect("draw");
        });
    }

    #[test]
    fn toggle_show_with_draws_the_apps_look_and_still_toggles() {
        let mut rig = Rig::new(120, 40);
        let at = Rect::new(0.0, 0.0, 120.0, 40.0);
        let mut on = false;
        draw_toggle(&mut rig, &mut on, at);
        assert_eq!(
            rig.pixel(60, 20),
            [0, 200, 0, 255],
            "the app's look, not the default"
        );
        rig.at(110, 20);
        draw_toggle(&mut rig, &mut on, at);
        rig.button(110, 20, true);
        draw_toggle(&mut rig, &mut on, at);
        rig.button(110, 20, false);
        draw_toggle(&mut rig, &mut on, at);
        assert!(on, "the click still turns it on");
    }

    #[test]
    fn checkbox_show_with_draws_the_apps_look_and_still_toggles() {
        let mut rig = Rig::new(120, 40);
        let at = Rect::new(0.0, 0.0, 120.0, 40.0);
        let mut checked = false;
        draw_checkbox(&mut rig, &mut checked, at);
        assert_eq!(
            rig.pixel(60, 20),
            [0, 200, 0, 255],
            "the app's look, not the default"
        );
        rig.at(10, 10);
        draw_checkbox(&mut rig, &mut checked, at);
        rig.button(10, 10, true);
        draw_checkbox(&mut rig, &mut checked, at);
        rig.button(10, 10, false);
        draw_checkbox(&mut rig, &mut checked, at);
        assert!(checked, "the click still ticks it");
    }
}
