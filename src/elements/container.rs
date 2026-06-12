use super::traits::{Container, Element};
use crate::{AureaError, AureaResult, ffi::*};
use std::os::raw::c_void;

/// Layout orientation for a Box container.
#[derive(Debug, Clone, Copy)]
pub enum BoxOrientation {
    Horizontal,
    Vertical,
}

/// A native container that arranges children in a row or column.
pub struct Box {
    handle: *mut c_void,
    _orientation: BoxOrientation,
    /// Keeps child elements alive so their Drop impls run only when the Box
    /// itself is dropped, not when they are moved in via `add`.
    _children: Vec<std::boxed::Box<dyn std::any::Any>>,
}

impl Box {
    /// Create a new box container with the given orientation.
    pub fn new(orientation: BoxOrientation) -> AureaResult<Self> {
        let is_vertical = match orientation {
            BoxOrientation::Vertical => 1,
            BoxOrientation::Horizontal => 0,
        };

        let handle = unsafe { ng_platform_create_box(is_vertical) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(Self {
            handle,
            _orientation: orientation,
            _children: Vec::new(),
        })
    }
}

impl Element for Box {
    fn handle(&self) -> *mut c_void {
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        unsafe {
            ng_platform_box_invalidate(self.handle);
        }
    }
}

impl Box {
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

impl Container for Box {
    /// Add a child element with layout weight.
    ///
    /// On macOS the weight affects space distribution; on Linux and Windows
    /// the weight is ignored (GTK/Win32 layouts do not use it).
    fn add_weighted<E: Element + 'static>(&mut self, element: E, weight: f32) -> AureaResult<()> {
        let result = unsafe { ng_platform_box_add(self.handle, element.handle(), weight) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        // Keep the element alive so its Drop (e.g. Canvas scheduler unregister)
        // only runs when this Box is dropped, not when the element is "added".
        self._children.push(std::boxed::Box::new(element));
        Ok(())
    }
}
