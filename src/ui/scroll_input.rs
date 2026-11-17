//! A frame's scroll input: wheel clicks and smooth pixels kept apart, so a scroller moves by its
//! own step a click and by the pixels a trackpad gives, and a control that steps goes once a click.

use crate::window::{ScrollEvent, ScrollPhase, ScrollSource};

/// What scrolled this frame and no widget used yet.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ScrollInput {
    /// Wheel detents, right and up positive.
    pub(crate) clicks: (f32, f32),
    /// Pixels from trackpads and fingers, right and up positive.
    pub(crate) smooth: (f32, f32),
    /// A trackpad or fingers moved: already smooth, so no easing on top.
    pub(crate) fingers: bool,
    /// The system is carrying a flick on.
    pub(crate) momentum: bool,
}

impl ScrollInput {
    /// Pixels to move along both axes when a click is `step` pixels.
    pub(crate) fn pixels(&self, step: (f32, f32)) -> (f32, f32) {
        (
            self.clicks.0 * step.0 + self.smooth.0,
            self.clicks.1 * step.1 + self.smooth.1,
        )
    }

    /// Leaves the part of each axis not `used`, as fractions of what there was.
    pub(crate) fn consume(&mut self, used: (f32, f32)) {
        fn rest(used: f32) -> f32 {
            if used.is_finite() {
                1.0 - used.clamp(0.0, 1.0)
            } else {
                1.0
            }
        }
        let (kx, ky) = (rest(used.0), rest(used.1));
        self.clicks = (self.clicks.0 * kx, self.clicks.1 * ky);
        self.smooth = (self.smooth.0 * kx, self.smooth.1 * ky);
    }

    /// Takes `steps` off the vertical scroll, `line` pixels a step: clicks first, then pixels.
    /// Subtracts rather than scales, so whole clicks stay whole.
    pub(crate) fn take_steps_y(&mut self, steps: f32, line: f32) {
        let from_clicks = if steps.signum() == self.clicks.1.signum() {
            steps.abs().min(self.clicks.1.abs()).copysign(steps)
        } else {
            0.0
        };
        self.clicks.1 -= from_clicks;
        self.smooth.1 -= (steps - from_clicks) * line;
    }

    /// Adds `event`, minus the axis a gesture is locked off.
    pub(crate) fn add(&mut self, event: &ScrollEvent, lock: &mut AxisLock) {
        let (mut dx, mut dy) = (to_f32(event.delta_x), to_f32(event.delta_y));
        match lock.follow(event, dx, dy) {
            Some(Axis::X) => dy = 0.0,
            Some(Axis::Y) => dx = 0.0,
            None => {}
        }
        if let Some((cx, cy)) = event.clicks {
            let (cx, cy) = (to_f32(cx), to_f32(cy));
            // a locked gesture keeps to its axis, clicks too
            self.clicks.0 += if dx == 0.0 { 0.0 } else { cx };
            self.clicks.1 += if dy == 0.0 { 0.0 } else { cy };
        } else {
            self.smooth.0 += dx;
            self.smooth.1 += dy;
        }
        self.fingers |= event.source != ScrollSource::Wheel;
        self.momentum |= event.is_momentum();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    X,
    Y,
}

/// A trackpad or finger gesture keeps to the axis its first real move went along, so a swipe that
/// is mostly down does not drift sideways. Wheels never lock.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AxisLock {
    axis: Option<Axis>,
    live: bool,
}

/// How much more one axis must move than the other to pick it.
const LOCK_RATIO: f32 = 1.5;

impl AxisLock {
    fn follow(&mut self, event: &ScrollEvent, dx: f32, dy: f32) -> Option<Axis> {
        match event.phase {
            ScrollPhase::Discrete => {
                *self = Self::default();
                return None;
            }
            ScrollPhase::Began => {
                *self = Self {
                    axis: None,
                    live: true,
                };
            }
            _ => {}
        }
        if self.live && self.axis.is_none() {
            if dy.abs() > dx.abs() * LOCK_RATIO {
                self.axis = Some(Axis::Y);
            } else if dx.abs() > dy.abs() * LOCK_RATIO {
                self.axis = Some(Axis::X);
            }
        }
        let axis = self.axis;
        if matches!(event.phase, ScrollPhase::MomentumEnded)
            || (event.phase == ScrollPhase::Ended && event.source == ScrollSource::Touch)
        {
            *self = Self::default();
        }
        axis
    }
}

/// Whole steps out of clicks and pixels, keeping the fraction for the next event: a trackpad
/// moves a stepper as surely as a wheel, and a fine wheel steps once its eighths add up.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StepCarry(f32);

impl StepCarry {
    /// Steps up (positive) or down out of `clicks` detents and `pixels`, `line` pixels a step.
    pub(crate) fn take(&mut self, clicks: f32, pixels: f32, line: f32) -> i32 {
        let add = clicks + if line > 0.0 { pixels / line } else { 0.0 };
        if add == 0.0 {
            return 0;
        }
        if add.signum() != self.0.signum() {
            self.0 = 0.0;
        }
        self.0 += add;
        let whole = self.0.trunc();
        self.0 -= whole;
        whole_steps(whole)
    }

    pub(crate) fn reset(&mut self) {
        self.0 = 0.0;
    }
}

// Scroll amounts are small; f32 keeps them.
#[allow(clippy::cast_possible_truncation)]
fn to_f32(value: f64) -> f32 {
    value as f32
}

// `whole` is a truncated step count far inside i32.
#[allow(clippy::cast_possible_truncation)]
fn whole_steps(whole: f32) -> i32 {
    whole as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::Modifiers;

    fn trackpad(phase: ScrollPhase, dx: f64, dy: f64) -> ScrollEvent {
        ScrollEvent {
            delta_x: dx,
            delta_y: dy,
            clicks: None,
            source: ScrollSource::Trackpad,
            phase,
            ..ScrollEvent::wheel(0.0, 0.0, 0.0, Modifiers::default())
        }
    }

    #[test]
    fn clicks_and_pixels_stay_apart_and_scale_by_the_step() {
        let mut input = ScrollInput::default();
        let mut lock = AxisLock::default();
        input.add(&ScrollEvent::wheel(0.0, 2.0, 16.0, Modifiers::default()), &mut lock);
        input.add(&trackpad(ScrollPhase::Discrete, 0.0, 5.0), &mut lock);
        assert_eq!(input.clicks, (0.0, 2.0));
        assert_eq!(input.smooth, (0.0, 5.0));
        assert_eq!(input.pixels((10.0, 20.0)), (0.0, 45.0));
        assert!(input.fingers);
    }

    #[test]
    fn consuming_a_fraction_leaves_the_rest_of_both_parts() {
        let mut input = ScrollInput {
            clicks: (2.0, 4.0),
            smooth: (10.0, 8.0),
            ..ScrollInput::default()
        };
        input.consume((0.5, 1.0));
        assert_eq!(input.clicks, (1.0, 0.0));
        assert_eq!(input.smooth, (5.0, 0.0));
        input.consume((f32::NAN, 0.0));
        assert_eq!(input.clicks, (1.0, 0.0), "nan uses nothing");
    }

    #[test]
    fn a_mostly_vertical_swipe_keeps_to_y_until_it_ends() {
        let mut input = ScrollInput::default();
        let mut lock = AxisLock::default();
        input.add(&trackpad(ScrollPhase::Began, 0.0, 0.0), &mut lock);
        input.add(&trackpad(ScrollPhase::Moved, 1.0, 6.0), &mut lock);
        input.add(&trackpad(ScrollPhase::Moved, 4.0, 1.0), &mut lock);
        input.add(&trackpad(ScrollPhase::Momentum, 3.0, 2.0), &mut lock);
        assert_eq!(input.smooth, (0.0, 9.0));
        assert!(input.momentum);
        input.add(&trackpad(ScrollPhase::MomentumEnded, 0.0, 0.0), &mut lock);
        input.add(&trackpad(ScrollPhase::Began, 0.0, 0.0), &mut lock);
        input.add(&trackpad(ScrollPhase::Moved, 5.0, 0.0), &mut lock);
        assert_eq!(input.smooth, (5.0, 9.0), "a new gesture picks again");
    }

    #[test]
    fn a_diagonal_start_waits_for_a_clear_axis() {
        let mut input = ScrollInput::default();
        let mut lock = AxisLock::default();
        input.add(&trackpad(ScrollPhase::Began, 0.0, 0.0), &mut lock);
        input.add(&trackpad(ScrollPhase::Moved, 2.0, 2.0), &mut lock);
        assert_eq!(input.smooth, (2.0, 2.0));
        input.add(&trackpad(ScrollPhase::Moved, 0.0, 5.0), &mut lock);
        input.add(&trackpad(ScrollPhase::Moved, 3.0, 0.0), &mut lock);
        assert_eq!(input.smooth, (2.0, 7.0));
    }

    #[test]
    fn wheels_never_lock() {
        let mut input = ScrollInput::default();
        let mut lock = AxisLock::default();
        input.add(&ScrollEvent::wheel(1.0, 1.0, 16.0, Modifiers::default()), &mut lock);
        assert_eq!(input.clicks, (1.0, 1.0));
    }

    #[test]
    fn steps_carry_their_fraction_and_reset_on_a_turn() {
        let mut carry = StepCarry::default();
        assert_eq!(carry.take(1.0, 0.0, 16.0), 1);
        assert_eq!(carry.take(0.0, 10.0, 16.0), 0);
        assert_eq!(carry.take(0.0, 10.0, 16.0), 1, "20 of 32 pixels: one step, a bit kept");
        assert_eq!(carry.take(0.0, -4.0, 16.0), 0, "turning back drops the kept bit");
        assert_eq!(carry.take(-0.75, 0.0, 16.0), -1);
        assert_eq!(carry.take(0.125, 0.0, 0.0), 0, "eighths of a fine wheel add up");
    }
}
