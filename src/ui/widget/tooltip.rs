//! The tooltip asked for this frame: a [`card`] with a [`Label`], drawn over everything as the
//! frame ends.

use std::mem;
use std::time::{Duration, Instant};

use crate::AureaResult;
use crate::render::Rect;
use crate::ui::component::card;
use crate::ui::frame::{Ui, line_pitch};
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

    fn delay_for(&self, long_pressed: bool) -> Duration {
        if long_pressed {
            Duration::ZERO
        } else {
            self.delay
        }
    }

    /// How far it has faded in, `elapsed` after it was first asked for.
    fn shown(&self, long_pressed: bool, elapsed: Duration) -> f32 {
        let Some(into) = elapsed.checked_sub(self.delay_for(long_pressed)) else {
            return 0.0;
        };
        if self.fade.is_zero() {
            return 1.0;
        }
        into.as_secs_f32() / self.fade.as_secs_f32()
    }

    /// When the delay ends and the fade with it, `since` being when it was first asked for.
    fn ends(&self, long_pressed: bool, since: Instant) -> (Instant, Instant) {
        let delay_end = since + self.delay_for(long_pressed);
        (delay_end, delay_end + self.fade)
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
    if shown <= 0.0 {
        // Waiting: sleep until the delay is up, no busy frames.
        let (delay_end, _) = tip.options.ends(tip.long_pressed, since);
        ui.wake_at(delay_end);
        return Ok(());
    }
    if shown < 1.0 {
        // Reduced motion makes `keep_animating` a no-op, so wake for the fade's end too.
        let (_, fade_end) = tip.options.ends(tip.long_pressed, since);
        ui.keep_animating();
        ui.wake_at(fade_end);
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
    let pad = 8.0;
    #[allow(clippy::cast_precision_loss)]
    let canvas = Rect::new(0.0, 0.0, ui.ctx.width() as f32, ui.ctx.height() as f32);
    let widest = ui
        .state
        .tip
        .options
        .widest
        .min(canvas.width - pad * 4.0)
        .max(40.0);
    let probe = Label::new(text, ui.theme.palette.text).small();
    let full = probe.width(ui);
    let overflow = if full <= widest {
        Overflow::Visible
    } else {
        Overflow::Wrap(line_pitch(&ui.fonts().small))
    };
    let label = probe.overflow(overflow);
    let body_h = label.height(ui, widest);
    let (w, h) = (full.min(widest) + pad * 2.0, body_h + pad * 1.2);
    let gap = 6.0;
    let x = (anchor.center().x - w / 2.0).clamp(gap, (canvas.right() - w - gap).max(gap));
    let y = if anchor.bottom() + gap + h <= canvas.bottom() {
        anchor.bottom() + gap
    } else {
        (anchor.y - gap - h).max(gap)
    };
    let rect = Rect::new(x, y, w, h);
    card(ui.theme).fade(alpha).paint(ui, rect)?;
    let body = Rect::new(rect.x + pad, rect.y + pad * 0.6, w - pad * 2.0, body_h);
    Label {
        color: ui.theme.palette.text.fade(alpha),
        ..label
    }
    .paint(ui, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WindowEvent;
    use crate::ui::rig::Rig;
    use crate::ui::{Button, Response};
    use std::thread;

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

    fn quick(rig: &mut Rig) {
        let ms = Duration::from_millis;
        let quick = TooltipOptions::new().delay(ms(40)).fade(ms(40));
        rig.frame(|ui| ui.set_tooltip_options(quick));
    }

    fn sleep_until(at: Instant) {
        if let Some(wait) = at.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
    }

    #[test]
    fn reduced_motion_wakes_at_the_end_of_the_delay_and_shows_the_tip() {
        let mut rig = Rig::new(200, 100).still();
        quick(&mut rig);
        frame(&mut rig);
        rig.at(20.0, 20.0);
        frame(&mut rig);
        let (_, since) = rig.state.tip.since.expect("asked for");
        assert_eq!(
            rig.state.next_frame_at(),
            Some(since + Duration::from_millis(40))
        );
        sleep_until(rig.state.next_frame_at().expect("wake"));
        frame(&mut rig);
        let at = rig.state.next_frame_at().expect("fade ends");
        sleep_until(at);
        frame(&mut rig);
        assert_ne!(
            rig.pixel(50, 47),
            rig.pixel(150, 47),
            "the tip is drawn under the button"
        );
    }

    #[test]
    fn motion_waits_for_the_delay_without_busy_frames() {
        let mut rig: Rig = Rig::new(200, 100);
        rig.state.set_reduced_motion(false);
        let long = Duration::from_secs(10);
        let hovered = Response {
            rect: Rect::new(10.0, 10.0, 80.0, 30.0),
            hovered: true,
            ..Response::default()
        };
        rig.frame(|ui| {
            ui.set_tooltip_options(TooltipOptions::new().delay(long));
            ui.tooltip(&hovered, "Save the file");
        });
        let (_, since) = rig.state.tip.since.expect("asked for");
        assert_eq!(
            rig.state.next_frame_at(),
            Some(since + Duration::from_secs(10))
        );
        assert!(!rig.state.wants_frame(), "no busy frames while waiting");
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
