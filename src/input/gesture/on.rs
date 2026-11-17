//! Two fingers' gesture math, and the events it sends: scrolls, zooms and turns.

use std::f32::consts::{PI, TAU};
use std::os::raw::c_void;

use crate::window::push_window_event;
use crate::{GesturePhase, Modifiers, Point, WindowEvent};

/// How far two fingers must turn, in radians, before it is a twist: a pinch's jitter stays under
/// this.
const TWIST: f32 = PI / 90.0;

const GESTURE_MAGNIFY: u32 = 0;
const GESTURE_ROTATE: u32 = 1;

/// Where two fingers meet, how far apart they are, and the angle of the line from one to the other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pose {
    pub(crate) center: Point,
    pub(crate) span: f32,
    pub(crate) angle: f32,
}

/// Where fingers at `a` and `b` lie. Two fingers on one spot have no line between them, so they
/// keep the `last` angle.
pub(crate) fn pose(a: Point, b: Point, last: f32) -> Pose {
    let span = (a.x - b.x).hypot(a.y - b.y);
    Pose {
        center: Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
        span,
        angle: if span > 0.0 {
            (b.y - a.y).atan2(b.x - a.x)
        } else {
            last
        },
    }
}

pub(crate) fn totals(phase: GesturePhase, start: Pose, now: Pose, turn: f32) -> WindowEvent {
    let scale = if start.span > 0.0 {
        now.span / start.span
    } else {
        1.0
    };
    WindowEvent::Gesture {
        phase,
        x: f64::from(now.center.x),
        y: f64::from(now.center.y),
        dx: f64::from(now.center.x - start.center.x),
        dy: f64::from(now.center.y - start.center.y),
        scale: f64::from(scale),
        turn: f64::from(turn),
    }
}

/// Scrolls by a move from `was` to `now`, if it went anywhere.
pub(crate) fn wheel(out: &mut Vec<WindowEvent>, was: Point, now: Point) {
    let (dx, dy) = (now.x - was.x, now.y - was.y);
    if dx != 0.0 || dy != 0.0 {
        // natural scrolling, the way a trackpad reports it: content follows the fingers.
        out.push(WindowEvent::MouseWheel {
            delta_x: f64::from(-dx),
            delta_y: f64::from(dy),
            x: f64::from(now.x),
            y: f64::from(now.y),
            modifiers: Modifiers::default(),
        });
    }
}

/// Emits what two fingers moved to `now` from `was` amount to, and returns the twist still inside
/// the dead zone.
pub(crate) fn gesture(
    out: &mut Vec<WindowEvent>,
    was: Pose,
    now: Pose,
    turn: Option<f32>,
) -> Option<f32> {
    wheel(out, was.center, now.center);
    if was.span > 0.0 && now.span > 0.0 && (now.span - was.span).abs() > f32::EPSILON {
        out.push(WindowEvent::Magnify {
            factor: f64::from(now.span / was.span),
            x: f64::from(now.center.x),
            y: f64::from(now.center.y),
        });
    }
    let twist = wrap(now.angle - was.angle);
    match turn {
        None if twist != 0.0 => {
            out.push(rotate(twist, now.center));
            None
        }
        None => None,
        Some(total) if (total + twist).abs() > TWIST => {
            out.push(rotate(total + twist, now.center));
            None
        }
        Some(total) => Some(total + twist),
    }
}

fn rotate(radians: f32, at: Point) -> WindowEvent {
    WindowEvent::Rotate {
        radians: f64::from(radians),
        x: f64::from(at.x),
        y: f64::from(at.y),
    }
}

/// The turn from `delta` radians, taken the short way round, as a value in `[-PI, PI)`.
pub(crate) fn wrap(delta: f32) -> f32 {
    (delta + PI).rem_euclid(TAU) - PI
}

/// The event for a native pinch (`kind` magnify, `value` a factor) or twist (`kind` rotate, `value`
/// radians, clockwise) about (`x`, `y`); `None` for a value that is not one.
fn native_event(kind: u32, value: f64, x: f64, y: f64) -> Option<WindowEvent> {
    match kind {
        GESTURE_MAGNIFY if value.is_finite() && value > 0.0 => Some(WindowEvent::Magnify {
            factor: value,
            x,
            y,
        }),
        GESTURE_ROTATE if value.is_finite() => Some(WindowEvent::Rotate {
            radians: value,
            x,
            y,
        }),
        _ => None,
    }
}

/// Queues a native pinch or twist for `window`.
pub(crate) fn deliver(window: *mut c_void, kind: u32, value: f64, x: f64, y: f64) {
    if let Some(event) = native_event(kind, value, x, y) {
        push_window_event(window, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pinch_keeps_its_factor_and_spot() {
        assert!(matches!(
            native_event(GESTURE_MAGNIFY, 1.5, 3.0, 4.0),
            Some(WindowEvent::Magnify { factor, x, y }) if factor == 1.5 && x == 3.0 && y == 4.0
        ));
        for factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(native_event(GESTURE_MAGNIFY, factor, 0.0, 0.0).is_none());
        }
    }

    #[test]
    fn a_twist_keeps_its_sign_and_unknown_kinds_are_dropped() {
        assert!(matches!(
            native_event(GESTURE_ROTATE, -0.25, 3.0, 4.0),
            Some(WindowEvent::Rotate { radians, x, y }) if radians == -0.25 && x == 3.0 && y == 4.0
        ));
        assert!(native_event(GESTURE_ROTATE, f64::NAN, 0.0, 0.0).is_none());
        assert!(native_event(7, 1.0, 0.0, 0.0).is_none());
    }
}
