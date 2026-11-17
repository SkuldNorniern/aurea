//! The finger state machine: what touches amount to, as the mouse, a click, a tap or a gesture.

use std::time::{Duration, Instant};

#[cfg(feature = "gestures")]
use crate::input::gesture::{Pose, gesture, pose, scroll_edge, totals, wheel, wrap};
#[cfg(feature = "gestures")]
use crate::{GesturePhase, ScrollPhase};
use crate::{Point, WindowEvent, press_on_hold};

use super::mouse::{as_pen, long_press, mouse_move, press, release};

/// How far a finger moves, in logical pixels, before it is the mouse rather than a possible
/// gesture.
const SLOP: f32 = 8.0;
/// How long a finger stays still before it is the mouse.
const HOLD: Duration = Duration::from_millis(120);
/// How long a finger stays still before it is a long press.
const LONG_PRESS: Duration = Duration::from_millis(500);
/// How long several fingers stay down at most, from the first landing to the last lift, to be a tap.
const TAP: Duration = Duration::from_millis(250);
/// How far a finger of a tap moves at most, in logical pixels.
const TAP_SLOP: f32 = 10.0;

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
    /// The touch that is the mouse, its button held. `still` is where and since when a finger has
    /// not moved; `None` once it has, and for a pen.
    Pointer {
        id: usize,
        still: Option<(Point, Instant)>,
    },
    /// Two fingers: how they landed and lie, how far they have turned in all, and how far while
    /// still inside the dead zone (`None` once they are turning for real).
    #[cfg(feature = "gestures")]
    Gesture {
        a: usize,
        b: usize,
        start: Pose,
        pose: Pose,
        turned: f32,
        turn: Option<f32>,
    },
    /// The finger left of a gesture, scrolling.
    #[cfg(feature = "gestures")]
    Pan {
        id: usize,
        at: Point,
    },
    /// A long press fired; the press is gone, so its lift sends no release.
    LongPressed {
        id: usize,
    },
    /// A gesture ended; nothing more until every finger lifts.
    Done,
}

fn distance(a: Point, b: Point) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Fingers that may yet be a tap: since when, how many, and where.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tap {
    since: Instant,
    fingers: usize,
    at: Point,
}

/// The touches on the screen and what they amount to.
#[derive(Debug)]
pub(crate) struct Touches {
    down: Vec<(usize, Point)>,
    /// Where the fingers down now landed.
    landed: Vec<(usize, Point)>,
    state: State,
    tap: Option<Tap>,
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
            landed: Vec::new(),
            state: State::Idle,
            tap: None,
        }
    }

    /// Whether a finger is waiting to be told apart, so [`Touches::tick`] should be called soon.
    pub(crate) fn waiting(&self) -> bool {
        matches!(
            self.state,
            State::Pending { .. } | State::Pointer { still: Some(_), .. }
        )
    }

    /// A touch from a mobile host: `action` 0 down, 1 up, 2 move, 3 cancel. `None` for any other.
    pub(crate) fn native(
        &mut self,
        action: i32,
        id: usize,
        at: Point,
        pencil: bool,
        now: Instant,
    ) -> Option<Vec<WindowEvent>> {
        let phase = match action {
            0 => Phase::Down,
            1 => Phase::Up,
            2 => Phase::Move,
            3 => Phase::Cancel,
            _ => return None,
        };
        Some(self.touch(phase, id, at, pencil, now))
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
        self.starting_tap(phase, pencil);
        self.state = match self.state {
            // a pencil landing on a waiting finger: the finger was a palm.
            _ if pencil && phase == Phase::Down && self.takes_pencil() => {
                press(&mut out, at);
                State::Pointer { id, still: None }
            }
            State::Idle if phase == Phase::Down => State::Pending {
                id,
                start: at,
                at,
                since: now,
            },
            State::Pending { .. } => self.pending(&mut out, phase, id, at),
            State::Pointer { id: held, still } if held == id => match phase {
                Phase::Move => {
                    out.push(mouse_move(at, true));
                    State::Pointer {
                        id,
                        still: still.filter(|(from, _)| distance(*from, at) <= SLOP),
                    }
                }
                Phase::Up => {
                    release(&mut out, at);
                    self.rest()
                }
                Phase::Cancel => {
                    out.push(WindowEvent::PressCancelled);
                    out.push(WindowEvent::MouseExited);
                    self.rest()
                }
                Phase::Down => self.state,
            },
            State::LongPressed { id: held } if held == id => self.long_pressed(&mut out, phase),
            #[cfg(feature = "gestures")]
            State::Gesture { .. } => self.gesture(&mut out, phase, id),
            #[cfg(feature = "gestures")]
            State::Pan { .. } => self.pan(&mut out, phase, id, at),
            State::Done if matches!(phase, Phase::Up | Phase::Cancel) => self.rest(),
            state => state,
        };
        self.ending_tap(&mut out, phase, now);
        if pencil {
            as_pen(&mut out);
        }
        out
    }

    /// Whether a pencil landing now draws: nothing is waiting on a finger, or a pan was left behind.
    fn takes_pencil(&self) -> bool {
        match self.state {
            State::Idle | State::Pending { .. } => true,
            #[cfg(feature = "gestures")]
            State::Pan { .. } => true,
            _ => false,
        }
    }

    /// Before the state moves on: a second finger on a waiting one may be a tap, and more may join
    /// it, while a pencil or a gesture coming back from a pan rules one out.
    fn starting_tap(&mut self, phase: Phase, pencil: bool) {
        if phase != Phase::Down {
            return;
        }
        match self.state {
            _ if pencil => self.tap = None,
            State::Pending { start, since, .. } => {
                let at = self.down.last().map_or(start, |(_, at)| *at);
                self.tap = Some(Tap {
                    since,
                    fingers: 2,
                    at: Point::new((start.x + at.x) / 2.0, (start.y + at.y) / 2.0),
                });
            }
            #[cfg(feature = "gestures")]
            State::Pan { .. } => self.tap = None,
            _ => {
                if let Some(tap) = &mut self.tap {
                    tap.fingers = tap.fingers.max(self.down.len());
                }
            }
        }
    }

    /// After the state moved on: the last finger lifting in time is the tap.
    fn ending_tap(&mut self, out: &mut Vec<WindowEvent>, phase: Phase, now: Instant) {
        if phase == Phase::Cancel {
            self.tap = None;
        }
        if phase != Phase::Up || !self.down.is_empty() {
            return;
        }
        if let Some(tap) = self.tap.take()
            && now.duration_since(tap.since) <= TAP
        {
            out.push(WindowEvent::FingerTap {
                fingers: u8::try_from(tap.fingers).unwrap_or(u8::MAX),
                x: f64::from(tap.at.x),
                y: f64::from(tap.at.y),
            });
        }
    }

    /// The finger of a long press: lifting only leaves, as its press is gone.
    fn long_pressed(&self, out: &mut Vec<WindowEvent>, phase: Phase) -> State {
        if matches!(phase, Phase::Up | Phase::Cancel) {
            out.push(WindowEvent::MouseExited);
            self.rest()
        } else {
            self.state
        }
    }

    fn track(&mut self, phase: Phase, id: usize, at: Point) {
        match phase {
            Phase::Down => {
                self.down.retain(|(d, _)| *d != id);
                self.down.push((id, at));
                self.landed.retain(|(d, _)| *d != id);
                self.landed.push((id, at));
            }
            Phase::Move => {
                if let Some(entry) = self.down.iter_mut().find(|(d, _)| *d == id) {
                    entry.1 = at;
                }
                let strayed = self
                    .landed
                    .iter()
                    .any(|(d, from)| *d == id && distance(*from, at) > TAP_SLOP);
                if strayed {
                    self.tap = None;
                }
            }
            Phase::Up | Phase::Cancel => {
                self.down.retain(|(d, _)| *d != id);
                self.landed.retain(|(d, _)| *d != id);
            }
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
            Phase::Down => self.second_finger(out, held, was, id, at),
            _ if held != id => self.state,
            Phase::Move if distance(start, at) > SLOP => {
                press(out, start);
                out.push(mouse_move(at, true));
                State::Pointer { id, still: None }
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

    /// A second finger on a waiting one starts a gesture. Without `gestures` it is ignored.
    #[cfg(feature = "gestures")]
    fn second_finger(
        &self,
        out: &mut Vec<WindowEvent>,
        a: usize,
        at_a: Point,
        b: usize,
        at_b: Point,
    ) -> State {
        begin(out, a, at_a, b, at_b)
    }

    #[cfg(not(feature = "gestures"))]
    fn second_finger(
        &self,
        _out: &mut Vec<WindowEvent>,
        _a: usize,
        _at_a: Point,
        _b: usize,
        _at_b: Point,
    ) -> State {
        self.state
    }

    /// Two fingers: their moves scroll and zoom, and either lifting ends it. If that leaves one
    /// finger it pans on, unless the system took the touch.
    #[cfg(feature = "gestures")]
    fn gesture(&self, out: &mut Vec<WindowEvent>, phase: Phase, id: usize) -> State {
        let State::Gesture {
            a,
            b,
            start,
            pose: was,
            turned,
            turn,
        } = self.state
        else {
            return self.state;
        };
        if id != a && id != b {
            return self.state;
        }
        let left = if turn.is_some() { 0.0 } else { turned };
        match phase {
            Phase::Move => match (self.at(a), self.at(b)) {
                (Some(pa), Some(pb)) => {
                    let now = pose(pa, pb, was.angle);
                    let turned = turned + wrap(now.angle - was.angle);
                    // the pointer goes where the fingers meet, so what scrolls or zooms is what is under them.
                    out.push(mouse_move(now.center, false));
                    let turn = gesture(out, was, now, turn);
                    let shown = if turn.is_some() { 0.0 } else { turned };
                    out.push(totals(GesturePhase::Moved, start, now, shown));
                    State::Gesture {
                        a,
                        b,
                        start,
                        pose: now,
                        turned,
                        turn,
                    }
                }
                _ => State::Done,
            },
            Phase::Up => {
                out.push(totals(GesturePhase::Ended, start, was, left));
                if let [(id, at)] = self.down[..] {
                    return State::Pan { id, at };
                }
                scroll_edge(out, ScrollPhase::Ended, was.center);
                out.push(WindowEvent::MouseExited);
                self.rest()
            }
            Phase::Cancel => {
                out.push(totals(GesturePhase::Cancelled, start, was, left));
                scroll_edge(out, ScrollPhase::Ended, was.center);
                out.push(WindowEvent::MouseExited);
                self.rest()
            }
            Phase::Down => self.state,
        }
    }

    /// The finger left of a gesture: it scrolls, and another landing starts a new gesture.
    #[cfg(feature = "gestures")]
    fn pan(&self, out: &mut Vec<WindowEvent>, phase: Phase, id: usize, at: Point) -> State {
        let State::Pan { id: held, at: was } = self.state else {
            return self.state;
        };
        match phase {
            Phase::Down => begin(out, held, was, id, at),
            _ if held != id => self.state,
            Phase::Move => {
                out.push(mouse_move(at, false));
                wheel(out, was, at);
                State::Pan { id, at }
            }
            Phase::Up | Phase::Cancel => {
                scroll_edge(out, ScrollPhase::Ended, at);
                out.push(WindowEvent::MouseExited);
                self.rest()
            }
        }
    }

    /// A finger that stayed still long enough becomes the mouse.
    pub(crate) fn tick(&mut self, now: Instant) -> Vec<WindowEvent> {
        self.advance(now, press_on_hold())
    }

    fn advance(&mut self, now: Instant, press_on_hold: bool) -> Vec<WindowEvent> {
        let mut out = Vec::new();
        if let State::Pending {
            id, start, since, ..
        } = self.state
            && !press_on_hold
            && now.duration_since(since) >= LONG_PRESS
        {
            out.push(long_press(start));
            self.state = State::LongPressed { id };
        }
        if let State::Pending {
            id,
            start,
            at,
            since,
        } = self.state
            && press_on_hold
            && now.duration_since(since) >= HOLD
        {
            press(&mut out, start);
            if at != start {
                out.push(mouse_move(at, true));
            }
            self.state = State::Pointer {
                id,
                still: Some((start, since)),
            };
        }
        if let State::Pointer {
            id,
            still: Some((from, since)),
        } = self.state
            && now.duration_since(since) >= LONG_PRESS
        {
            out.push(WindowEvent::PressCancelled);
            out.push(long_press(from));
            self.state = State::LongPressed { id };
        }
        out
    }

    #[cfg(feature = "gestures")]
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

/// Fingers `a` at `at_a` and `b` at `at_b` start a gesture.
#[cfg(feature = "gestures")]
fn begin(out: &mut Vec<WindowEvent>, a: usize, at_a: Point, b: usize, at_b: Point) -> State {
    let start = pose(at_a, at_b, 0.0);
    out.push(totals(GesturePhase::Began, start, start, 0.0));
    scroll_edge(out, ScrollPhase::Began, start.center);
    State::Gesture {
        a,
        b,
        start,
        pose: start,
        turned: 0.0,
        turn: Some(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PointerKind, ScrollSource};

    /// What happened, minus a scroll gesture's empty start and end; `scroll_edges` checks those.
    fn kinds(events: &[WindowEvent]) -> Vec<&'static str> {
        events
            .iter()
            .filter(|e| !matches!(e, WindowEvent::Scroll(s) if s.delta_x == 0.0 && s.delta_y == 0.0))
            .map(|e| match e {
                WindowEvent::MouseMove { .. } => "move",
                WindowEvent::MouseButton { pressed: true, .. } => "press",
                WindowEvent::MouseButton { pressed: false, .. } => "release",
                WindowEvent::MouseExited => "exit",
                WindowEvent::Scroll(_) => "wheel",
                WindowEvent::PressCancelled => "press cancelled",
                WindowEvent::LongPress { .. } => "long press",
                WindowEvent::Magnify { .. } => "magnify",
                WindowEvent::Rotate { .. } => "rotate",
                WindowEvent::Gesture { .. } => "gesture",
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
        assert_eq!(kinds(&out), ["press cancelled", "exit"]);
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
    fn a_still_finger_long_presses_once() {
        let now = Instant::now();
        let mut t = Touches::new();
        t.touch(Phase::Down, 1, p(5.0, 5.0), false, now);
        t.tick(now + HOLD);
        assert!(t.waiting(), "frames keep coming until it fires");
        assert!(kinds(&t.tick(now + HOLD * 2)).is_empty());
        let out = t.tick(now + LONG_PRESS);
        assert_eq!(kinds(&out), ["press cancelled", "long press"]);
        assert!(kinds(&t.tick(now + LONG_PRESS * 2)).is_empty());
        assert!(!t.waiting());
        let lift = t.touch(Phase::Up, 1, p(5.0, 5.0), false, now + LONG_PRESS * 2);
        assert_eq!(kinds(&lift), ["exit"], "no release after the cancel");
    }

    #[test]
    fn without_a_press_on_hold_a_long_press_comes_first() {
        let now = Instant::now();
        let mut t = Touches::new();
        t.touch(Phase::Down, 1, p(5.0, 5.0), false, now);
        assert!(t.advance(now + HOLD, false).is_empty());
        assert!(t.waiting());
        let out = t.advance(now + LONG_PRESS, false);
        assert_eq!(kinds(&out), ["long press"], "nothing to cancel");
        let lift = t.touch(Phase::Up, 1, p(5.0, 5.0), false, now + LONG_PRESS);
        assert_eq!(kinds(&lift), ["exit"]);
    }

    #[test]
    #[cfg(feature = "gestures")]
    fn without_a_press_on_hold_a_tap_moving_and_second_finger_still_work() {
        let now = Instant::now();
        let later = now + HOLD * 2;
        let mut t = Touches::new();
        t.touch(Phase::Down, 1, p(5.0, 5.0), false, now);
        t.advance(later, false);
        let out = t.touch(Phase::Up, 1, p(5.0, 5.0), false, later);
        assert_eq!(kinds(&out), ["move", "press", "release", "exit"]);

        t.touch(Phase::Down, 2, p(5.0, 5.0), false, now);
        t.advance(later, false);
        let out = t.touch(Phase::Move, 2, p(50.0, 5.0), false, later);
        assert_eq!(kinds(&out), ["move", "press", "move"]);
        t.touch(Phase::Up, 2, p(50.0, 5.0), false, later);

        t.touch(Phase::Down, 3, p(0.0, 0.0), false, now);
        t.advance(later, false);
        let out = t.touch(Phase::Down, 4, p(100.0, 0.0), false, later);
        assert_eq!(
            kinds(&out),
            ["gesture"],
            "a slow second finger still pinches"
        );
    }

    #[test]
    fn a_cancelled_press_emits_no_button_and_lifting_clicks_nothing() {
        let now = Instant::now();
        let mut t = Touches::new();
        t.touch(Phase::Down, 1, p(5.0, 5.0), false, now);
        assert_eq!(kinds(&t.tick(now + HOLD)), ["move", "press"]);
        let out = t.touch(Phase::Cancel, 1, p(5.0, 5.0), false, now);
        assert_eq!(kinds(&out), ["press cancelled", "exit"]);
        assert!(kinds(&t.touch(Phase::Up, 1, p(5.0, 5.0), false, now)).is_empty());
        assert!(!t.waiting());
    }

    #[test]
    fn moving_cancels_a_long_press() {
        let now = Instant::now();
        let mut t = Touches::new();
        t.touch(Phase::Down, 1, p(5.0, 5.0), false, now);
        t.tick(now + HOLD);
        t.touch(Phase::Move, 1, p(50.0, 5.0), false, now + HOLD);
        assert!(kinds(&t.tick(now + LONG_PRESS)).is_empty());
    }

    #[test]
    fn a_pencil_presses_at_once() {
        let mut t = Touches::new();
        let out = t.touch(Phase::Down, 7, p(5.0, 5.0), true, Instant::now());
        assert_eq!(kinds(&out), ["move", "press"]);
    }

    fn taps(events: &[WindowEvent]) -> Vec<(u8, f64, f64)> {
        events
            .iter()
            .filter_map(|e| match *e {
                WindowEvent::FingerTap { fingers, x, y } => Some((fingers, x, y)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn one_finger_clicks_rather_than_taps() {
        let now = Instant::now();
        let mut t = Touches::new();
        t.touch(Phase::Down, 1, p(10.0, 10.0), false, now);
        let out = t.touch(Phase::Up, 1, p(10.0, 10.0), false, now);
        assert!(taps(&out).is_empty());
        assert_eq!(kinds(&out), ["move", "press", "release", "exit"]);
    }

    /// Everything below needs two fingers to gesture: `two_fingers` and what it drives.
    #[cfg(feature = "gestures")]
    mod gestures {
        use super::*;
        use std::f32::consts::PI;

        /// Two fingers down 100 apart, at y 0.
        fn two_fingers(now: Instant) -> Touches {
            let mut t = Touches::new();
            t.touch(Phase::Down, 1, p(0.0, 0.0), false, now);
            assert_eq!(
                kinds(&t.touch(Phase::Down, 2, p(100.0, 0.0), false, now)),
                ["gesture"]
            );
            t
        }

        /// The last [`WindowEvent::Gesture`] in `events`: phase, center, moved, scale and turn.
        fn last_gesture(
            events: &[WindowEvent],
        ) -> (GesturePhase, (f64, f64), (f64, f64), f64, f64) {
            events
                .iter()
                .rev()
                .find_map(|e| match *e {
                    WindowEvent::Gesture {
                        phase,
                        x,
                        y,
                        dx,
                        dy,
                        scale,
                        turn,
                    } => Some((phase, (x, y), (dx, dy), scale, turn)),
                    _ => None,
                })
                .expect("a gesture event")
        }

        #[test]
        fn gesture_totals_count_from_the_landing() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            t.touch(Phase::Move, 1, p(-50.0, 0.0), false, now);
            let out = t.touch(Phase::Move, 2, p(150.0, 0.0), false, now);
            let (phase, _, _, scale, turn) = last_gesture(&out);
            assert_eq!(phase, GesturePhase::Moved);
            assert_eq!(scale, 2.0);
            assert_eq!(turn, 0.0);
            // on from there, not from the last reading.
            t.touch(Phase::Move, 1, p(-100.0, 20.0), false, now);
            let out = t.touch(Phase::Move, 2, p(200.0, 20.0), false, now);
            let (_, center, moved, scale, _) = last_gesture(&out);
            assert_eq!(scale, 3.0);
            assert_eq!(center, (50.0, 20.0));
            assert_eq!(moved, (0.0, 20.0));
        }

        #[test]
        fn gesture_turn_adds_up_past_a_half_turn() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            let out = t.touch(Phase::Move, 2, turned(1.0), false, now);
            assert_eq!(last_gesture(&out).4, 0.0, "jitter");
            for deg in [90.0, 170.0, 250.0] {
                t.touch(Phase::Move, 2, turned(deg), false, now);
            }
            let out = t.touch(Phase::Move, 2, turned(300.0), false, now);
            let total = last_gesture(&out).4;
            assert!((total - 300f64.to_radians()).abs() < 1e-3, "{total}");
        }

        #[test]
        fn a_cancelled_gesture_says_so() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            let out = t.touch(Phase::Cancel, 1, p(0.0, 0.0), false, now);
            assert_eq!(last_gesture(&out).0, GesturePhase::Cancelled);
            assert_eq!(kinds(&out), ["gesture", "exit"]);
            assert!(
                t.touch(Phase::Move, 2, p(160.0, 0.0), false, now)
                    .is_empty(),
                "the system took it, so the other finger is dead"
            );
        }

        fn scroll_edges(events: &[WindowEvent]) -> Vec<ScrollPhase> {
            events
                .iter()
                .filter_map(|e| match e {
                    WindowEvent::Scroll(s) => {
                        assert_eq!(s.source, ScrollSource::Touch);
                        Some(s.phase)
                    }
                    _ => None,
                })
                .collect()
        }

        #[test]
        fn a_finger_scroll_begins_moves_and_ends() {
            let now = Instant::now();
            let mut t = Touches::new();
            t.touch(Phase::Down, 1, p(0.0, 0.0), false, now);
            let down = t.touch(Phase::Down, 2, p(100.0, 0.0), false, now);
            assert_eq!(scroll_edges(&down), [ScrollPhase::Began]);
            t.touch(Phase::Move, 1, p(0.0, 10.0), false, now);
            let moved = t.touch(Phase::Move, 2, p(100.0, 10.0), false, now);
            assert_eq!(scroll_edges(&moved), [ScrollPhase::Moved]);
            t.touch(Phase::Up, 1, p(0.0, 10.0), false, now);
            let up = t.touch(Phase::Up, 2, p(100.0, 10.0), false, now);
            assert_eq!(scroll_edges(&up), [ScrollPhase::Ended]);
        }

        #[test]
        fn two_fingers_moving_together_scroll() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            // both move down 10: a scroll that follows them.
            t.touch(Phase::Move, 1, p(0.0, 10.0), false, now);
            let out = t.touch(Phase::Move, 2, p(100.0, 10.0), false, now);
            assert_eq!(kinds(&out)[..2], ["move", "wheel"]);
            assert!(matches!(out[1], WindowEvent::Scroll(s) if s.delta_y == 5.0));
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
        fn the_finger_left_of_a_gesture_pans() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            let out = t.touch(Phase::Up, 1, p(0.0, 0.0), false, now);
            assert_eq!(kinds(&out), ["gesture"]);
            assert_eq!(last_gesture(&out).0, GesturePhase::Ended);
            let out = t.touch(Phase::Move, 2, p(100.0, 30.0), false, now);
            assert_eq!(kinds(&out), ["move", "wheel"]);
            assert!(matches!(out[1], WindowEvent::Scroll(s) if s.delta_y == 30.0));
            assert_eq!(
                kinds(&t.touch(Phase::Up, 2, p(100.0, 30.0), false, now)),
                ["exit"]
            );
            assert!(t.touch(Phase::Down, 3, p(1.0, 1.0), false, now).is_empty());
            assert!(t.waiting());
        }

        #[test]
        fn two_fingers_lifting_soon_tap_where_they_met() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            let out = t.touch(Phase::Up, 1, p(0.0, 0.0), false, now);
            assert!(taps(&out).is_empty(), "one is still down");
            let out = t.touch(Phase::Up, 2, p(100.0, 0.0), false, now + TAP);
            assert_eq!(taps(&out), [(2, 50.0, 0.0)]);
            assert_eq!(kinds(&out), ["exit", "other"]);
        }

        #[test]
        fn three_fingers_tap_as_three() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            t.touch(Phase::Down, 3, p(50.0, 50.0), false, now);
            t.touch(Phase::Up, 1, p(0.0, 0.0), false, now);
            t.touch(Phase::Up, 3, p(50.0, 50.0), false, now);
            let out = t.touch(Phase::Up, 2, p(100.0, 0.0), false, now);
            assert_eq!(taps(&out), [(3, 50.0, 0.0)]);
        }

        #[test]
        fn a_slow_or_moved_or_cancelled_lift_is_no_tap() {
            let now = Instant::now();
            let mut slow = two_fingers(now);
            slow.touch(Phase::Up, 1, p(0.0, 0.0), false, now);
            let late = now + TAP + Duration::from_millis(1);
            assert!(taps(&slow.touch(Phase::Up, 2, p(100.0, 0.0), false, late)).is_empty());

            let mut moved = two_fingers(now);
            moved.touch(Phase::Move, 2, p(120.0, 0.0), false, now);
            moved.touch(Phase::Move, 2, p(100.0, 0.0), false, now);
            moved.touch(Phase::Up, 1, p(0.0, 0.0), false, now);
            assert!(taps(&moved.touch(Phase::Up, 2, p(100.0, 0.0), false, now)).is_empty());

            let mut taken = two_fingers(now);
            taken.touch(Phase::Cancel, 1, p(0.0, 0.0), false, now);
            assert!(taps(&taken.touch(Phase::Up, 2, p(100.0, 0.0), false, now)).is_empty());
        }

        #[test]
        fn a_finger_landing_on_a_pan_starts_a_new_gesture() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            t.touch(Phase::Up, 1, p(0.0, 0.0), false, now);
            t.touch(Phase::Move, 2, p(100.0, 30.0), false, now);
            let out = t.touch(Phase::Down, 3, p(0.0, 30.0), false, now);
            assert_eq!(last_gesture(&out).0, GesturePhase::Began);
            t.touch(Phase::Move, 3, p(-100.0, 30.0), false, now);
            let out = t.touch(Phase::Move, 2, p(100.0, 30.0), false, now);
            assert_eq!(last_gesture(&out).3, 2.0, "from the new landing");
        }

        #[test]
        fn a_pencil_landing_on_a_pan_draws() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            t.touch(Phase::Up, 1, p(0.0, 0.0), false, now);
            let out = t.touch(Phase::Down, 3, p(5.0, 5.0), true, now);
            assert_eq!(kinds(&out), ["move", "press"]);
        }

        #[test]
        fn a_third_finger_lifting_leaves_the_gesture_alone() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            t.touch(Phase::Down, 3, p(50.0, 50.0), false, now);
            assert!(t.touch(Phase::Up, 3, p(50.0, 50.0), false, now).is_empty());
            let out = t.touch(Phase::Move, 2, p(150.0, 0.0), false, now);
            assert_eq!(last_gesture(&out).3, 1.5);
        }

        #[test]
        fn a_gesture_with_two_fingers_left_ends_until_they_lift() {
            let now = Instant::now();
            let mut t = two_fingers(now);
            t.touch(Phase::Down, 3, p(50.0, 50.0), false, now);
            assert_eq!(
                kinds(&t.touch(Phase::Up, 1, p(0.0, 0.0), false, now)),
                ["gesture", "exit"]
            );
            assert!(t.touch(Phase::Move, 2, p(9.0, 9.0), false, now).is_empty());
            assert!(t.touch(Phase::Up, 2, p(9.0, 9.0), false, now).is_empty());
            assert!(t.touch(Phase::Up, 3, p(9.0, 9.0), false, now).is_empty());
            assert!(t.touch(Phase::Down, 4, p(1.0, 1.0), false, now).is_empty());
            assert!(t.waiting());
        }
    }
}
