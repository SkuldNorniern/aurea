use super::native::NativeElement;
use super::traits::{Container, Element};
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::any::Any;
use std::os::raw::c_void;

/// Layout orientation for a Stack container.
#[derive(Debug, Clone, Copy)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// A native container that arranges children in a row or column.
pub struct Stack {
    handle: NativeElement,
    _orientation: Orientation,
    /// Keeps child elements alive so their Drop impls run only when the Stack
    /// itself is dropped, not when they are moved in via `add`.
    _children: Vec<Box<dyn Any>>,
}

impl Stack {
    /// Create a new stack container with the given orientation.
    pub fn new(orientation: Orientation) -> AureaResult<Self> {
        let is_vertical = match orientation {
            Orientation::Vertical => 1,
            Orientation::Horizontal => 0,
        };

        let handle = unsafe { ng_platform_create_box(is_vertical) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self {
            handle: NativeElement::new(handle),
            _orientation: orientation,
            _children: Vec::new(),
        })
    }
}

impl Element for Stack {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {
        unsafe {
            ng_platform_box_invalidate(self.handle.handle());
        }
    }
}

impl Stack {
    /// Add a spacer that expands with the given weight.
    pub fn add_spacer(&mut self, weight: f32) -> AureaResult<()> {
        use super::Spacer;
        self.add_weighted(Spacer::new()?, weight)
    }

    /// Add multiple elements with the same layout weight.
    pub fn add_many<E, I>(&mut self, elements: I, weight: f32) -> AureaResult<()>
    where
        E: Element + 'static,
        I: IntoIterator<Item = E>,
    {
        <Self as Container>::add_all_weighted(self, elements, weight)
    }
}

impl Container for Stack {
    /// Add a child element with layout weight.
    ///
    /// On macOS the weight affects space distribution; on Linux and Windows
    /// the weight is ignored (GTK/Win32 layouts do not use it).
    fn add_weighted<E: Element + 'static>(&mut self, element: E, weight: f32) -> AureaResult<()> {
        let result = unsafe { ng_platform_box_add(self.handle.handle(), element.handle(), weight) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        // The native parent frees its children, so the child must stop freeing
        // itself or it would be freed twice.
        element.released_to_parent();

        // The Rust value stays alive so its other cleanup (a Canvas
        // unregistering from the scheduler, a widget dropping its callback)
        // runs when this Stack is dropped, not when the child is added.
        self._children.push(Box::new(element));
        Ok(())
    }
}
