//! Values that ease towards targets, one per key, for UI state such as
//! hover highlights, selections and panels sliding in.
//!
//! Call [`Tweens::frame`] once at the start of each drawn frame, then read
//! each value with [`Tweens::to`]: it moves towards the target by how much
//! time passed. Values not read in a frame are forgotten. While anything is
//! still moving, [`Tweens::busy`] is true and the caller should ask for
//! another frame.

use std::collections::HashMap;
use std::hash::Hash;
use std::mem::take;
use std::time::Instant;

/// How close counts as arrived, relative to the target's size.
const SNAP: f32 = 0.002;
/// Longest step one frame takes, so a frame after a quiet spell does not
/// jump straight to the end.
const MAX_STEP: f32 = 1.0 / 30.0;

#[derive(Debug)]
struct Value {
    now: f32,
    used: bool,
}

#[derive(Debug)]
pub struct Tweens<K> {
    values: HashMap<K, Value>,
    start: Instant,
    last: Option<Instant>,
    dt: f32,
    busy: bool,
    instant: bool,
}

impl<K> Default for Tweens<K> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            start: Instant::now(),
            last: None,
            dt: 0.0,
            busy: false,
            instant: false,
        }
    }
}

impl<K: Eq + Hash + Copy> Tweens<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Values jump straight to their targets, for people who asked the
    /// system for reduced motion.
    pub fn set_instant(&mut self, instant: bool) {
        self.instant = instant;
    }

    /// Starts a frame: measures the time since the last one and forgets
    /// values the last frame did not read.
    pub fn frame(&mut self) {
        let now = Instant::now();
        self.dt = self
            .last
            .map_or(0.0, |l| now.duration_since(l).as_secs_f32().min(MAX_STEP));
        self.last = Some(now);
        self.values.retain(|_, v| take(&mut v.used));
        self.busy = false;
    }

    /// Seconds since these tweens were made, for looping effects.
    pub fn time(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    /// Whether anything is still moving and wants another frame.
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// Marks this frame as wanting a next one, for effects driven by
    /// [`Tweens::time`] rather than a value.
    pub fn keep_busy(&mut self) {
        if !self.instant {
            self.busy = true;
        }
    }

    /// `key` moved towards `target` at `rate` (per second, exponential). A
    /// value seen for the first time starts at its target.
    pub fn to(&mut self, key: K, target: f32, rate: f32) -> f32 {
        self.towards(key, target, rate, rate)
    }

    /// Like [`Tweens::to`], with one rate going up and another going down.
    pub fn towards(&mut self, key: K, target: f32, up: f32, down: f32) -> f32 {
        let dt = self.dt;
        let instant = self.instant;
        let v = self.values.entry(key).or_insert(Value {
            now: target,
            used: true,
        });
        v.used = true;
        if instant {
            v.now = target;
            return target;
        }
        let rate = if target > v.now { up } else { down };
        v.now += (target - v.now) * (1.0 - (-rate * dt).exp());
        if (target - v.now).abs() < SNAP * target.abs().max(1.0) {
            v.now = target;
        } else {
            self.busy = true;
        }
        v.now
    }

    /// Like [`Tweens::to`], but a value seen for the first time starts at
    /// `start` and eases in from there instead of starting at its target.
    pub fn from_to(&mut self, key: K, start: f32, target: f32, rate: f32) -> f32 {
        self.values.entry(key).or_insert(Value {
            now: start,
            used: true,
        });
        self.to(key, target, rate)
    }

    /// Where `key` is now, without reading it for this frame: a value only
    /// looked at here is still forgotten at the next [`Tweens::frame`].
    pub fn get(&self, key: K) -> Option<f32> {
        self.values.get(&key).map(|v| v.now)
    }

    /// Puts `key` at `value` now; it moves from there next time it is read.
    pub fn set(&mut self, key: K, value: f32) {
        self.values.insert(
            key,
            Value {
                now: value,
                used: true,
            },
        );
        if !self.instant {
            self.busy = true;
        }
    }
}

/// Smoothstep: eases 0..1 in and out.
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_ease_to_their_target_and_settle() {
        let mut t = Tweens::new();
        t.frame();
        assert_eq!(t.to("bar", 0.0, 14.0), 0.0);
        t.dt = 0.016;
        let first = t.to("bar", 2.0, 14.0);
        assert!(first > 0.0 && first < 2.0);
        assert!(t.busy());
        for _ in 0..200 {
            t.busy = false;
            t.to("bar", 2.0, 14.0);
        }
        assert_eq!(t.to("bar", 2.0, 14.0), 2.0);
        assert!(!t.busy());
    }

    #[test]
    fn unread_values_are_forgotten() {
        let mut t = Tweens::new();
        t.frame();
        t.set("page", 0.0);
        t.frame();
        t.frame();
        // Forgotten, so it starts again at its target.
        assert_eq!(t.to("page", 1.0, 11.0), 1.0);
    }

    #[test]
    fn instant_mode_never_animates() {
        let mut t = Tweens::new();
        t.set_instant(true);
        t.frame();
        t.to("x", 0.0, 10.0);
        t.dt = 0.016;
        assert_eq!(t.to("x", 1.0, 10.0), 1.0);
        t.keep_busy();
        assert!(!t.busy());
    }

    #[test]
    fn from_to_eases_in_from_its_start() {
        let mut t = Tweens::new();
        t.frame();
        t.dt = 0.016;
        let first = t.from_to("guide", 0.0, 1.0, 3.5);
        assert!(first > 0.0 && first < 1.0);
        assert!(t.busy());
        // Once seen, the start no longer applies.
        assert!(t.from_to("guide", 0.0, 1.0, 3.5) > first);
    }

    #[test]
    fn get_peeks_without_keeping_the_value() {
        let mut t = Tweens::new();
        assert_eq!(t.get("x"), None);
        t.frame();
        t.to("x", 2.0, 10.0);
        assert_eq!(t.get("x"), Some(2.0));
        t.frame();
        assert_eq!(t.get("x"), Some(2.0));
        t.frame();
        assert_eq!(t.get("x"), None);
    }

    #[test]
    fn smoothstep_is_clamped() {
        assert_eq!(smoothstep(-1.0), 0.0);
        assert_eq!(smoothstep(0.5), 0.5);
        assert_eq!(smoothstep(2.0), 1.0);
    }
}
