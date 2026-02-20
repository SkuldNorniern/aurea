use super::traits::Element;
use crate::view::FrameScheduler;
use crate::{AureaError, AureaResult, ffi::*};
use std::{
    os::raw::c_void,
    sync::{Arc, Mutex},
    time::Instant,
};

/// Animation state for progress bar
struct AnimationState {
    current_value: f64,
    target_value: f64,
    speed: f64,
    increasing: bool,
    last_update: Instant,
    enabled: bool,
}

impl AnimationState {
    fn new() -> Self {
        Self {
            current_value: 0.0,
            target_value: 1.0,
            speed: 0.02, // Progress per frame (adjust for animation speed)
            increasing: true,
            last_update: Instant::now(),
            enabled: false,
        }
    }

    fn update(&mut self) -> Option<f64> {
        if !self.enabled {
            return None;
        }

        let now = Instant::now();
        // Throttle updates to ~30fps for smoother animation
        if now.duration_since(self.last_update).as_millis() < 33 {
            return None;
        }

        self.last_update = now;

        if self.increasing {
            self.current_value += self.speed;
            if self.current_value >= self.target_value {
                self.current_value = self.target_value;
                self.increasing = false;
                self.target_value = 0.0;
            }
        } else {
            self.current_value -= self.speed;
            if self.current_value <= self.target_value {
                self.current_value = self.target_value;
                self.increasing = true;
                self.target_value = 1.0;
            }
        }

        Some(self.current_value)
    }

    fn needs_update(&self) -> bool {
        self.enabled
    }

    #[allow(dead_code)]
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

pub struct ProgressBar {
    handle: *mut c_void,
    animation_state: Arc<Mutex<AnimationState>>,
}

impl ProgressBar {
    pub fn new() -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_progress_bar() };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        let animation_state = Arc::new(Mutex::new(AnimationState::new()));
        let progress_bar = Self {
            handle,
            animation_state: animation_state.clone(),
        };

        // Register with frame scheduler for animation updates
        progress_bar.register_animation();

        Ok(progress_bar)
    }

    /// Register animation callback with frame scheduler
    fn register_animation(&self) {
        let handle = self.handle as usize;
        let animation_state = self.animation_state.clone();

        let callback: Arc<dyn Fn() -> AureaResult<()> + Send + Sync> = Arc::new(move || {
            let mut state = crate::sync::lock(&animation_state);

            // Update animation state
            if let Some(new_value) = state.update() {
                unsafe {
                    ng_platform_progress_bar_set_value(handle as *mut c_void, new_value);
                    // Invalidate to trigger redraw
                    ng_platform_progress_bar_invalidate(handle as *mut c_void);
                }
            }

            // Always schedule next frame if animation is enabled (for continuous animation)
            // This ensures frames are processed even when update() returns None due to throttling
            if state.needs_update() {
                FrameScheduler::schedule();
            }

            Ok(())
        });

        FrameScheduler::register_canvas(self.handle, callback);
    }

    pub fn set_value(&mut self, value: f64) -> AureaResult<()> {
        // Stop animation when manually setting value
        {
            let mut state = crate::sync::lock(&self.animation_state);
            state.enabled = false;
        }

        let result = unsafe { ng_platform_progress_bar_set_value(self.handle, value) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// Start automatic animation (oscillates between 0 and 1)
    pub fn start_animation(&self) -> AureaResult<()> {
        let mut state = crate::sync::lock(&self.animation_state);
        state.enabled = true;
        state.current_value = 0.0;
        state.target_value = 1.0;
        state.increasing = true;
        state.last_update = Instant::now();

        // Schedule initial frame to start animation
        FrameScheduler::schedule();

        // Also invalidate to trigger immediate update
        unsafe {
            self.invalidate_platform(None);
        }

        Ok(())
    }

    /// Stop automatic animation
    pub fn stop_animation(&self) -> AureaResult<()> {
        let mut state = crate::sync::lock(&self.animation_state);
        state.enabled = false;
        Ok(())
    }

    /// Set animation speed (progress change per frame, typically 0.01-0.05)
    pub fn set_animation_speed(&self, speed: f64) -> AureaResult<()> {
        let mut state = crate::sync::lock(&self.animation_state);
        state.speed = speed.max(0.001).min(0.1); // Clamp to reasonable range
        Ok(())
    }

    pub fn set_indeterminate(&mut self, indeterminate: bool) -> AureaResult<()> {
        // Stop animation when setting indeterminate mode
        {
            let mut state = crate::sync::lock(&self.animation_state);
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

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_progress_bar_invalidate(self.handle);
        }
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        // Unregister from frame scheduler
        FrameScheduler::unregister_canvas(self.handle);
    }
}
