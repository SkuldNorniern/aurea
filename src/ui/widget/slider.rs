//! A slider: a [`Track`] and a [`Knob`].

use std::ops::RangeInclusive;

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::{Action, ActionData};
use crate::ui::component::{FocusRing, Knob, StateColors, Track};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Shadow};
use crate::ui::state::{Channel, Interaction, Key, Sense, WidgetId};
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

/// A horizontal slider over a range.
///
/// ```rust,no_run
/// # use aurea::ui::{Slider, Ui};
/// # fn f(ui: &mut Ui<'_, u8>, exposure: &mut f32) -> aurea::AureaResult<()> {
/// let at = aurea::Rect::new(0.0, 0.0, 200.0, 24.0);
/// // Filled out from 0 either way; a double click puts it back to 0.
/// Slider::new(exposure, -100.0..=100.0).origin(0.0).reset_to(0.0).show(ui, 1, at)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Slider<'v> {
    value: &'v mut f32,
    range: RangeInclusive<f32>,
    step: Option<f32>,
    origin: Option<f32>,
    reset: Option<f32>,
    style: Option<SliderStyle>,
    enabled: bool,
}

/// Geometry and native colors supplied to a custom slider track painter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderTrack {
    /// Actual thin bar, already clipped by any enclosing scroll area.
    pub rect: Rect,
    /// Native fill span and colors, available for reuse or customization.
    pub track: Track,
    /// Knob position as a share of the track width.
    pub position: f32,
    pub enabled: bool,
    /// Disabled opacity, already applied to the painter's drawing context.
    pub opacity: f32,
}

const FINE: f32 = 0.1;
const LAST_X: Channel = Channel::Custom(1);
const RESET_HELD: Channel = Channel::Custom(2);

impl<'v> Slider<'v> {
    pub fn new(value: &'v mut f32, range: RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            step: None,
            origin: None,
            reset: None,
            style: None,
            enabled: true,
        }
    }

    /// Fill from `origin` to the value rather than from the start: a neutral middle, like 0 in -100
    /// to 100.
    pub fn origin(mut self, origin: f32) -> Self {
        self.origin = Some(origin);
        self
    }

    /// A double click sets the value back to `value`.
    pub fn reset_to(mut self, value: f32) -> Self {
        self.reset = Some(value);
        self
    }

    /// Values snap to multiples of `step` from the range's start, and the arrow keys move by it.
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

    /// Replaces only the track drawing, retaining native input, knob and focus.
    ///
    /// ```rust,no_run
    /// # use aurea::ui::{Slider, Ui};
    /// # use aurea::render::{Color, GradientStop, LinearGradient, Point, Rect};
    /// # fn hue(ui: &mut Ui<'_, u8>, value: &mut f32) -> aurea::AureaResult<()> {
    /// Slider::new(value, 0.0..=1.0).show_with_track(
    ///     ui, 1, Rect::new(0.0, 0.0, 200.0, 24.0), |ui, track| {
    ///         let bar = track.rect;
    ///         ui.ctx.fill_linear_gradient(&LinearGradient {
    ///             start: Point::new(bar.x, bar.y),
    ///             end: Point::new(bar.right(), bar.y),
    ///             stops: vec![
    ///                 GradientStop { offset: 0.0, color: Color::rgb(255, 0, 0) },
    ///                 GradientStop { offset: 1.0, color: Color::rgb(0, 0, 255) },
    ///             ],
    ///         }, bar)
    ///     },
    /// )?;
    /// # Ok(()) }
    /// ```
    pub fn show_with_track<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        painter: impl FnOnce(&mut Ui<'_, Id>, SliderTrack) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        self.show_painted(ui, id, rect, |ui, track| {
            ui.ctx.set_alpha(track.opacity)?;
            painter(ui, track)
        })
    }

    fn pointer<Id: WidgetId>(
        &mut self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        groove: Rect,
        (lo, span): (f32, f32),
    ) {
        let press = ui.press(id, 0);
        let resetting = ui.state.tweens.get((Key::new(id, 0), RESET_HELD));
        let double = press.is_some_and(|press| press.clicks >= 2);
        if let Some(reset) = self.reset
            && (double || resetting.is_some_and(|held| held > 0.5))
        {
            ui.set_animated(id, 0, RESET_HELD, 1.0);
            *self.value = reset;
            return;
        }
        let Some(at) = ui.pointer() else {
            return;
        };
        let width = groove.width.max(1.0);
        match ui.state.tweens.get((Key::new(id, 0), LAST_X)) {
            Some(last) if ui.modifiers().shift && press.is_none() => {
                *self.value += (at.x - last) / width * span * FINE;
            }
            _ => *self.value = lo + ((at.x - groove.x) / width).clamp(0.0, 1.0) * span,
        }
        ui.set_animated(id, 0, LAST_X, at.x);
    }

    fn wheel<Id: WidgetId>(
        &mut self,
        ui: &mut Ui<'_, Id>,
        input: Interaction,
        (lo, hi): (f32, f32),
    ) {
        if input.enabled && input.hovered {
            let wheel = ui.wheel().1;
            let step = self.step.unwrap_or((hi - lo).abs() / 100.0);
            if wheel != 0.0 && step.is_finite() && step > 0.0 {
                let previous = snap(*self.value, lo, hi, self.step);
                let next = (previous + wheel * step).clamp(lo.min(hi), lo.max(hi));
                *self.value = next;
                ui.consume_wheel((0.0, (next - previous) / step));
            }
        }
    }

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
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        self.show_painted(ui, id, rect, |ui, track| track.track.paint(ui, track.rect))
    }
}

impl Slider<'_> {
    fn show_painted<Id: WidgetId>(
        mut self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        painter: impl FnOnce(&mut Ui<'_, Id>, SliderTrack) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| SliderStyle::new(ui.theme));
        let r = style.knob_radius.min(rect.height / 2.0);
        let input = ui.interact(id, 0, rect, Sense::Adjust, self.enabled);
        let mut response = Response::new(rect, input);
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let span = hi - lo;
        let before = *self.value;
        #[cfg(feature = "accessibility")]
        self.accessibility(ui, id, (lo, hi));
        let groove = rect.inset_xy(r, 0.0);
        if input.held {
            self.pointer(ui, id, groove, (lo, span));
        } else {
            ui.set_animated(id, 0, RESET_HELD, 0.0);
        }
        if input.focused {
            self.keys(ui, span);
        }
        self.wheel(ui, input, (lo, hi));
        *self.value = snap(*self.value, lo, hi, self.step);
        #[cfg(feature = "accessibility")]
        self.describe_accessibility(ui, id, (lo, hi));
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
        let shown = if input.held {
            ui.set_animated(id, 0, Channel::Value, t);
            t
        } else {
            let rate = ui.theme.motion.slide * 1.6;
            ui.animate(id, 0, Channel::Value, t, rate)
        };
        let fade = state.enabled;
        let empty = style.track.at(state);
        let filled = if self.enabled {
            style.filled
        } else {
            empty.mix(ui.theme.palette.dim, 0.5)
        };
        let from = match self.origin {
            Some(origin) if span > 0.0 => ((origin.clamp(lo, hi) - lo) / span).clamp(0.0, 1.0),
            _ => 0.0,
        };
        let track = Track::new(shown, empty, filled)
            .span(from.min(shown), from.max(shown))
            .thickness(style.thickness);
        let bar = track.bar(groove);
        ui.clipped(bar, |ui| {
            painter(
                ui,
                SliderTrack {
                    rect: bar,
                    track,
                    position: shown,
                    enabled: self.enabled,
                    opacity: fade,
                },
            )
        })?;
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

#[cfg(feature = "accessibility")]
impl Slider<'_> {
    fn accessibility<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, id: Id, (lo, hi): (f32, f32)) {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let step = self.step.unwrap_or((hi - lo) / 100.0);
        for action in ui.accessibility_actions(id, 0) {
            match (action.action, action.data) {
                (Action::Increment, _) => *self.value += step,
                (Action::Decrement, _) => *self.value -= step,
                (Action::SetValue, Some(ActionData::NumericValue(value)))
                    if value.is_finite() && lo.is_finite() && hi.is_finite() =>
                {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        *self.value = value.clamp(f64::from(lo), f64::from(hi)) as f32;
                    }
                }
                _ => {}
            }
        }
    }
    fn describe_accessibility<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        (lo, hi): (f32, f32),
    ) {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let step = self.step.unwrap_or((hi - lo) / 100.0);
        if let Some(bridge) = &mut ui.state.accessibility
            && let Some(node) = bridge.node(Key::new(id, 0))
        {
            node.set_numeric_value(f64::from(snap(*self.value, lo, hi, self.step)));
            node.set_min_numeric_value(f64::from(lo));
            node.set_max_numeric_value(f64::from(hi));
            node.set_numeric_value_step(f64::from(step));
            node.add_action(Action::Increment);
            node.add_action(Action::Decrement);
            node.add_action(Action::SetValue);
        }
    }
}

fn snap(value: f32, lo: f32, hi: f32, step: Option<f32>) -> f32 {
    let (lo, hi) = (lo.min(hi), lo.max(hi));
    let value = if value.is_nan() {
        lo
    } else {
        value.clamp(lo, hi)
    };
    match step {
        Some(step) if step.is_finite() && step > 0.0 => {
            (lo + ((value - lo) / step).round() * step).min(hi)
        }
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::window::{Modifiers, MouseButton, MouseButtons, WindowEvent};

    fn frame(raster: &mut CpuRasterizer, state: &mut UiState<u8>, value: &mut f32) {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("frame");
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            Slider::new(value, -100.0..=100.0)
                .origin(0.0)
                .reset_to(0.0)
                .show(&mut ui, 1, Rect::new(0.0, 0.0, 218.0, 24.0))
                .expect("draw");
        }
        drop(ctx);
        raster.end_frame().expect("end");
    }

    fn button(state: &mut UiState<u8>, x: f64, pressed: bool, clicks: u8) {
        state.handle(&WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed,
            modifiers: Modifiers::default(),
            x,
            y: 12.0,
            click_count: clicks,
            time: Default::default(),
        });
    }

    fn moved(state: &mut UiState<u8>, x: f64, shift: bool) {
        state.handle(&WindowEvent::MouseMove {
            x,
            y: 12.0,
            buttons: MouseButtons::from_bits(1),
            modifiers: Modifiers {
                shift,
                ..Modifiers::default()
            },
            time: Default::default(),
        });
    }

    #[test]
    fn a_double_click_resets() {
        let (mut raster, mut state, mut value) =
            (CpuRasterizer::new(218, 24), UiState::new(), 55.0);
        frame(&mut raster, &mut state, &mut value);
        button(&mut state, 150.0, true, 2);
        frame(&mut raster, &mut state, &mut value);
        assert_eq!(value, 0.0);
    }

    #[test]
    fn a_reset_holds_until_release() {
        let (mut raster, mut state, mut value) =
            (CpuRasterizer::new(218, 24), UiState::new(), 55.0);
        frame(&mut raster, &mut state, &mut value);
        button(&mut state, 150.0, true, 2);
        frame(&mut raster, &mut state, &mut value);
        assert_eq!(value, 0.0);
        moved(&mut state, 180.0, false);
        frame(&mut raster, &mut state, &mut value);
        assert_eq!(value, 0.0, "held reset ignores the drag");
        button(&mut state, 180.0, false, 1);
        frame(&mut raster, &mut state, &mut value);
        button(&mut state, 180.0, true, 1);
        frame(&mut raster, &mut state, &mut value);
        assert!(
            (value - 71.0).abs() < 0.01,
            "a new press drags again: {value}"
        );
    }

    #[test]
    fn shift_drags_finely() {
        let (mut raster, mut state, mut value) = (CpuRasterizer::new(218, 24), UiState::new(), 0.0);
        frame(&mut raster, &mut state, &mut value);
        button(&mut state, 109.0, true, 1);
        frame(&mut raster, &mut state, &mut value);
        assert!(value.abs() < 0.01, "{value}");
        moved(&mut state, 149.0, true);
        frame(&mut raster, &mut state, &mut value);
        assert!((value - 4.0).abs() < 0.01, "fine: {value}");
        moved(&mut state, 149.0, false);
        frame(&mut raster, &mut state, &mut value);
        assert!((value - 40.0).abs() < 0.01, "plain: {value}");
    }

    #[test]
    fn the_fill_starts_at_the_origin() {
        let (mut raster, mut state, mut value) =
            (CpuRasterizer::new(218, 24), UiState::new(), -50.0);
        frame(&mut raster, &mut state, &mut value);
        let out = raster.frame_output().expect("pixels");
        let px = |x: usize| {
            let i = (12 * out.width as usize + x) * 4;
            u32::from(out.bytes()[i])
                + u32::from(out.bytes()[i + 1])
                + u32::from(out.bytes()[i + 2])
        };
        let accent = Theme::dark().palette.accent;
        let accent = u32::from(accent.r) + u32::from(accent.g) + u32::from(accent.b);
        assert!(px(80).abs_diff(accent) < 30, "filled left of the middle");
        assert!(px(160).abs_diff(accent) > 60, "empty right of it");
    }

    #[test]
    fn snapping_keeps_to_the_range_and_the_step() {
        assert_eq!(snap(0.26, 0.0, 1.0, Some(0.25)), 0.25);
        assert_eq!(snap(7.0, 0.0, 1.0, None), 1.0);
        assert_eq!(snap(f32::NEG_INFINITY, 2.0, 4.0, Some(0.5)), 2.0);
        assert_eq!(snap(f32::NAN, 2.0, 4.0, None), 2.0);
        assert_eq!(snap(9.9, 0.0, 10.0, Some(3.0)), 9.0);
        assert_eq!(snap(0.37, 0.0, 1.0, Some(0.0)), 0.37);
        assert_eq!(snap(0.37, 0.0, 1.0, Some(f32::NAN)), 0.37);
        assert_eq!(snap(0.37, 0.0, 1.0, Some(-0.5)), 0.37);
    }
}
