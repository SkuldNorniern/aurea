use super::native::{CallbackWatch, NativeElement};
use super::traits::Element;
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::cmp::Ordering;
use std::os::raw::c_void;

/// A native slider over a numeric range.
pub struct Slider {
    handle: NativeElement,
    change: Option<CallbackWatch>,
}

impl Slider {
    /// Creates a slider over `min..max`, starting at `min`.
    pub fn new(min: f64, max: f64) -> AureaResult<Self> {
        Self::with_value(min, max, min)
    }

    /// Create a slider with initial value.
    pub fn with_value(min: f64, max: f64, value: f64) -> AureaResult<Self> {
        if !min.is_finite() || !max.is_finite() || min.partial_cmp(&max) != Some(Ordering::Less) {
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
        let clamped = if value.is_nan() {
            min
        } else {
            value.clamp(min, max)
        };
        let _ = slider.set_value(clamped);
        Ok(slider)
    }

    /// Sets the value, clamped to the range. Does not call `on_change`.
    pub fn set_value(&mut self, value: f64) -> AureaResult<()> {
        if value.is_nan() {
            return Err(AureaError::ElementOperationFailed);
        }
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

    /// The current value.
    pub fn get_value(&self) -> f64 {
        unsafe { ng_platform_slider_get_value(self.handle.handle()) }
    }

    /// Enables or disables the slider.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Window;

    #[test]
    fn endless_ranges_are_refused() {
        assert!(Slider::new(0.0, f64::INFINITY).is_err());
        assert!(Slider::new(f64::NEG_INFINITY, 0.0).is_err());
        assert!(Slider::new(f64::NAN, 1.0).is_err());
    }

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn a_nan_value_leaves_the_slider_where_it_was() -> AureaResult<()> {
        let _window = Window::new("slider", 200, 100)?;
        let mut slider = Slider::with_value(0.0, 10.0, f64::NAN)?;
        assert_eq!(slider.get_value(), 0.0);
        slider.set_value(4.0)?;
        assert!(slider.set_value(f64::NAN).is_err());
        assert!((slider.get_value() - 4.0).abs() < 0.01);
        slider.set_value(99.0)?;
        assert_eq!(slider.get_value(), 10.0);
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn native_controls_are_v6_without_an_app_manifest() -> AureaResult<()> {
        unsafe extern "system" {
            fn SendMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
        }
        const CCM_GETVERSION: u32 = 0x2008;
        let _window = Window::new("v6", 200, 100)?;
        let slider = Slider::new(0.0, 1.0)?;
        let version = unsafe { SendMessageW(slider.handle(), CCM_GETVERSION, 0, 0) };
        assert_eq!(version, 6);
        Ok(())
    }
}
