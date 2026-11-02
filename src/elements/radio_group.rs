use super::native::{CallbackWatch, NativeElement};
use super::traits::{Container, Element};
use super::{Orientation, Stack};
use crate::ffi::{
    c_title, ng_platform_checkbox_get_checked, ng_platform_checkbox_set_checked,
    ng_platform_create_radio,
};
use crate::render::Rect;
use crate::{AureaError, AureaResult};
use std::cell::Cell;
use std::os::raw::c_void;
use std::ptr::null_mut;
use std::rc::Rc;

/// Native radio buttons in a column, one picked at a time. Desktop only: iOS and Android have no
/// such control, and creating one is an error there.
pub struct RadioGroup {
    stack: Stack,
    radios: Vec<*mut c_void>,
    picked: Rc<Cell<Option<usize>>>,
    change: Vec<CallbackWatch>,
}

struct Radio(NativeElement);

impl Element for Radio {
    fn handle(&self) -> *mut c_void {
        self.0.handle()
    }

    fn released_to_parent(&self) {
        self.0.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {}
}

impl RadioGroup {
    /// One button per label, the first picked: GTK's radio buttons always have one picked.
    pub fn new(labels: &[&str]) -> AureaResult<Self> {
        let mut stack = Stack::new(Orientation::Vertical)?;
        let mut radios = Vec::with_capacity(labels.len());
        let mut previous = null_mut();
        for label in labels {
            let label = c_title(*label)?;
            let handle = unsafe { ng_platform_create_radio(label.as_ptr(), previous) };
            if handle.is_null() {
                return Err(AureaError::ElementOperationFailed);
            }
            stack.add(Radio(NativeElement::new(handle)))?;
            radios.push(handle);
            previous = handle;
        }
        let mut group = Self {
            stack,
            radios,
            picked: Rc::new(Cell::new(None)),
            change: Vec::new(),
        };
        if !labels.is_empty() {
            group.select(0)?;
        }
        Ok(group)
    }

    /// Picks button `index`; [`Self::on_change`] does not hear it.
    ///
    /// # Errors
    ///
    /// [`AureaError::ElementOperationFailed`] when there is no such button.
    pub fn select(&mut self, index: usize) -> AureaResult<()> {
        if index >= self.radios.len() {
            return Err(AureaError::ElementOperationFailed);
        }
        for (i, radio) in self.radios.iter().enumerate() {
            let on = i32::from(i == index);
            if unsafe { ng_platform_checkbox_set_checked(*radio, on) } != 0 {
                return Err(AureaError::ElementOperationFailed);
            }
        }
        self.picked.set(Some(index));
        Ok(())
    }

    /// The button picked now; `None` only with no buttons.
    pub fn selected(&self) -> Option<usize> {
        self.radios
            .iter()
            .position(|radio| unsafe { ng_platform_checkbox_get_checked(*radio) } != 0)
    }

    /// Calls `on_change` with the button the user picked, once per pick.
    pub fn on_change(&mut self, on_change: impl Fn(usize) + 'static) -> AureaResult<()> {
        self.change.clear();
        let on_change = Rc::new(on_change);
        self.picked.set(self.selected());
        for (i, radio) in self.radios.iter().enumerate() {
            let (radio, picked, on_change) =
                (*radio, Rc::clone(&self.picked), Rc::clone(&on_change));
            // the button losing its pick is told too on some platforms, and a pick's button again
            // on others.
            let watch = NativeElement::watch_change_of(radio, move || {
                let on = unsafe { ng_platform_checkbox_get_checked(radio) } != 0;
                if on && picked.get() != Some(i) {
                    picked.set(Some(i));
                    on_change(i);
                }
            })?;
            self.change.push(watch);
        }
        Ok(())
    }
}

impl Element for RadioGroup {
    fn handle(&self) -> *mut c_void {
        self.stack.handle()
    }

    fn released_to_parent(&self) {
        self.stack.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, rect: Option<Rect>) {
        unsafe { self.stack.invalidate_platform(rect) };
    }
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod tests {
    use super::*;
    use crate::Window;
    use std::cell::RefCell;

    #[test]
    #[ignore = "creates native widgets; run with --ignored"]
    fn one_is_picked_at_a_time() -> AureaResult<()> {
        let _window = Window::new("test", 200, 100)?;
        let mut group = RadioGroup::new(&["Low", "Mid", "High"])?;
        assert_eq!(group.selected(), Some(0));
        group.select(1)?;
        assert_eq!(group.selected(), Some(1));
        group.select(2)?;
        assert_eq!(group.selected(), Some(2));
        assert!(group.select(9).is_err());
        assert_eq!(group.selected(), Some(2));
        Ok(())
    }

    #[test]
    #[ignore = "creates native widgets; run with --ignored"]
    fn select_stays_quiet() -> AureaResult<()> {
        let _window = Window::new("test", 200, 100)?;
        let heard = Rc::new(RefCell::new(Vec::new()));
        let mut group = RadioGroup::new(&["Low", "Mid"])?;
        let sink = Rc::clone(&heard);
        group.on_change(move |i| sink.borrow_mut().push(i))?;
        group.select(1)?;
        assert!(heard.borrow().is_empty());
        Ok(())
    }
}
