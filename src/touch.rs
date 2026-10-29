//! Turns touches on a phone or tablet screen into the mouse, wheel and pinch events a desktop
//! window sends, so the same UI code runs on both.
//!
//! A pencil is the mouse at once. A finger waits a moment first: it becomes the mouse when it moves
//! or stays down, a click when it lifts, and the start of a two-finger gesture when a second finger
//! lands. Two fingers moving together scroll as [`WindowEvent::MouseWheel`], spreading or closing
//! zoom as [`WindowEvent::Magnify`], and turning about each other as [`WindowEvent::Rotate`].

use std::f32::consts::{PI, TAU};
use std::time::{Duration, Instant};

use crate::{Modifiers, MouseButton, MouseButtons, Point, PointerKind, WindowEvent};

/// How far a finger moves, in logical pixels, before it is the mouse rather than a possible
/// gesture.
const SLOP: f32 = 8.0;
/// How long a finger stays still before it is the mouse.
const HOLD: Duration = Duration::from_millis(120);
/// How far two fingers must turn, in radians, before it is a twist: a pinch's jitter stays under
/// this.
const TWIST: f32 = PI / 90.0;

/// What a touch did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Down,
    Move,
    Up,
    /// The system took the touch, for its own gesture.
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    /// One finger down that is not the mouse yet.
    Pending {
        id: usize,
        start: Point,
        at: Point,
        since: Instant,
    },
    /// The touch that is the mouse, its button held.
    Pointer {
        id: usize,
    },
    /// Two fingers: how they lie, and how far they have turned while still inside the dead zone
    /// (`None` once they are turning for real).
    Gesture {
        a: usize,
        b: usize,
        pose: Pose,
        turn: Option<f32>,
    },
    /// A gesture ended; nothing more until every finger lifts.
    Done,
}

/// Where two fingers meet, how far apart they are, and the angle of the line from one to the other.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    center: Point,
    span: f32,
    angle: f32,
}

/// The touches on the screen and what they amount to.
#[derive(Debug)]
pub(crate) struct Touches {
    down: Vec<(usize, Point)>,
    state: State,
}

impl Default for Touches {
    fn default() -> Self {
        Self::new()
    }
}

impl Touches {
    pub(crate) fn new() -> Self {
        Self {
            down: Vec::new(),
            state: State::Idle,
        }
    }

    /// Whether a finger is waiting to be told apart, so [`Touches::tick`] should be called soon.
    pub(crate) fn waiting(&self) -> bool {
        matches!(self.state, State::Pending { .. })
    }

    /// The events for touch `id` at `at`, in logical pixels.
    pub(crate) fn touch(
        &mut self,
        phase: Phase,
        id: usize,
        at: Point,
        pencil: bool,
        now: Instant,
    ) -> Vec<WindowEvent> {
        self.track(phase, id, at);
        let mut out = Vec::new();
        self.state = match self.state {
            // a pencil landing on a waiting finger: the finger was a palm.
            State::Idle | State::Pending { .. } if pencil && phase == Phase::Down => {
                press(&mut out, at);
                State::Pointer { id }
            }
            State::Idle if phase == Phase::Down => State::Pending {
                id,
                start: at,
                at,
                since: now,
            },
            State::Pending { .. } => self.pending(&mut out, phase, id, at),
            State::Pointer { id: held } if held == id => match phase {
                Phase::Move => {
                    out.push(mouse_move(at, true));
                    self.state
                }
                Phase::Up | Phase::Cancel => {
                    release(&mut out, at);
                    self.rest()
                }
                Phase::Down => self.state,
            },
            State::Gesture { .. } => self.gesture(&mut out, phase, id),
            State::Done if matches!(phase, Phase::Up | Phase::Cancel) => self.rest(),
            state => state,
        };
        if pencil {
            as_pen(&mut out);
        }
        out
    }

    fn track(&mut self, phase: Phase, id: usize, at: Point) {
        match phase {
            Phase::Down => {
                self.down.retain(|(d, _)| *d != id);
                self.down.push((id, at));
            }
            Phase::Move => {
                if let Some(entry) = self.down.iter_mut().find(|(d, _)| *d == id) {
                    entry.1 = at;
                }
            }
            Phase::Up | Phase::Cancel => self.down.retain(|(d, _)| *d != id),
        }
    }

    /// A finger was waiting: a second one starts a gesture, moving far makes it the mouse, and
    /// lifting is a click.
    fn pending(&self, out: &mut Vec<WindowEvent>, phase: Phase, id: usize, at: Point) -> State {
        let State::Pending {
            id: held,
            start,
            at: was,
            since,
        } = self.state
        else {
            return self.state;
        };
        match phase {
            Phase::Down => State::Gesture {
                a: held,
                b: id,
                pose: pose(was, at, 0.0),
                turn: Some(0.0),
            },
            _ if held != id => self.state,
            Phase::Move if distance(start, at) > SLOP => {
                press(out, start);
                out.push(mouse_move(at, true));
                State::Pointer { id }
            }
            Phase::Move => State::Pending {
                id,
                start,
                at,
                since,
            },
            Phase::Up => {
                press(out, start);
                release(out, at);
                State::Idle
            }
            Phase::Cancel => State::Idle,
        }
    }

    /// Two fingers: their moves scroll and zoom, and either lifting ends it.
    fn gesture(&self, out: &mut Vec<WindowEvent>, phase: Phase, id: usize) -> State {
        let State::Gesture {
            a,
            b,
            pose: was,
            turn,
        } = self.state
        else {
            return self.state;
        };
        if id != a && id != b {
            return self.state;
        }
        match phase {
            Phase::Move => match (self.at(a), self.at(b)) {
                (Some(pa), Some(pb)) => {
                    let now = pose(pa, pb, was.angle);
                    let turn = gesture(out, was, now, turn);
                    State::Gesture {
                        a,
                        b,
                        pose: now,
                        turn,
                    }
                }
                _ => State::Done,
            },
            Phase::Up | Phase::Cancel => {
                out.push(WindowEvent::MouseExited);
                self.rest()
            }
            Phase::Down => self.state,
        }
    }

    /// A finger that stayed still long enough becomes the mouse.
    pub(crate) fn tick(&mut self, now: Instant) -> Vec<WindowEvent> {
        let mut out = Vec::new();
        if let State::Pending {
            id,
            start,
            at,
            since,
        } = self.state
            && now.duration_since(since) >= HOLD
        {
            press(&mut out, start);
            if at != start {
                out.push(mouse_move(at, true));
            }
            self.state = State::Pointer { id };
        }
        out
    }

    fn at(&self, id: usize) -> Option<Point> {
        self.down.iter().find(|(d, _)| *d == id).map(|(_, at)| *at)
    }

    /// After a touch ends: wait for the rest to lift, if any are down.
    fn rest(&self) -> State {
        if self.down.is_empty() {
            State::Idle
        } else {
            State::Done
        }
    }
}

/// Emits what two fingers moved to `now` from `was` amount to, and returns the twist still inside
/// the dead zone.
fn gesture(out: &mut Vec<WindowEvent>, was: Pose, now: Pose, turn: Option<f32>) -> Option<f32> {
    // the pointer goes where the fingers meet, so what scrolls or zooms is what is under them.
    out.push(mouse_move(now.center, false));
    let (dx, dy) = (now.center.x - was.center.x, now.center.y - was.center.y);
    if dx != 0.0 || dy != 0.0 {
        // natural scrolling, the way a trackpad reports it: content follows the fingers.
        out.push(WindowEvent::MouseWheel {
            delta_x: f64::from(-dx),
            delta_y: f64::from(dy),
            modifiers: Modifiers::default(),
        });
    }
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

fn press(out: &mut Vec<WindowEvent>, at: Point) {
    out.push(mouse_move(at, false));
    out.push(button(at, true));
}

fn release(out: &mut Vec<WindowEvent>, at: Point) {
    out.push(button(at, false));
    out.push(WindowEvent::MouseExited);
}

/// Marks the mouse events in `events` as a pencil's, which [`mouse_move`] and [`button`] make as a
/// finger's.
fn as_pen(events: &mut [WindowEvent]) {
    for event in events {
        if let WindowEvent::MouseMove { pointer, .. } | WindowEvent::MouseButton { pointer, .. } =
            event
        {
            *pointer = PointerKind::Pen;
        }
    }
}

fn mouse_move(at: Point, held: bool) -> WindowEvent {
    let buttons = if held {
        MouseButtons::from_bits(1 << MouseButton::Left.to_raw())
    } else {
        MouseButtons::default()
    };
    WindowEvent::MouseMove {
        x: f64::from(at.x),
        y: f64::from(at.y),
        buttons,
        modifiers: Modifiers::default(),
        time: Duration::default(),
        pointer: PointerKind::Touch,
    }
}

fn button(at: Point, pressed: bool) -> WindowEvent {
    WindowEvent::MouseButton {
        button: MouseButton::Left,
        pressed,
        modifiers: Modifiers::default(),
        x: f64::from(at.x),
        y: f64::from(at.y),
        click_count: 1,
        time: Duration::default(),
        pointer: PointerKind::Touch,
    }
}

/// Where fingers at `a` and `b` lie. Two fingers on one spot have no line between them, so they
/// keep the `last` angle.
fn pose(a: Point, b: Point, last: f32) -> Pose {
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
fn wrap(delta: f32) -> f32 {
    (delta + PI).rem_euclid(TAU) - PI
}

fn distance(a: Point, b: Point) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(events: &[WindowEvent]) -> Vec<&'static str> {
        events
            .iter()
            .map(|e| match e {
                WindowEvent::MouseMove { .. } => "move",
                WindowEvent::MouseButton { pressed: true, .. } => "press",
                WindowEvent::MouseButton { pressed: false, .. } => "release",
                WindowEvent::MouseExited => "exit",
                WindowEvent::MouseWheel { .. } => "wheel",
                WindowEvent::Magnify { .. } => "magnify",
                WindowEvent::Rotate { .. } => "rotate",
                _ => "other",
            })
            .collect()
    }

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn a_tap_clicks_where_it_landed() {
        let mut t = Touches::new();
        let now = Instant::now();
        assert!(
            t.touch(Phase::Down, 1, p(10.0, 10.0), false, now)
                .is_empty()
        );
        assert!(t.waiting());
        let out = t.touch(Phase::Up, 1, p(11.0, 10.0), false, now);
        assert_eq!(kinds(&out), ["move", "press", "release", "exit"]);
        assert!(!t.waiting());
    }

    #[test]
    fn a_finger_that_moves_drags_from_its_start() {
        let mut t = Touches::new();
        let now = Instant::now();
        t.touch(Phase::Down, 1, p(10.0, 10.0), false, now);
        assert!(
            t.touch(Phase::Move, 1, p(12.0, 10.0), false, now)
                .is_empty()
        );
        let out = t.touch(Phase::Move, 1, p(30.0, 10.0), false, now);
        assert_eq!(kinds(&out), ["move", "press", "move"]);
        assert!(matches!(out[1], WindowEvent::MouseButton { x, .. } if x == 10.0));
        let out = t.touch(Phase::Move, 1, p(40.0, 10.0), false, now);
        assert_eq!(kinds(&out), ["move"]);
        let out = t.touch(Phase::Up, 1, p(40.0, 10.0), false, now);
        assert_eq!(kinds(&out), ["release", "exit"]);
    }

    #[test]
    fn a_finger_held_still_becomes_the_mouse() {
        let mut t = Touches::new();
        let now = Instant::now();
        t.touch(Phase::Down, 1, p(10.0, 10.0), false, now);
        assert!(t.tick(now + HOLD / 2).is_empty());
        assert_eq!(kinds(&t.tick(now + HOLD)), ["move", "press"]);
        assert!(!t.waiting());
    }

    #[test]
    fn a_cancelled_touch_lets_go() {
        let mut t = Touches::new();
        let now = Instant::now();
        t.touch(Phase::Down, 1, p(10.0, 10.0), false, now);
        assert!(
            t.touch(Phase::Cancel, 1, p(10.0, 10.0), false, now)
                .is_empty()
        );
        t.touch(Phase::Down, 2, p(5.0, 5.0), true, now);
        let out = t.touch(Phase::Cancel, 2, p(5.0, 5.0), true, now);
        assert_eq!(kinds(&out), ["release", "exit"]);
    }

    #[test]
    fn fingers_and_pencils_say_what_they_are() {
        let now = Instant::now();
        let pointers = |events: Vec<WindowEvent>| -> Vec<PointerKind> {
            events
                .iter()
                .filter_map(|e| match e {
                    WindowEvent::MouseMove { pointer, .. }
                    | WindowEvent::MouseButton { pointer, .. } => Some(*pointer),
                    _ => None,
                })
                .collect()
        };
        let mut pencil = Touches::new();
        let down = pencil.touch(Phase::Down, 1, p(5.0, 5.0), true, now);
        assert_eq!(pointers(down), [PointerKind::Pen; 2]);
        let mut finger = Touches::new();
        finger.touch(Phase::Down, 1, p(5.0, 5.0), false, now);
        let up = finger.touch(Phase::Up, 1, p(5.0, 5.0), false, now);
        assert!(!up.is_empty());
        assert!(pointers(up).iter().all(|&k| k == PointerKind::Touch));
    }

    #[test]
    fn a_pencil_presses_at_once() {
        let mut t = Touches::new();
        let out = t.touch(Phase::Down, 7, p(5.0, 5.0), true, Instant::now());
        assert_eq!(kinds(&out), ["move", "press"]);
    }

    /// Two fingers down 100 apart, at y 0.
    fn two_fingers(now: Instant) -> Touches {
        let mut t = Touches::new();
        t.touch(Phase::Down, 1, p(0.0, 0.0), false, now);
        assert!(
            t.touch(Phase::Down, 2, p(100.0, 0.0), false, now)
                .is_empty()
        );
        t
    }

    #[test]
    fn two_fingers_moving_together_scroll() {
        let now = Instant::now();
        let mut t = two_fingers(now);
        // both move down 10: a scroll that follows them.
        t.touch(Phase::Move, 1, p(0.0, 10.0), false, now);
        let out = t.touch(Phase::Move, 2, p(100.0, 10.0), false, now);
        assert_eq!(kinds(&out)[..2], ["move", "wheel"]);
        assert!(matches!(out[1], WindowEvent::MouseWheel { delta_y, .. } if delta_y == 5.0));
    }

    #[test]
    fn two_fingers_spreading_magnify_without_pressing() {
        let now = Instant::now();
        let mut t = two_fingers(now);
        t.touch(Phase::Move, 1, p(-50.0, 0.0), false, now);
        let out = t.touch(Phase::Move, 2, p(150.0, 0.0), false, now);
        let factor = out.iter().find_map(|e| match e {
            WindowEvent::Magnify { factor, .. } => Some(*factor),
            _ => None,
        });
        assert!(factor.is_some_and(|f| f > 1.0));
        assert!(!kinds(&out).contains(&"press"));
    }

    /// Where the second of [`two_fingers`] is when turned `deg` degrees clockwise on screen, its
    /// first finger still at the origin.
    fn turned(deg: f32) -> Point {
        let r = deg.to_radians();
        p(100.0 * r.cos(), 100.0 * r.sin())
    }

    fn rotations(events: &[WindowEvent]) -> Vec<(f64, f64, f64)> {
        events
            .iter()
            .filter_map(|e| match *e {
                WindowEvent::Rotate { radians, x, y } => Some((radians, x, y)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn two_fingers_twisting_rotate_clockwise_about_their_middle() {
        let now = Instant::now();
        let mut t = two_fingers(now);
        let out = t.touch(Phase::Move, 2, turned(30.0), false, now);
        let [(radians, x, y)] = rotations(&out)[..] else {
            panic!("one rotation: {out:?}");
        };
        assert!((radians - f64::from(PI / 6.0)).abs() < 1e-4, "{radians}");
        // the fingers now lie at (0, 0) and (86.6, 50): their middle.
        assert!(
            (x - 43.3).abs() < 0.01 && (y - 25.0).abs() < 0.01,
            "{x} {y}"
        );
        // the next move turns on from where the last left off.
        let out = t.touch(Phase::Move, 2, turned(45.0), false, now);
        let [(radians, ..)] = rotations(&out)[..] else {
            panic!("one rotation: {out:?}");
        };
        assert!((radians - f64::from(PI / 12.0)).abs() < 1e-4, "{radians}");
    }

    #[test]
    fn a_twist_across_the_back_turns_the_short_way() {
        let now = Instant::now();
        let mut t = two_fingers(now);
        t.touch(Phase::Move, 2, turned(170.0), false, now);
        // from 170 degrees to -170 is 20 degrees on, through 180, not 340 back.
        let out = t.touch(Phase::Move, 2, turned(-170.0), false, now);
        let [(radians, ..)] = rotations(&out)[..] else {
            panic!("one rotation: {out:?}");
        };
        assert!((radians - f64::from(PI / 9.0)).abs() < 1e-4, "{radians}");
    }

    #[test]
    fn a_pinch_does_not_turn() {
        let now = Instant::now();
        let mut t = two_fingers(now);
        t.touch(Phase::Move, 1, p(-50.0, 0.0), false, now);
        let out = t.touch(Phase::Move, 2, p(150.0, 0.0), false, now);
        assert!(out.iter().any(|e| matches!(e, WindowEvent::Magnify { .. })));
        assert!(rotations(&out).is_empty());
    }

    #[test]
    fn a_small_twist_waits_for_the_dead_zone_to_pass() {
        let now = Instant::now();
        let mut t = two_fingers(now);
        let out = t.touch(Phase::Move, 2, turned(1.0), false, now);
        assert!(rotations(&out).is_empty(), "jitter");
        // past 2 degrees in all, the whole turn comes through at once.
        let out = t.touch(Phase::Move, 2, turned(2.5), false, now);
        let [(radians, ..)] = rotations(&out)[..] else {
            panic!("one rotation: {out:?}");
        };
        assert!(
            (radians - f64::from((2.5f32).to_radians())).abs() < 1e-4,
            "{radians}"
        );
    }

    #[test]
    fn a_pure_twist_does_not_spread_or_close() {
        let now = Instant::now();
        let mut t = two_fingers(now);
        let out = t.touch(Phase::Move, 2, turned(30.0), false, now);
        assert!(!rotations(&out).is_empty());
        for e in &out {
            if let WindowEvent::Magnify { factor, .. } = e {
                assert!((factor - 1.0).abs() < 1e-4, "{factor}");
            }
        }
    }

    #[test]
    fn a_gesture_ends_when_a_finger_lifts() {
        let now = Instant::now();
        let mut t = two_fingers(now);
        assert_eq!(
            kinds(&t.touch(Phase::Up, 1, p(0.0, 0.0), false, now)),
            ["exit"]
        );
        // the other does nothing until it lifts too.
        assert!(
            t.touch(Phase::Move, 2, p(160.0, 0.0), false, now)
                .is_empty()
        );
        assert!(t.touch(Phase::Up, 2, p(160.0, 0.0), false, now).is_empty());
        assert!(t.touch(Phase::Down, 3, p(1.0, 1.0), false, now).is_empty());
        assert!(t.waiting());
    }
}
