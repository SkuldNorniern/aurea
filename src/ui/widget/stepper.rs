//! A stepper: a number between a minus and a plus part, changed a step at a time.

use std::fmt::Write;
use std::mem;
use std::ops::RangeInclusive;

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Align, Eased, Response, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape, Surface};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::{CursorIcon, KeyCode};

const REPEAT_AFTER: f32 = 0.42;
const REPEAT_EVERY: f32 = 0.07;
const HELD_SINCE: Channel = Channel::Custom(1);
const REPEATS: Channel = Channel::Custom(2);

/// How a stepper looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepperStyle {
    /// Fill colour.
    pub face: Color,
    /// Fill colours of the step buttons.
    pub button: StateColors,
    /// Border colour.
    pub border: Color,
    /// Value colour.
    pub text: Color,
    /// Button glyph colour.
    pub mark: Color,
    /// Corner radius.
    pub radius: f32,
}

impl StepperStyle {
    /// A stepper style from the theme.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            face: p.raised,
            button: StateColors::new(p.raised, p.hover, p.pressed),
            border: p.border,
            text: p.text,
            mark: p.dim,
            radius: theme.radius,
        }
    }
}

/// A number in `range`, `step` at a time: the buttons, the arrow keys (Page Up and Down ten steps,
/// Home and End the ends) and the wheel over it.
#[derive(Debug)]
pub struct Stepper<'v> {
    value: &'v mut f32,
    range: RangeInclusive<f32>,
    step: f32,
    decimals: usize,
    unit: &'v str,
    style: Option<StepperStyle>,
    enabled: bool,
}

impl<'v> Stepper<'v> {
    /// A stepper on `value`, limited to `range`.
    pub fn new(value: &'v mut f32, range: RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            step: 1.0,
            decimals: 0,
            unit: "",
            style: None,
            enabled: true,
        }
    }

    /// Sets how much each step changes the value.
    #[must_use]
    pub fn step(mut self, step: f32) -> Self {
        self.step = step.abs().max(f32::EPSILON);
        self
    }

    /// Digits shown after the point.
    #[must_use]
    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals.min(6);
        self
    }

    /// After the number, like `%` or ` px`.
    #[must_use]
    pub fn unit(mut self, unit: &'v str) -> Self {
        self.unit = unit;
        self
    }

    /// Sets the look.
    #[must_use]
    pub fn style(mut self, style: StepperStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Sets whether it takes input.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
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

    fn key_steps<Id: WidgetId>(&self, ui: &Ui<'_, Id>) -> f32 {
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let mut steps = 0.0;
        for (key, _) in ui.keys() {
            steps += match key {
                KeyCode::Up | KeyCode::Right => 1.0,
                KeyCode::Down | KeyCode::Left => -1.0,
                KeyCode::PageUp => 10.0,
                KeyCode::PageDown => -10.0,
                KeyCode::Home => (lo - *self.value) / self.step,
                KeyCode::End => (hi - *self.value) / self.step,
                _ => 0.0,
            };
        }
        steps
    }
}

fn button_steps<Id: WidgetId>(ui: &mut Ui<'_, Id>, id: Id, part: u16, held: bool) -> f32 {
    let now = ui.time();
    if ui.press(id, part).is_some() {
        ui.set_animated(id, part, HELD_SINCE, now);
        ui.set_animated(id, part, REPEATS, 0.0);
        return 1.0;
    }
    if !held {
        return 0.0;
    }
    ui.keep_animating();
    let since = ui.animate(id, part, HELD_SINCE, now, 0.0);
    let done = ui.animate(id, part, REPEATS, 0.0, 0.0);
    let due = ((now - since - REPEAT_AFTER) / REPEAT_EVERY).floor() + 1.0;
    if due > done {
        ui.set_animated(id, part, REPEATS, due);
        due - done
    } else {
        0.0
    }
}

impl<Id: WidgetId> Widget<Id> for Stepper<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| StepperStyle::new(ui.theme));
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let input = ui.interact(id, 0, rect, Sense::Adjust, self.enabled);
        let mut response = Response::new(rect, input);
        let side = rect.height;
        let (minus, rest) = rect.split_left(side, 0.0);
        let (plus, middle) = rest.split_right(side, 0.0);
        let can_down = self.enabled && *self.value > lo;
        let can_up = self.enabled && *self.value < hi;
        let down = ui.interact(id, 1, minus, Sense::Hover, can_down);
        let up = ui.interact(id, 2, plus, Sense::Hover, can_up);
        if down.hovered || up.hovered {
            ui.set_cursor(CursorIcon::Pointer);
        }
        if ui.press(id, 1).is_some() || ui.press(id, 2).is_some() {
            ui.state.give_focus(Some(Key::new(id, 0)));
        }

        let mut steps = button_steps(ui, id, 2, up.held) - button_steps(ui, id, 1, down.held);
        if input.focused {
            steps += self.key_steps(ui);
        }
        let before = *self.value;
        if steps != 0.0 {
            let stepped =
                lo + (((*self.value + steps * self.step) - lo) / self.step).round() * self.step;
            *self.value = stepped.clamp(lo.min(hi), hi.max(lo));
        }
        if input.enabled && ui.pointer().is_some_and(|at| rect.contains(at)) {
            let previous = *self.value;
            let wheel = ui.wheel().1;
            if wheel != 0.0 {
                let stepped =
                    lo + (((previous + wheel * self.step) - lo) / self.step).round() * self.step;
                *self.value = stepped.clamp(lo.min(hi), hi.max(lo));
                ui.consume_wheel((0.0, (*self.value - previous) / self.step));
            }
        }
        if (*self.value - before).abs() > f32::EPSILON {
            response.changed = true;
            ui.mark_changed();
        }

        let state = ui.eased(id, 0, input);
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
        Surface::new(style.face.fade(state.enabled))
            .radius(style.radius)
            .border(ui.theme.border_width, style.border.fade(state.enabled))
            .paint(ui, rect)?;
        let down_state = ui.eased(id, 1, down);
        let up_state = ui.eased(id, 2, up);
        step_button(ui, minus, MarkShape::Minus, down_state, &style)?;
        step_button(ui, plus, MarkShape::Plus, up_state, &style)?;

        let mut text = mem::take(&mut ui.state.scratch);
        text.clear();
        let _ = write!(text, "{:.*}{}", self.decimals, *self.value, self.unit);
        let drawn = Label::new(&text, style.text.fade(state.enabled))
            .align(Align::Center)
            .paint(ui, middle);
        ui.state.scratch = text;
        drawn?;
        Ok(response)
    }
}

fn step_button<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    at: Rect,
    shape: MarkShape,
    state: Eased,
    style: &StepperStyle,
) -> AureaResult<()> {
    let face = style.button.at(state);
    if state.hover > 0.01 || state.press > 0.01 {
        Surface::new(face)
            .radius(style.radius)
            .paint(ui, at.inset(2.0))?;
    }
    Mark::new(shape, style.mark.fade(state.enabled)).paint(ui, at.inset(at.height * 0.3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    struct Spin {
        rig: Rig,
        value: f32,
    }

    impl Spin {
        fn frame(&mut self) {
            self.rig.frame(|ui| {
                Stepper::new(&mut self.value, 0.0..=2.0)
                    .step(0.5)
                    .decimals(1)
                    .show(ui, 1, Rect::new(0.0, 0.0, 150.0, 30.0))
                    .expect("draw");
            });
        }

        fn click(&mut self, x: f64) {
            for pressed in [true, false] {
                self.rig.button(x, 15.0, pressed);
                self.frame();
            }
        }

        fn key(&mut self, key: KeyCode) {
            self.rig.key(key);
            self.frame();
        }

        fn at(&mut self, x: f32, y: f32) {
            self.rig.at(x, y);
            self.frame();
        }

        fn wheel(&mut self, x: f32, y: f32, notches: f64) {
            self.rig.wheel(x, y, 0.0, notches);
            self.frame();
        }
    }

    #[test]
    fn buttons_keys_and_the_wheel_step_inside_the_range() {
        let mut rig = Spin {
            rig: Rig::new(150, 30),
            value: 1.0,
        };
        rig.frame();
        rig.click(140.0);
        assert_eq!(rig.value, 1.5);
        rig.click(140.0);
        rig.click(140.0);
        assert_eq!(rig.value, 2.0, "held at the top");
        rig.click(10.0);
        assert_eq!(rig.value, 1.5);
        rig.key(KeyCode::Home);
        assert_eq!(rig.value, 0.0, "the click left the keyboard on it");
        rig.at(75.0, 15.0);
        rig.wheel(75.0, 15.0, 1.0);
        assert_eq!(rig.value, 0.5);
    }
}
