//! A dial: a knob that turns with drag, wheel and keys.

use std::f32::consts::PI;
use std::ops::RangeInclusive;

use super::Widget;
use super::look::Look;
use super::slider::snap;
use crate::AureaResult;
use crate::render::{Color, LineCap, Paint, PaintStyle, Path, Point, Rect};
use crate::ui::accessibility::Role;
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label};
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

// ring starts lower left.
const START: f32 = 0.75 * PI;
const SWEEP: f32 = 1.5 * PI;
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

/// What a [`Dial`] draws, from [`Dial::look`].
#[derive(Clone, Copy, Debug)]
pub struct DialParts<'v> {
    /// Where the value sits over the range, 0 at the start to 1 at the end, eased as it turns.
    pub position: f32,
    /// Where the fill starts, as a share, when an origin is set.
    pub origin: Option<f32>,
    /// The angle the ring starts at, in radians.
    pub start: f32,
    /// How far the ring sweeps over the whole range, in radians.
    pub sweep: f32,
    /// The area the ring is drawn in, centred on the dial.
    pub ring: Rect,
    /// The readout text, if any.
    pub readout: Option<&'v str>,
    /// The line the readout is in, if any.
    pub readout_rect: Option<Rect>,
    /// The caption text, if any.
    pub caption: Option<&'v str>,
    /// The line the caption is in, if any.
    pub caption_rect: Option<Rect>,
    /// Whether it takes input; the default look fades when not.
    pub enabled: bool,
    /// The style it draws with, the theme's unless one was given.
    pub style: DialStyle,
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
    readout: Option<&'v str>,
    caption: Option<&'v str>,
    fine: f32,
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
            readout: None,
            caption: None,
            fine: 0.1,
        }
    }

    /// Shows `text` in a line under the ring, and sizes the ring in what is left.
    pub fn readout(mut self, text: &'v str) -> Self {
        self.readout = Some(text);
        self
    }

    /// Shows `text` in a line under the ring and any readout, for what the dial is: name and value
    /// on a mixer strip.
    pub fn caption(mut self, text: &'v str) -> Self {
        self.caption = Some(text);
        self
    }

    /// Fill from `origin`, not the start.
    pub fn origin(mut self, origin: f32) -> Self {
        self.origin = Some(origin);
        self
    }

    /// How much slower a drag with Shift held moves it, as a share of a plain drag; 0.1 by default.
    pub fn fine(mut self, share: f32) -> Self {
        self.fine = share.max(0.0);
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
            let fine = if ui.modifiers().shift { self.fine } else { 1.0 };
            *self.value += (last - at.y) / travel.max(1.0) * span * fine;
        }
        ui.set_animated(id, 0, LAST_Y, at.y);
    }

    /// What is left, and the line a caption takes at the bottom.
    fn split_caption(&self, rect: Rect, line: f32) -> (Rect, Option<Rect>) {
        match self.caption {
            Some(_) => {
                let (text, rest) = rect.split_bottom(line, 0.0);
                (rest, Some(text))
            }
            None => (rect, None),
        }
    }

    /// The ring area and, with a readout, the line its text takes under it.
    fn split_readout(&self, rect: Rect, line: f32) -> (Rect, Option<Rect>) {
        match self.readout {
            Some(_) => {
                let (text, ring) = rect.split_bottom(line, 0.0);
                (ring, Some(text))
            }
            None => (rect, None),
        }
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

impl<'v> Dial<'v> {
    /// Takes the input as [`Dial::show`] does and works out what it would draw, without drawing:
    /// the app draws it, or [`Look::paint_default`] does, or its pieces [`Look::paint_ring`],
    /// [`Look::paint_face`], [`Look::paint_readout`] and [`Look::paint_caption`].
    pub fn look<Id: WidgetId>(
        mut self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<DialParts<'v>> {
        let style = self.style.unwrap_or_else(|| DialStyle::new(ui.theme));
        let (rest, caption) = self.split_caption(rect, ui.theme.text_size * 1.4);
        let (area, line) = self.split_readout(rest, ui.theme.text_size * 1.4);
        let input = ui.interact(id, 0, area, Sense::Adjust, self.enabled);
        let mut response = Response::new(rect, input);
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let span = hi - lo;
        let before = *self.value;
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
            let step = self.step.unwrap_or(span / 100.0);
            if step.is_finite() && step > 0.0 {
                let (low, high) = (lo.min(hi), lo.max(hi));
                let steps =
                    ui.take_wheel_steps((high - *self.value) / step, (*self.value - low) / step);
                *self.value += steps as f32 * step;
            }
        }
        *self.value = snap(*self.value, lo, hi, self.step);
        if (*self.value - before).abs() > f32::EPSILON {
            response.changed = true;
            ui.mark_changed();
        }
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
        Look {
            rect,
            state,
            held_at: ui.held_at(id, 0),
            response,
            parts: DialParts {
                position: shown,
                origin: self.origin.map(share),
                start: START,
                sweep: SWEEP,
                ring: area,
                readout: self.readout,
                readout_rect: line,
                caption: self.caption,
                caption_rect: caption,
                enabled: self.enabled,
                style,
            },
        }
    }
}

impl Look<DialParts<'_>> {
    /// The ring, the face, then the readout and the caption.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_ring(ui)?;
        self.paint_face(ui)?;
        self.paint_readout(ui)?;
        self.paint_caption(ui)
    }

    /// The angle at `t` of the way round the range, in radians.
    pub fn angle(&self, t: f32) -> f32 {
        self.parts.start + self.parts.sweep * t
    }

    /// The radius of the ring's centre line.
    pub fn radius(&self) -> f32 {
        let ring = self.parts.ring;
        (ring.width.min(ring.height) - self.parts.style.thickness).max(0.0) / 2.0
    }

    /// The radius of the face inside the ring.
    pub fn face_radius(&self) -> f32 {
        (self.radius() - self.parts.style.thickness * 1.5).max(0.0)
    }

    /// The empty ring, and the arc from the origin to the value filled over it.
    pub fn paint_ring<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (fade, style) = (self.state.enabled, &self.parts.style);
        let c = self.parts.ring.center();
        let radius = self.radius();
        let stroke = |color: Color| {
            Paint::new()
                .color(color.fade(fade))
                .style(PaintStyle::Stroke)
                .stroke_width(style.thickness)
                .line_cap(LineCap::Round)
        };
        let mut ring = Path::new();
        ring.arc(c, radius, (self.angle(0.0), self.angle(1.0)), false);
        ui.ctx.draw_path(&ring, &stroke(style.ring))?;
        let shown = self.parts.position;
        let from = self.parts.origin.unwrap_or(0.0);
        if (shown - from).abs() > f32::EPSILON {
            let mut filled = Path::new();
            let ends = (self.angle(from.min(shown)), self.angle(from.max(shown)));
            filled.arc(c, radius, ends, false);
            ui.ctx.draw_path(&filled, &stroke(style.filled))?;
        }
        Ok(())
    }

    /// The knob inside the ring, its value mark and its focus ring.
    pub fn paint_face<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style) = (self.state, &self.parts.style);
        let fade = state.enabled;
        let c = self.parts.ring.center();
        let face = self.face_radius();
        ui.circle(c, face, style.face.fade(fade))?;
        let (sin, cos) = self.angle(self.parts.position).sin_cos();
        let at = |r: f32| Point::new(c.x + cos * r, c.y + sin * r);
        ui.line(at(face * 0.4), at(face * 0.85), 2.0, style.mark.fade(fade))?;
        let knob = Rect::new(c.x - face, c.y - face, face * 2.0, face * 2.0);
        FocusRing::new(ui.theme, face, state.focus).paint(ui, knob)
    }

    /// The readout text, in its line under the ring.
    pub fn paint_readout<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        match (self.parts.readout, self.parts.readout_rect) {
            (Some(text), Some(line)) => Label::new(text, ui.theme.palette.dim)
                .small()
                .centered()
                .paint(ui, line),
            _ => Ok(()),
        }
    }

    /// The caption text, in the line at the bottom.
    pub fn paint_caption<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        match (self.parts.caption, self.parts.caption_rect) {
            (Some(text), Some(line)) => Label::new(text, ui.theme.palette.text)
                .small()
                .centered()
                .paint(ui, line),
            _ => Ok(()),
        }
    }
}

impl<Id: WidgetId> Widget<Id> for Dial<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let look = self.look(ui, id, rect);
        look.paint_default(ui)?;
        Ok(look.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::ui::rig::Rig;
    use crate::window::{
        Modifiers, MouseButton, MouseButtons, PointerKind, ScrollEvent, WindowEvent,
    };

    fn frame(raster: &mut CpuRasterizer, state: &mut UiState<u8>, value: &mut f32) {
        frame_fine(raster, state, value, 0.1);
    }

    fn frame_fine(raster: &mut CpuRasterizer, state: &mut UiState<u8>, value: &mut f32, fine: f32) {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("frame");
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            Dial::new(value, 0.0..=100.0)
                .reset_to(50.0)
                .fine(fine)
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
    fn fine_sets_how_slow_shift_goes() {
        let (mut raster, mut state, mut value) = rig();
        frame_fine(&mut raster, &mut state, &mut value, 0.5);
        button(&mut state, 20.0, true, 1);
        frame_fine(&mut raster, &mut state, &mut value, 0.5);
        moved(&mut state, -80.0, true);
        frame_fine(&mut raster, &mut state, &mut value, 0.5);
        assert!((value - 45.0).abs() < 0.01, "{value}");
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
    fn a_readout_takes_a_line_the_ring_does_not_reach() {
        let mut raster = CpuRasterizer::new(40, 60);
        let mut state = UiState::new();
        let mut value = 20.0;
        let mut draw = |state: &mut UiState<u8>, value: &mut f32| {
            let theme = Theme::dark();
            let mut ctx = raster.begin_frame().expect("frame");
            {
                let mut ui = state.frame(&mut *ctx, &theme);
                Dial::new(value, 0.0..=100.0)
                    .readout("20")
                    .show(&mut ui, 1, Rect::new(0.0, 0.0, 40.0, 60.0))
                    .expect("draw");
            }
            drop(ctx);
            raster.end_frame().expect("end");
        };
        draw(&mut state, &mut value);
        button(&mut state, 59.0, true, 1);
        draw(&mut state, &mut value);
        moved(&mut state, 0.0, false);
        draw(&mut state, &mut value);
        button(&mut state, 0.0, false, 1);
        assert_eq!(value, 20.0, "the text line is not the dial");

        button(&mut state, 5.0, true, 1);
        draw(&mut state, &mut value);
        moved(&mut state, 0.0, false);
        draw(&mut state, &mut value);
        assert!((value - 22.5).abs() < 0.01, "the ring is: {value}");
    }

    #[test]
    fn the_wheel_steps_and_the_ends_hold() {
        let (mut raster, mut state, mut value) = rig();
        frame(&mut raster, &mut state, &mut value);
        moved(&mut state, 20.0, false);
        state.handle(&WindowEvent::Scroll(ScrollEvent {
            x: 20.0,
            y: 20.0,
            ..ScrollEvent::wheel(0.0, 500.0, 16.0, Modifiers::default())
        }));
        frame(&mut raster, &mut state, &mut value);
        assert_eq!(value, 100.0);
    }

    #[test]
    fn look_draws_the_apps_look_and_still_turns() {
        let mut rig: Rig = Rig::new(40, 40);
        let at = Rect::new(0.0, 0.0, 40.0, 40.0);
        let mut value = 20.0;
        let draw = |rig: &mut Rig, value: &mut f32| {
            rig.frame(|ui| {
                let dial = Dial::new(value, 0.0..=100.0).look(ui, 1, at);
                ui.fill(dial.rect, Color::rgb(0, 200, 0)).expect("draw");
            });
        };
        draw(&mut rig, &mut value);
        assert_eq!(rig.pixel(20, 20), [0, 200, 0, 255], "the app's look");
        rig.at(20.0, 20.0);
        draw(&mut rig, &mut value);
        rig.button(20.0, 20.0, true);
        draw(&mut rig, &mut value);
        rig.drag_to(20.0, 0.0);
        draw(&mut rig, &mut value);
        assert!(
            (value - 30.0).abs() < 0.01,
            "the drag still turns it: {value}"
        );
    }
}
