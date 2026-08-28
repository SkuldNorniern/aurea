use super::traits::Element;
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use aurea_foundation::lock;
use aurea_runtime::{FrameInfo, FrameScheduler, TickerId};
use std::{
    os::raw::c_void,
    sync::{Arc, Mutex},
};

/// Default sweep speed, progress per second.
const DEFAULT_SPEED: f64 = 0.6;

/// Animation state for progress bar
struct AnimationState {
    current_value: f64,
    target_value: f64,
    /// Progress per second.
    speed: f64,
    increasing: bool,
    enabled: bool,
}

impl AnimationState {
    fn new() -> Self {
        Self {
            current_value: 0.0,
            target_value: 1.0,
            speed: DEFAULT_SPEED,
            increasing: true,
            enabled: false,
        }
    }

    /// Advances by `delta` seconds of real time.
    ///
    /// The scheduler samples the clock once per frame and hands the same delta
    /// to everything it runs, so a widget has no reason to read `Instant::now()`
    /// and throttle itself.
    fn update(&mut self, delta: f64) -> Option<f64> {
        if !self.enabled {
            return None;
        }

        let step = self.speed * delta;

        if self.increasing {
            self.current_value += step;
            if self.current_value >= self.target_value {
                self.current_value = self.target_value;
                self.increasing = false;
                self.target_value = 0.0;
            }
        } else {
            self.current_value -= step;
            if self.current_value <= self.target_value {
                self.current_value = self.target_value;
                self.increasing = true;
                self.target_value = 1.0;
            }
        }

        Some(self.current_value)
    }
}

pub struct ProgressBar {
    handle: *mut c_void,
    animation_state: Arc<Mutex<AnimationState>>,
    /// The ticker driving the sweep, while one is running.
    ticker: Arc<Mutex<Option<TickerId>>>,
}

impl ProgressBar {
    pub fn new() -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_progress_bar() };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self {
            handle,
            animation_state: Arc::new(Mutex::new(AnimationState::new())),
            ticker: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a progress bar with an initial determinate value.
    pub fn with_value(value: f64) -> AureaResult<Self> {
        let mut bar = Self::new()?;
        bar.set_value(value)?;
        Ok(bar)
    }

    /// Create a progress bar and configure indeterminate mode.
    pub fn with_indeterminate(indeterminate: bool) -> AureaResult<Self> {
        let mut bar = Self::new()?;
        bar.set_indeterminate(indeterminate)?;
        Ok(bar)
    }

    /// Starts the ticker that advances the sweep, if one is not running.
    ///
    /// A progress bar is not a canvas, so it registers a ticker instead of
    /// pretending to be one in the canvas registry.
    fn start_ticker(&self) {
        let mut ticker = lock(&self.ticker);
        if ticker.is_some() {
            return;
        }

        let handle = self.handle as usize;
        let animation_state = self.animation_state.clone();
        let slot = Arc::clone(&self.ticker);

        let id = FrameScheduler::register_ticker(move |info: FrameInfo| {
            let advanced = {
                let mut state = lock(&animation_state);
                state.update(info.delta.as_secs_f64())
            };

            match advanced {
                Some(value) => {
                    unsafe {
                        ng_platform_progress_bar_set_value(handle as *mut c_void, value);
                        ng_platform_progress_bar_invalidate(handle as *mut c_void);
                    }
                    true
                }
                None => {
                    // Animation is off, so stop waking up every frame.
                    *lock(&slot) = None;
                    false
                }
            }
        });

        *ticker = Some(id);
    }

    /// Stops the ticker if one is running.
    fn stop_ticker(&self) {
        if let Some(id) = lock(&self.ticker).take() {
            FrameScheduler::unregister_ticker(id);
        }
    }

    pub fn set_value(&mut self, value: f64) -> AureaResult<()> {
        // Stop animation when manually setting value
        {
            let mut state = lock(&self.animation_state);
            state.enabled = false;
        }

        let result = unsafe { ng_platform_progress_bar_set_value(self.handle, value) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// Set progress using 0..=100 percent input.
    pub fn set_percent(&mut self, percent: u8) -> AureaResult<()> {
        let normalized = f64::from(percent.min(100)) / 100.0;
        self.set_value(normalized)
    }

    /// Start automatic animation (oscillates between 0 and 1)
    pub fn start_animation(&self) -> AureaResult<()> {
        {
            let mut state = lock(&self.animation_state);
            state.enabled = true;
            state.current_value = 0.0;
            state.target_value = 1.0;
            state.increasing = true;
        }

        self.start_ticker();

        unsafe {
            self.invalidate_platform(None);
        }

        Ok(())
    }

    /// Stop automatic animation
    pub fn stop_animation(&self) -> AureaResult<()> {
        lock(&self.animation_state).enabled = false;
        self.stop_ticker();
        Ok(())
    }

    /// Set animation speed, as progress per second.
    ///
    /// This used to be per frame, which tied the sweep to the frame rate.
    pub fn set_animation_speed(&self, speed: f64) -> AureaResult<()> {
        let mut state = lock(&self.animation_state);
        state.speed = speed.clamp(0.03, 3.0);
        Ok(())
    }

    pub fn set_indeterminate(&mut self, indeterminate: bool) -> AureaResult<()> {
        // Stop animation when setting indeterminate mode
        {
            let mut state = lock(&self.animation_state);
            state.enabled = false;
        }

        let result = unsafe {
            ng_platform_progress_bar_set_indeterminate(
                self.handle,
                if indeterminate { 1 } else { 0 },
            )
        };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    pub fn set_enabled(&mut self, enabled: bool) -> AureaResult<()> {
        let result = unsafe {
            ng_platform_progress_bar_set_enabled(self.handle, if enabled { 1 } else { 0 })
        };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }
}

impl Element for ProgressBar {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {
        unsafe {
            ng_platform_progress_bar_invalidate(self.handle);
        }
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        self.stop_ticker();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_advances_by_real_time_not_by_frame_count() {
        let mut state = AnimationState::new();
        state.enabled = true;
        state.speed = 1.0;

        // One second in one step, or in ten, has to land in the same place.
        let one_step = state.update(0.5).expect("enabled");

        let mut state = AnimationState::new();
        state.enabled = true;
        state.speed = 1.0;
        let mut many_steps = 0.0;
        for _ in 0..10 {
            many_steps = state.update(0.05).expect("enabled");
        }

        assert!(
            (one_step - many_steps).abs() < 1e-9,
            "{one_step} vs {many_steps}"
        );
    }

    #[test]
    fn a_disabled_sweep_reports_nothing() {
        let mut state = AnimationState::new();
        assert_eq!(state.update(0.016), None);
    }

    #[test]
    fn the_sweep_turns_around_at_both_ends() {
        let mut state = AnimationState::new();
        state.enabled = true;
        state.speed = 10.0;

        assert_eq!(state.update(1.0), Some(1.0), "clamped at the top");
        assert!(!state.increasing, "and turned around");
        assert_eq!(state.update(1.0), Some(0.0), "clamped at the bottom");
        assert!(state.increasing);
    }
}
