//! A dial: a knob that turns with drag, wheel and keys.

use std::f32::consts::PI;
use std::ops::RangeInclusive;

use super::Widget;
use super::slider::snap;
use crate::AureaResult;
use crate::render::{Color, LineCap, Paint, PaintStyle, Path, Point, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::Component;
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

// ring starts lower left.
const START: f32 = 0.75 * PI;
const SWEEP: f32 = 1.5 * PI;
const FINE: f32 = 0.1;
const LAST_Y: Channel = Channel::Custom(1);

/// How a dial looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DialStyle {
    /// Empty ring.
    pub ring: Color,
    /// Filled ring.
    pub filled: Color,
    /// Ring thickness.
    pub thickness: f32,
    /// Knob inside the ring.
    pub face: Color,
    /// Value mark.
    pub mark: Color,
    /// Drag pixels for the whole range.
    pub travel: f32,
}

impl DialStyle {
    /// Default style from `theme`.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            ring: p.raised,
            filled: p.accent,
            thickness: 3.0,
            face: p.knob,
            mark: p.accent,
            travel: 200.0,
        }
    }
}

/// Round control over a range. Drag up or down, shift for fine, double click resets.
///
/// ```rust,no_run
/// # use aurea::ui::{Dial, Ui};
/// # fn f(ui: &mut Ui<'_, u8>, pan: &mut f32) -> aurea::AureaResult<()> {
/// let at = aurea::Rect::new(0.0, 0.0, 40.0, 40.0);
/// Dial::new(pan, -1.0..=1.0).origin(0.0).reset_to(0.0).show(ui, 1, at)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Dial<'v> {
    value: &'v mut f32,
    range: RangeInclusive<f32>,
    step: Option<f32>,
    origin: Option<f32>,
    reset: Option<f32>,
    style: Option<DialStyle>,
    enabled: bool,
}

impl<'v> Dial<'v> {
    /// Dial editing `value` over `range`.
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

    /// Fill from `origin`, not the start.
    pub fn origin(mut self, origin: f32) -> Self {
        self.origin = Some(origin);
        self
    }

    /// Value a double click goes back to.
    pub fn reset_to(mut self, value: f32) -> Self {
        self.reset = Some(value);
        self
    }

    /// Snap to `step` from the range start. Wheel and keys move by it too.
    pub fn step(mut self, step: f32) -> Self {
        self.step = (step > 0.0).then_some(step);
        self
    }

    /// Sets the style.
    pub fn style(mut self, style: DialStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Sets whether it takes input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Shows it as `id` at `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }

    fn drag<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, id: Id, span: f32, travel: f32) {
        let press = ui.press(id, 0);
        if let (Some(reset), Some(press)) = (self.reset, press)
            && press.clicks >= 2
        {
            *self.value = reset;
        }
        let Some(at) = ui.pointer() else {
            return;
        };
        if press.is_none()
            && let Some(last) = ui.animated(id, 0, LAST_Y)
        {
            let fine = if ui.modifiers().shift { FINE } else { 1.0 };
            *self.value += (last - at.y) / travel.max(1.0) * span * fine;
        }
        ui.set_animated(id, 0, LAST_Y, at.y);
    }

    fn keys<Id: WidgetId>(&mut self, ui: &Ui<'_, Id>, span: f32) {
        let step = self.step.unwrap_or(span / 100.0);
        for (key, _) in ui.keys() {
            *self.value += match key {
                KeyCode::Left | KeyCode::Down => -step,
                KeyCode::Right | KeyCode::Up => step,
                KeyCode::Home => f32::NEG_INFINITY,
                KeyCode::End => f32::INFINITY,
                _ => 0.0,
            };
        }
    }
}

impl<Id: WidgetId> Widget<Id> for Dial<'_> {
    fn show(mut self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| DialStyle::new(ui.theme));
        let input = ui.interact(id, 0, rect, Sense::Adjust, self.enabled);
        let mut response = Response::new(rect, input);
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let span = hi - lo;
        let before = *self.value;
        #[cfg(feature = "accessibility")]
        {
            let step = self.step.unwrap_or(span / 100.0);
            ui.accessible_adjust(id, 0, self.value, (lo.min(hi), lo.max(hi)), step);
        }
        if input.held {
            self.drag(ui, id, span, style.travel);
        }
        if input.focused {
            self.keys(ui, span);
        }
        if input.enabled && input.hovered {
            let wheel = ui.take_wheel().1;
            *self.value += wheel * self.step.unwrap_or(span / 100.0);
        }
        *self.value = snap(*self.value, lo, hi, self.step);
        if (*self.value - before).abs() > f32::EPSILON {
            response.changed = true;
            ui.mark_changed();
        }
        #[cfg(feature = "accessibility")]
        {
            let step = self.step.unwrap_or(span / 100.0);
            let range = (lo.min(hi), lo.max(hi));
            ui.accessible_range(id, 0, Role::Slider, *self.value, range, step);
        }

        let share = |value: f32| {
            if span > 0.0 {
                ((value - lo) / span).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let t = share(*self.value);
        let state = ui.eased(id, 0, input);
        let shown = if input.held {
            ui.set_animated(id, 0, Channel::Value, t);
            t
        } else {
            let rate = ui.theme.motion.slide * 1.6;
            ui.animate(id, 0, Channel::Value, t, rate)
        };
        let fade = state.enabled;
        let c = rect.center();
        let radius = (rect.width.min(rect.height) - style.thickness).max(0.0) / 2.0;
        let angle = |t: f32| START + SWEEP * t;
        let stroke = |color: Color| {
            Paint::new()
                .color(color.fade(fade))
                .style(PaintStyle::Stroke)
                .stroke_width(style.thickness)
                .line_cap(LineCap::Round)
        };
        let mut ring = Path::new();
        ring.arc(c, radius, (angle(0.0), angle(1.0)), false);
        ui.ctx.draw_path(&ring, &stroke(style.ring))?;
        let from = self.origin.map_or(0.0, share);
        if (shown - from).abs() > f32::EPSILON {
            let mut filled = Path::new();
            let ends = (angle(from.min(shown)), angle(from.max(shown)));
            filled.arc(c, radius, ends, false);
            ui.ctx.draw_path(&filled, &stroke(style.filled))?;
        }
        let face = (radius - style.thickness * 1.5).max(0.0);
        ui.circle(c, face, style.face.fade(fade))?;
        let (sin, cos) = angle(shown).sin_cos();
        let at = |r: f32| Point::new(c.x + cos * r, c.y + sin * r);
        ui.line(at(face * 0.4), at(face * 0.85), 2.0, style.mark.fade(fade))?;
        let knob = Rect::new(c.x - face, c.y - face, face * 2.0, face * 2.0);
        FocusRing::new(ui.theme, face, state.focus).paint(ui, knob)?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::window::{Modifiers, MouseButton, MouseButtons, PointerKind, WindowEvent};

    fn frame(raster: &mut CpuRasterizer, state: &mut UiState<u8>, value: &mut f32) {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("frame");
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            Dial::new(value, 0.0..=100.0)
                .reset_to(50.0)
                .show(&mut ui, 1, Rect::new(0.0, 0.0, 40.0, 40.0))
                .expect("draw");
        }
        drop(ctx);
        raster.end_frame().expect("end");
    }

    fn button(state: &mut UiState<u8>, y: f64, pressed: bool, clicks: u8) {
        state.handle(&WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed,
            modifiers: Modifiers::default(),
            x: 20.0,
            y,
            click_count: clicks,
            time: Default::default(),
            pointer: PointerKind::Mouse,
        });
    }

    fn moved(state: &mut UiState<u8>, y: f64, shift: bool) {
        state.handle(&WindowEvent::MouseMove {
            x: 20.0,
            y,
            buttons: MouseButtons::from_bits(1),
            modifiers: Modifiers {
                shift,
                ..Modifiers::default()
            },
            pointer: PointerKind::Mouse,
            time: Default::default(),
        });
    }

    fn rig() -> (CpuRasterizer, UiState<u8>, f32) {
        (CpuRasterizer::new(40, 40), UiState::new(), 20.0)
    }

    #[test]
    fn dragging_up_turns_it_up_from_where_it_was() {
        let (mut raster, mut state, mut value) = rig();
        frame(&mut raster, &mut state, &mut value);
        button(&mut state, 20.0, true, 1);
        frame(&mut raster, &mut state, &mut value);
        assert_eq!(value, 20.0, "a press alone keeps the value");
        moved(&mut state, 0.0, false);
        frame(&mut raster, &mut state, &mut value);
        assert!((value - 30.0).abs() < 0.01, "{value}");
        moved(&mut state, -100.0, true);
        frame(&mut raster, &mut state, &mut value);
        assert!((value - 35.0).abs() < 0.01, "fine: {value}");
    }

    #[test]
    fn a_double_click_resets() {
        let (mut raster, mut state, mut value) = rig();
        frame(&mut raster, &mut state, &mut value);
        button(&mut state, 20.0, true, 2);
        frame(&mut raster, &mut state, &mut value);
        assert_eq!(value, 50.0);
    }

    #[test]
    fn the_wheel_steps_and_the_ends_hold() {
        let (mut raster, mut state, mut value) = rig();
        frame(&mut raster, &mut state, &mut value);
        moved(&mut state, 20.0, false);
        state.handle(&WindowEvent::MouseWheel {
            delta_x: 0.0,
            delta_y: 500.0,
            x: 20.0,
            y: 20.0,
            modifiers: Modifiers::default(),
        });
        frame(&mut raster, &mut state, &mut value);
        assert_eq!(value, 100.0);
    }
}
