//! A slider: a [`Track`] and a [`Knob`].

use std::ops::RangeInclusive;

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::{FocusRing, Knob, StateColors, Track};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Shadow};
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

/// How a slider looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderStyle {
    pub track: StateColors,
    pub filled: Color,
    pub thickness: f32,
    pub knob: Color,
    pub knob_radius: f32,
    pub knob_shadow: Option<Shadow>,
    /// The dot in the knob, brighter while held.
    pub dot: Color,
}

impl SliderStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            track: StateColors::new(p.raised, p.hover, p.hover),
            filled: p.accent,
            thickness: 4.0,
            knob: p.knob,
            knob_radius: 9.0,
            knob_shadow: Some(Shadow::new(p.shadow).grow(1.0)),
            dot: p.accent,
        }
    }
}

/// A horizontal slider over a range. Drag it, click the track, or use the
/// arrow keys (Page Up and Down for bigger steps, Home and End for the
/// ends) once it has focus.
#[derive(Debug)]
pub struct Slider<'v> {
    value: &'v mut f32,
    range: RangeInclusive<f32>,
    step: Option<f32>,
    style: Option<SliderStyle>,
    enabled: bool,
}

impl<'v> Slider<'v> {
    pub fn new(value: &'v mut f32, range: RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            step: None,
            style: None,
            enabled: true,
        }
    }

    /// Values snap to multiples of `step` from the range's start, and the
    /// arrow keys move by it. Without one they move by a hundredth.
    pub fn step(mut self, step: f32) -> Self {
        self.step = (step > 0.0).then_some(step);
        self
    }

    pub fn style(mut self, style: SliderStyle) -> Self {
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

    /// Moves the value by the keys pressed this frame.
    fn keys<Id: WidgetId>(&mut self, ui: &Ui<'_, Id>, span: f32) {
        let step = self.step.unwrap_or(span / 100.0);
        let page = (step * 10.0).max(span / 10.0);
        for (key, _) in ui.keys() {
            *self.value += match key {
                KeyCode::Left | KeyCode::Down => -step,
                KeyCode::Right | KeyCode::Up => step,
                KeyCode::PageDown => -page,
                KeyCode::PageUp => page,
                KeyCode::Home => f32::NEG_INFINITY,
                KeyCode::End => f32::INFINITY,
                _ => 0.0,
            };
        }
    }
}

impl<Id: WidgetId> Widget<Id> for Slider<'_> {
    fn show(mut self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| SliderStyle::new(ui.theme));
        let r = style.knob_radius.min(rect.height / 2.0);
        let input = ui.interact(id, 0, rect, Sense::Adjust, self.enabled);
        let mut response = Response::new(rect, input);
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let span = hi - lo;
        let before = *self.value;
        // The knob's centre runs from one end of the groove to the other.
        let groove = rect.inset_xy(r, 0.0);
        if input.held
            && let Some(at) = ui.pointer()
        {
            let t = ((at.x - groove.x) / groove.width.max(1.0)).clamp(0.0, 1.0);
            *self.value = lo + t * span;
        }
        if input.focused {
            self.keys(ui, span);
        }
        *self.value = snap(*self.value, lo, hi, self.step);
        if (*self.value - before).abs() > f32::EPSILON {
            response.changed = true;
            ui.mark_changed();
        }

        let t = if span > 0.0 {
            (*self.value - lo) / span
        } else {
            0.0
        };
        let state = ui.eased(id, 0, input);
        // Held, the knob follows the pointer exactly; let go, it eases.
        let shown = if input.held {
            ui.set_animated(id, 0, Channel::Value, t);
            t
        } else {
            let rate = ui.theme.motion.slide * 1.6;
            ui.animate(id, 0, Channel::Value, t, rate)
        };
        let fade = state.enabled;
        Track::new(shown, style.track.at(state), style.filled.fade(fade))
            .thickness(style.thickness)
            .paint(ui, groove)?;
        let size = r * (1.0 + 0.12 * state.hover - 0.08 * state.press);
        let x = groove.x + groove.width * shown;
        let at = Rect::new(x - size, rect.center().y - size, size * 2.0, size * 2.0);
        FocusRing::new(ui.theme, size, state.focus).paint(ui, at)?;
        let dot = style.dot.fade(fade * (0.4 + 0.6 * state.press));
        let mut knob = Knob::new(size, style.knob.fade(fade)).dot(0.38, dot);
        if let Some(shadow) = style.knob_shadow {
            knob = knob.shadow(Shadow {
                color: shadow.color.fade(fade),
                ..shadow
            });
        }
        knob.paint(ui, at)?;
        Ok(response)
    }
}

/// `value` kept in `lo..=hi` and on the step.
fn snap(value: f32, lo: f32, hi: f32, step: Option<f32>) -> f32 {
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    let value = if value.is_nan() {
        lo
    } else {
        value.clamp(lo, hi)
    };
    match step {
        Some(step) => (lo + ((value - lo) / step).round() * step).min(hi),
        None => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_keeps_to_the_range_and_the_step() {
        assert_eq!(snap(0.26, 0.0, 1.0, Some(0.25)), 0.25);
        assert_eq!(snap(7.0, 0.0, 1.0, None), 1.0);
        assert_eq!(snap(f32::NEG_INFINITY, 2.0, 4.0, Some(0.5)), 2.0);
        assert_eq!(snap(f32::NAN, 2.0, 4.0, None), 2.0);
        assert_eq!(snap(9.9, 0.0, 10.0, Some(3.0)), 9.0);
    }
}
