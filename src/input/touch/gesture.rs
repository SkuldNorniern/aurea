//! Two fingers' gesture math, and the events it sends: scrolls, zooms and turns.

use std::f32::consts::{PI, TAU};

use crate::{GesturePhase, Modifiers, Point, WindowEvent};

use super::Pose;
use super::mouse::mouse_move;

/// How far two fingers must turn, in radians, before it is a twist: a pinch's jitter stays under
/// this.
const TWIST: f32 = PI / 90.0;

pub(super) fn totals(phase: GesturePhase, start: Pose, now: Pose, turn: f32) -> WindowEvent {
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
pub(super) fn wheel(out: &mut Vec<WindowEvent>, was: Point, now: Point) {
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
pub(super) fn gesture(
    out: &mut Vec<WindowEvent>,
    was: Pose,
    now: Pose,
    turn: Option<f32>,
) -> Option<f32> {
    // the pointer goes where the fingers meet, so what scrolls or zooms is what is under them.
    out.push(mouse_move(now.center, false));
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

/// Where fingers at `a` and `b` lie. Two fingers on one spot have no line between them, so they
/// keep the `last` angle.
pub(super) fn pose(a: Point, b: Point, last: f32) -> Pose {
    let span = distance(a, b);
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

/// The turn from `delta` radians, taken the short way round, as a value in `[-PI, PI)`.
pub(super) fn wrap(delta: f32) -> f32 {
    (delta + PI).rem_euclid(TAU) - PI
}

pub(super) fn distance(a: Point, b: Point) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}
