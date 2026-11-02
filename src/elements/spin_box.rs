use super::native::{CallbackWatch, NativeElement};
use super::traits::Element;
use crate::ffi::{
    ng_platform_create_spin_box, ng_platform_spin_box_get_value, ng_platform_spin_box_set_value,
    status,
};
use crate::render::Rect;
use crate::{AureaError, AureaResult};
use std::os::raw::c_void;

/// A number the user types or steps with arrows. On iOS it is a stepper with no text, so show the
/// value beside it; Android has none, and creating one is an error there.
pub struct SpinBox {
    handle: NativeElement,
    change: Option<CallbackWatch>,
}

impl SpinBox {
    /// Over `min..=max`, moved `step` at a press, starting at `min`.
    pub fn new(min: f64, max: f64, step: f64) -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_spin_box(min, max, step, min) };
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(Self {
            handle: NativeElement::new(handle),
            change: None,
        })
    }

    /// The value now, kept in range. What is being typed counts once it reads as a number.
    pub fn value(&self) -> f64 {
        unsafe { ng_platform_spin_box_get_value(self.handle.handle()) }
    }

    /// Sets the value, kept in range; [`Self::on_change`] does not hear it.
    pub fn set_value(&mut self, value: f64) -> AureaResult<()> {
        status("SpinBox::set_value", unsafe {
            ng_platform_spin_box_set_value(self.handle.handle(), value)
        })
    }

    /// Calls `on_change` with the value when the user changes it.
    pub fn on_change(&mut self, on_change: impl Fn(f64) + 'static) -> AureaResult<()> {
        let handle = self.handle.handle();
        self.change = None;
        self.change = Some(self.handle.watch_change(move || {
            on_change(unsafe { ng_platform_spin_box_get_value(handle) });
        })?);
        Ok(())
    }
}

impl Element for SpinBox {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {}
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;
    use crate::Window;

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn the_value_stays_in_range() -> AureaResult<()> {
        let _window = Window::new("test", 200, 100)?;
        let mut spin = SpinBox::new(0.0, 10.0, 0.5)?;
        assert_eq!(spin.value(), 0.0);
        spin.set_value(2.5)?;
        assert_eq!(spin.value(), 2.5);
        spin.set_value(99.0)?;
        assert_eq!(spin.value(), 10.0);
        spin.set_value(-5.0)?;
        assert_eq!(spin.value(), 0.0);
        Ok(())
    }

    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn set_value_stays_quiet() -> AureaResult<()> {
        let _window = Window::new("test", 200, 100)?;
        use std::cell::Cell;
        use std::rc::Rc;

        let heard = Rc::new(Cell::new(0));
        let mut spin = SpinBox::new(0.0, 10.0, 1.0)?;
        let sink = Rc::clone(&heard);
        spin.on_change(move |_| sink.set(sink.get() + 1))?;
        spin.set_value(4.0)?;
        assert_eq!(heard.get(), 0);
        Ok(())
    }
}
