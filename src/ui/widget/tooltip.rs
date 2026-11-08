//! The tooltip asked for this frame: a [`card`] with a [`Label`], drawn over everything as the
//! frame ends.

use std::mem;
use std::time::{Duration, Instant};

use crate::AureaResult;
use crate::render::Rect;
use crate::ui::component::card;
use crate::ui::frame::Ui;
use crate::ui::paint::{Component, Label, Overflow};
use crate::ui::state::WidgetId;

/// How tooltips behave, for the whole UI; set with [`Ui::set_tooltip_options`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TooltipOptions {
    delay: Duration,
    fade: Duration,
    widest: f32,
    linger: Duration,
}

impl Default for TooltipOptions {
    fn default() -> Self {
        Self {
            delay: Duration::from_millis(450),
            fade: Duration::from_millis(120),
            widest: 320.0,
            linger: Duration::from_millis(1500),
        }
    }
}

impl TooltipOptions {
    /// The default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// How long the pointer rests before the tip shows. A long press shows it at once.
    #[must_use]
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// How long it takes to fade in.
    #[must_use]
    pub fn fade(mut self, fade: Duration) -> Self {
        self.fade = fade;
        self
    }

    /// How wide it goes before its text wraps.
    #[must_use]
    pub fn widest(mut self, widest: f32) -> Self {
        self.widest = widest.max(40.0);
        self
    }

    /// How long a long press's tip stays after the finger lifts.
    #[must_use]
    pub fn linger(mut self, linger: Duration) -> Self {
        self.linger = linger;
        self
    }

    /// How far it has faded in, `elapsed` after it was first asked for.
    fn shown(&self, long_pressed: bool, elapsed: Duration) -> f32 {
        let delay = if long_pressed {
            Duration::ZERO
        } else {
            self.delay
        };
        let Some(into) = elapsed.checked_sub(delay) else {
            return 0.0;
        };
        if self.fade.is_zero() {
            return 1.0;
        }
        into.as_secs_f32() / self.fade.as_secs_f32()
    }
}

pub(crate) fn draw<Id: WidgetId>(ui: &mut Ui<'_, Id>) -> AureaResult<()> {
    let Some(anchor) = anchor_now(ui) else {
        ui.state.tip.since = None;
        return Ok(());
    };
    let since = match ui.state.tip.since {
        Some((rect, since)) if rect == anchor => since,
        _ => {
            let now = Instant::now();
            ui.state.tip.since = Some((anchor, now));
            now
        }
    };
    let tip = &ui.state.tip;
    let shown = tip.options.shown(tip.long_pressed, since.elapsed());
    if shown < 1.0 {
        ui.keep_animating();
    }
    if shown <= 0.0 {
        return Ok(());
    }
    let text = mem::take(&mut ui.state.tip.text);
    let result = place(ui, anchor, &text, shown.min(1.0));
    ui.state.tip.text = text;
    result
}

/// Where the tip goes now: where it was asked for, or, after a long press lifted, where it was.
fn anchor_now<Id: WidgetId>(ui: &mut Ui<'_, Id>) -> Option<Rect> {
    let popup = ui.state.popup.is_open();
    let tip = &mut ui.state.tip;
    if !tip.long_pressed || popup {
        tip.linger = None;
        return tip.anchor;
    }
    let now = Instant::now();
    if let Some(anchor) = tip.anchor {
        tip.linger = Some((anchor, now + tip.options.linger));
        return Some(anchor);
    }
    match tip.linger {
        Some((anchor, until)) if now < until => {
            ui.wake_at(until);
            Some(anchor)
        }
        _ => {
            tip.linger = None;
            tip.long_pressed = false;
            None
        }
    }
}

fn place<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    anchor: Rect,
    text: &str,
    alpha: f32,
) -> AureaResult<()> {
    let fonts = ui.fonts();
    let font = &fonts.small;
    let pad = 8.0;
    let line = font.size * 1.45;
    #[allow(clippy::cast_precision_loss)]
    let canvas = Rect::new(0.0, 0.0, ui.ctx.width() as f32, ui.ctx.height() as f32);
    let widest = ui
        .state
        .tip
        .options
        .widest
        .min(canvas.width - pad * 4.0)
        .max(40.0);
    let full = ui.text_width(text, font);
    let lines = if full <= widest {
        1.0
    } else {
        (full / (widest * 0.9)).ceil()
    };
    let (w, h) = (full.min(widest) + pad * 2.0, line * lines + pad * 1.2);
    let gap = 6.0;
    let x = (anchor.center().x - w / 2.0).clamp(gap, (canvas.right() - w - gap).max(gap));
    let y = if anchor.bottom() + gap + h <= canvas.bottom() {
        anchor.bottom() + gap
    } else {
        (anchor.y - gap - h).max(gap)
    };
    let rect = Rect::new(x, y, w, h);
    card(ui.theme).fade(alpha).paint(ui, rect)?;
    let body = Rect::new(
        rect.x + pad,
        rect.y + pad * 0.6,
        w - pad * 2.0,
        line * lines,
    );
    let color = ui.theme.palette.text.fade(alpha);
    let overflow = if lines > 1.0 {
        Overflow::Wrap(line)
    } else {
        Overflow::Visible
    };
    Label::new(text, color)
        .small()
        .overflow(overflow)
        .paint(ui, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WindowEvent;
    use crate::ui::rig::Rig;
    use crate::ui::{Button, Response};

    fn frame(rig: &mut Rig) -> Response {
        rig.frame(|ui| {
            let button = Button::new("Save")
                .show(ui, 1, Rect::new(10.0, 10.0, 80.0, 30.0))
                .expect("button");
            ui.tooltip(&button, "Save the file");
            button
        })
    }

    fn long_press(rig: &mut Rig) {
        rig.send(WindowEvent::PressCancelled);
        rig.send(WindowEvent::LongPress { x: 20.0, y: 20.0 });
    }

    #[test]
    fn a_long_press_shows_the_tip_and_it_outlasts_the_finger() {
        let mut rig = Rig::new(200, 100).still();
        frame(&mut rig);
        long_press(&mut rig);
        frame(&mut rig);
        assert!(rig.state.tip.anchor.is_some());
        assert!(rig.state.tip.linger.is_some());
        rig.send(WindowEvent::MouseExited);
        frame(&mut rig);
        assert!(rig.state.tip.anchor.is_none(), "not hovered any more");
        assert!(rig.state.tip.since.is_some(), "still showing");
        assert!(rig.state.next_frame_at().is_some(), "and wakes to leave");
    }

    #[test]
    fn the_next_press_dismisses_a_lingering_tip() {
        let mut rig = Rig::new(200, 100).still();
        frame(&mut rig);
        long_press(&mut rig);
        frame(&mut rig);
        rig.send(WindowEvent::MouseExited);
        frame(&mut rig);
        rig.button(150.0, 80.0, true);
        frame(&mut rig);
        assert!(rig.state.tip.since.is_none());
    }

    #[test]
    fn it_leaves_when_its_time_is_up() {
        let mut rig = Rig::new(200, 100).still();
        frame(&mut rig);
        long_press(&mut rig);
        frame(&mut rig);
        rig.send(WindowEvent::MouseExited);
        rig.state.tip.linger = Some((Rect::default(), Instant::now()));
        frame(&mut rig);
        assert!(rig.state.tip.since.is_none());
        assert!(!rig.state.tip.long_pressed);
    }

    #[test]
    fn options_set_the_delay_and_fade() {
        let ms = Duration::from_millis;
        let quick = TooltipOptions::new().delay(ms(100)).fade(ms(100));
        assert_eq!(quick.shown(false, ms(50)), 0.0);
        assert!((quick.shown(false, ms(150)) - 0.5).abs() < 1e-3);
        assert!(
            (quick.shown(true, ms(50)) - 0.5).abs() < 1e-3,
            "a long press skips the delay"
        );
        let instant = TooltipOptions::new().delay(ms(0)).fade(ms(0));
        assert_eq!(instant.shown(false, ms(0)), 1.0);

        let mut rig: Rig = Rig::new(200, 100).still();
        rig.frame(|ui| ui.set_tooltip_options(quick));
        assert_eq!(rig.state.tip.options, quick, "it stays set");
    }

    #[test]
    fn a_mouse_still_waits_for_the_delay() {
        let mut rig = Rig::new(200, 100).still();
        frame(&mut rig);
        rig.drag_to(20.0, 20.0);
        frame(&mut rig);
        assert!(!rig.state.tip.long_pressed);
        assert!(rig.state.tip.linger.is_none());
    }
}
