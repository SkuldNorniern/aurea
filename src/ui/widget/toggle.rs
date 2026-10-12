//! A switch and a checkbox: a [`Track`](crate::ui::Track) or box, a
//! [`Knob`](crate::ui::Knob) or tick, and a [`Label`].

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape, Shadow, Surface};
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a switch looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToggleStyle {
    /// The track when off; on, it turns to `on`.
    pub off: StateColors,
    pub on: Color,
    pub knob: Color,
    pub knob_shadow: Option<Shadow>,
    pub border: Color,
    pub label: Color,
    /// The switch's height; it is 1.7 times as wide.
    pub height: f32,
}

impl ToggleStyle {
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

/// An on-off switch with an optional label before it. Clicking anywhere on
/// the row flips it.
#[derive(Debug)]
pub struct Toggle<'v> {
    on: &'v mut bool,
    label: Option<&'v str>,
    style: Option<ToggleStyle>,
    enabled: bool,
}

impl<'v> Toggle<'v> {
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

    pub fn style(mut self, style: ToggleStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
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

impl<Id: WidgetId> Widget<Id> for Toggle<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let input = ui.interact(id, 0, rect, Sense::Click, self.enabled);
        let mut response = Response::new(rect, input);
        if input.clicked {
            *self.on = !*self.on;
            response.changed = true;
            ui.mark_changed();
        }
        let state = ui.eased(id, 0, input);
        let slide = ui.theme.motion.slide;
        let t = ui.animate(id, 0, Channel::Value, f32::from(u8::from(*self.on)), slide);
        let style = self.style.unwrap_or_else(|| ToggleStyle::new(ui.theme));

        let h = rect.height.min(style.height);
        let (track, text) = rect.split_right(h * 1.7, ui.theme.padding);
        let track = track.centered(track.width, h);
        if let Some(label) = self.label {
            Label::new(label, style.label.fade(state.enabled)).paint(ui, text)?;
        }
        switch(ui, &style, track, (t, state))?;
        Ok(response)
    }
}

/// The switch itself: track and knob, `t` of the way to on.
fn switch<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    style: &ToggleStyle,
    track: Rect,
    (t, state): (f32, Eased),
) -> AureaResult<()> {
    let h = track.height;
    FocusRing::new(ui.theme, h / 2.0, state.focus).paint(ui, track)?;
    let fill = style.off.at(state).mix(style.on.fade(state.enabled), t);
    let border = style.border.fade(state.enabled * (1.0 - t));
    Surface::new(fill)
        .pill()
        .border(ui.theme.border_width, border)
        .paint(ui, track)?;
    // The knob stretches a little while pressed, the way it does on phones.
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

/// How a checkbox looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckboxStyle {
    pub empty: StateColors,
    pub checked: Color,
    pub mark: Color,
    pub border: Color,
    pub label: Color,
    pub size: f32,
    pub radius: f32,
}

impl CheckboxStyle {
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
    pub fn new(checked: &'v mut bool) -> Self {
        Self {
            checked,
            label: None,
            style: None,
            enabled: true,
        }
    }

    pub fn label(mut self, label: &'v str) -> Self {
        self.label = Some(label);
        self
    }

    pub fn style(mut self, style: CheckboxStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
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

impl<Id: WidgetId> Widget<Id> for Checkbox<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let input = ui.interact(id, 0, rect, Sense::Click, self.enabled);
        let mut response = Response::new(rect, input);
        if input.clicked {
            *self.checked = !*self.checked;
            response.changed = true;
            ui.mark_changed();
        }
        let state = ui.eased(id, 0, input);
        let rate = ui.theme.motion.slide * 1.5;
        let t = ui.animate(
            id,
            0,
            Channel::Value,
            f32::from(u8::from(*self.checked)),
            rate,
        );
        let style = self.style.unwrap_or_else(|| CheckboxStyle::new(ui.theme));

        let s = rect.height.min(style.size);
        let (cell, text) = rect.split_left(s, ui.theme.padding * 0.8);
        let mark = cell.centered(s, s);
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
        if let Some(label) = self.label {
            Label::new(label, style.label.fade(state.enabled)).paint(ui, text)?;
        }
        Ok(response)
    }
}
