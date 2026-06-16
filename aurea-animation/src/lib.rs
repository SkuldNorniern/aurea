pub mod easing;
pub use easing::EaseMode;

use std::time::Duration;

/// A time-based animation that produces eased progress values.
///
/// # Example
/// ```rust
/// use std::time::Duration;
/// use aurea_animation::{Animation, EaseMode};
///
/// let mut anim = Animation::new(Duration::from_secs(2)).ease(EaseMode::OutCubic);
/// // Call `tick(delta)` each frame:
/// // - Returns Some(t) where t ∈ [0, 1] while running.
/// // - Returns Some(1.0) on the final frame, then None when done.
/// ```
pub struct Animation {
    duration: Duration,
    easing: EaseMode,
    elapsed: Duration,
    looping: bool,
}

impl Animation {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            easing: EaseMode::Linear,
            elapsed: Duration::ZERO,
            looping: false,
        }
    }

    pub fn ease(mut self, mode: EaseMode) -> Self {
        self.easing = mode;
        self
    }

    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Advance by `delta` time. Returns eased `t ∈ [0, 1]`, or `None` when finished.
    ///
    /// On the last frame of a non-looping animation, yields `Some(1.0)` exactly once
    /// so callers always see the end value before the animation completes.
    pub fn tick(&mut self, delta: Duration) -> Option<f32> {
        // Sentinel: elapsed > duration means the final Some(1.0) was already yielded.
        if self.elapsed > self.duration {
            return None;
        }

        self.elapsed += delta;

        if self.elapsed >= self.duration {
            if self.looping {
                // Duration has no Rem<Duration>; subtract in a loop (≤few iters in practice).
                while self.elapsed >= self.duration {
                    self.elapsed -= self.duration;
                }
            } else {
                // Yield the terminal value once, then set sentinel for next call.
                self.elapsed = self.duration + Duration::from_nanos(1);
                return Some(self.easing.eval(1.0));
            }
        }

        let t = (self.elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        Some(self.easing.eval(t))
    }

    /// Fraction of time elapsed, independent of easing, clamped to [0, 1].
    pub fn progress(&self) -> f32 {
        (self.elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn is_done(&self) -> bool {
        self.elapsed > self.duration
    }

    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_yields_terminal_value_once() {
        let mut anim = Animation::new(Duration::from_millis(100));
        // Advance past end in one big tick
        let last = anim.tick(Duration::from_millis(200));
        assert_eq!(last, Some(1.0), "must yield Some(1.0) on final frame");
        assert_eq!(
            anim.tick(Duration::from_millis(1)),
            None,
            "must return None after done"
        );
    }

    #[test]
    fn tick_looping_wraps() {
        let mut anim = Animation::new(Duration::from_millis(100)).looping(true);
        let first = anim.tick(Duration::from_millis(50));
        assert!(first.is_some());
        let wrapped = anim.tick(Duration::from_millis(80));
        assert!(
            wrapped.is_some(),
            "looping animation must never return None"
        );
        // After wrap, elapsed should be 30ms → t ≈ 0.3
        assert!(anim.progress() < 0.5);
    }

    #[test]
    fn progress_clamps_to_one() {
        let mut anim = Animation::new(Duration::from_millis(100));
        anim.tick(Duration::from_millis(200));
        assert_eq!(anim.progress(), 1.0);
    }

    #[test]
    fn reset_restarts() {
        let mut anim = Animation::new(Duration::from_millis(100));
        anim.tick(Duration::from_millis(200));
        assert!(anim.is_done());
        anim.reset();
        assert!(!anim.is_done());
        assert!(anim.tick(Duration::from_millis(50)).is_some());
    }
}
