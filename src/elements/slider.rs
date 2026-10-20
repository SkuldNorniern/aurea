use super::native::{CallbackWatch, NativeElement};
use super::traits::Element;
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::cmp::Ordering;
use std::os::raw::c_void;

pub struct Slider {
    handle: NativeElement,
    change: Option<CallbackWatch>,
}

impl Slider {
    pub fn new(min: f64, max: f64) -> AureaResult<Self> {
        Self::with_value(min, max, min)
    }

    /// Create a slider with initial value.
    pub fn with_value(min: f64, max: f64, value: f64) -> AureaResult<Self> {
        if min.partial_cmp(&max) != Some(Ordering::Less) {
            return Err(AureaError::ElementOperationFailed);
        }

        let handle = unsafe { ng_platform_create_slider(min, max) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        let mut slider = Self {
            handle: NativeElement::new(handle),
            change: None,
        };
        let clamped = value.clamp(min, max);
        let _ = slider.set_value(clamped);
        Ok(slider)
    }

    pub fn set_value(&mut self, value: f64) -> AureaResult<()> {
        let result = unsafe { ng_platform_slider_set_value(self.handle.handle(), value) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }

    /// Calls `on_change` with the value while the user drags; [`Slider::set_value`] does not.
    pub fn on_change(&mut self, on_change: impl Fn(f64) + 'static) -> AureaResult<()> {
        let handle = self.handle.handle();
        self.change = None;
        self.change = Some(self.handle.watch_change(move || {
            on_change(unsafe { ng_platform_slider_get_value(handle) });
        })?);
        Ok(())
    }

    pub fn get_value(&self) -> f64 {
        unsafe { ng_platform_slider_get_value(self.handle.handle()) }
    }

    pub fn set_enabled(&self, enabled: bool) -> AureaResult<()> {
        let result = unsafe {
            ng_platform_slider_set_enabled(self.handle.handle(), if enabled { 1 } else { 0 })
        };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }
}

impl Element for Slider {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {
        unsafe {
            ng_platform_slider_invalidate(self.handle.handle());
        }
    }
}
