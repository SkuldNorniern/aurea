//! An element that stays reachable after a container adopts it.

use super::traits::Element;
use crate::ffi::{ng_platform_element_release, ng_platform_element_retain};
use crate::render::Rect;
use std::cell::{Ref, RefCell, RefMut};
use std::os::raw::c_void;
use std::rc::Rc;

/// Clones share one element, so one can go into a container while another keeps changing it:.
///
/// ```no_run
/// # use aurea::elements::{Container, Label, Orientation, Shared, Stack};
/// # fn main() -> aurea::AureaResult<()> {
/// let status = Shared::new(Label::new("Ready")?);
/// let mut root = Stack::new(Orientation::Vertical)?;
/// root.add(status.clone())?;
/// status.borrow_mut().set_text("Connected")?;
/// # Ok(())
/// # }
/// ```
pub struct Shared<T: Element>(Rc<Inner<T>>);

struct Inner<T> {
    element: RefCell<T>,
    _native: Retained,
}

struct Retained(*mut c_void);

impl Drop for Retained {
    fn drop(&mut self) {
        unsafe { ng_platform_element_release(self.0) };
    }
}

impl<T: Element> Shared<T> {
    pub fn new(element: T) -> Self {
        let handle = element.handle();
        unsafe { ng_platform_element_retain(handle) };
        Self(Rc::new(Inner {
            element: RefCell::new(element),
            _native: Retained(handle),
        }))
    }

    pub fn borrow(&self) -> Ref<'_, T> {
        self.0.element.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.0.element.borrow_mut()
    }
}

impl<T: Element> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: Element> Element for Shared<T> {
    fn handle(&self) -> *mut c_void {
        self.0.element.borrow().handle()
    }

    fn released_to_parent(&self) {
        self.0.element.borrow().released_to_parent();
    }

    unsafe fn invalidate_platform(&self, rect: Option<Rect>) {
        unsafe { self.0.element.borrow().invalidate_platform(rect) };
    }

    fn request_layout(&self) {
        self.0.element.borrow().request_layout();
    }
}
