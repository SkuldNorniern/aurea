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
        E: Element,
        I: IntoIterator<Item = E>,
    {
        for element in elements {
            self.add_weighted(element, weight)?;
        }
        Ok(())
    }
}

impl Container for Box {
    /// Add a child element with layout weight.
    ///
    /// On macOS the weight affects space distribution; on Linux and Windows
    /// the weight is ignored (GTK/Win32 layouts do not use it).
    fn add_weighted<E: Element>(&mut self, element: E, weight: f32) -> AureaResult<()> {
        let result = unsafe { ng_platform_box_add(self.handle, element.handle(), weight) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        Ok(())
    }
}
