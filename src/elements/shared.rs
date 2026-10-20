//! An element that stays reachable after a container adopts it.

use super::native::{CallbackWatch, watch_destroy};
use super::traits::Element;
use crate::ffi::{ng_platform_element_release, ng_platform_element_retain};
use crate::render::Rect;
use crate::{AureaError, AureaResult};
use std::cell::{Cell, Ref, RefCell, RefMut};
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
/// status.try_borrow_mut()?.set_text("Connected")?;
/// # Ok(())
/// # }
/// ```
pub struct Shared<T: Element>(Rc<Inner<T>>);

struct Inner<T> {
    element: RefCell<T>,
    life: Rc<Life>,
    _destroy: Option<CallbackWatch>,
    _native: Retained,
}

#[derive(Default)]
struct Life {
    destroyed: Cell<bool>,
    listeners: RefCell<Vec<Box<dyn FnOnce()>>>,
}

impl Life {
    fn end(&self) {
        self.destroyed.set(true);
        let listeners = self.listeners.take();
        for listener in listeners {
            listener();
        }
    }
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
        let life = Rc::new(Life::default());
        let heard = Rc::clone(&life);
        let destroy = watch_destroy(handle, move || heard.end()).ok();
        Self(Rc::new(Inner {
            element: RefCell::new(element),
            life,
            _destroy: destroy,
            _native: Retained(handle),
        }))
    }

    pub fn borrow(&self) -> Ref<'_, T> {
        self.0.element.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.0.element.borrow_mut()
    }

    /// The element, unless the platform destroyed it or it is borrowed.
    pub fn try_borrow_mut(&self) -> AureaResult<RefMut<'_, T>> {
        if !self.is_alive() {
            return Err(AureaError::ElementDestroyed);
        }
        self.0
            .element
            .try_borrow_mut()
            .map_err(|_| AureaError::ElementOperationFailed)
    }

    /// False once the platform tore the element down, with its window or its parent.
    pub fn is_alive(&self) -> bool {
        !self.0.life.destroyed.get()
    }

    /// Runs `listener` once when the platform tears the element down; at once if it already has.
    pub fn on_destroyed(&self, listener: impl FnOnce() + 'static) {
        if self.is_alive() {
            self.0.life.listeners.borrow_mut().push(Box::new(listener));
        } else {
            listener();
        }
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
