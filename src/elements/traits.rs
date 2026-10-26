use crate::ffi::{
    c_text, ng_platform_element_set_accessible_name, ng_platform_element_set_enabled,
    ng_platform_element_set_min_size, ng_platform_element_set_visible, status,
};
use crate::render::Rect;
use crate::{AureaError, AureaResult};
use std::os::raw::{c_int, c_void};

/// Base trait for all native GUI elements.
pub trait Element {
    /// Return the native handle for this element.
    fn handle(&self) -> *mut c_void;

    /// Tells the element that a container has taken it into its native hierarchy and will free it.
    fn released_to_parent(&self);

    /// Invalidates the whole element, or only `rect` when given.
    fn invalidate(&self, rect: Option<Rect>) {
        if let Some(r) = rect {
            self.invalidate_rect(r);
        } else {
            self.invalidate_all();
        }
    }

    /// Invalidates the whole element.
    fn invalidate_all(&self) {
        unsafe {
            self.invalidate_platform(None);
        }
    }

    /// Invalidates `rect` of the element.
    fn invalidate_rect(&self, rect: Rect) {
        unsafe {
            self.invalidate_platform(Some(rect));
        }
    }

    /// Invalidate the backing platform view for this element.
    ///
    /// # Safety
    ///
    /// Implementations call into native UI handles. The handle returned by
    /// [`Element::handle`] must still be valid, and callers must uphold any
    /// platform main-thread requirements for the underlying toolkit.
    unsafe fn invalidate_platform(&self, rect: Option<Rect>);

    /// Asks for a new layout pass. Does nothing by default.
    fn request_layout(&self) {}

    /// Greys the element out and stops its input.
    fn set_enabled(&self, enabled: bool) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_element_set_enabled(self.handle(), c_int::from(enabled)) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// Shows or hides the element; its container closes the gap.
    fn set_visible(&self, visible: bool) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_element_set_visible(self.handle(), c_int::from(visible)) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// The smallest size layout gives it, in logical pixels.
    fn set_min_size(&self, width: u16, height: u16) -> AureaResult<()> {
        let result = unsafe {
            ng_platform_element_set_min_size(self.handle(), c_int::from(width), c_int::from(height))
        };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// The name screen readers give the element when no label names it, such as an icon button
    /// or an editor; empty clears it.
    fn set_accessible_name(&self, name: &str) -> AureaResult<()> {
        let name = c_text(name)?;
        let code = unsafe { ng_platform_element_set_accessible_name(self.handle(), name.as_ptr()) };
        status("Element::set_accessible_name", code)
    }
}

/// A container element that can hold child elements.
pub trait Container: Element {
    /// Adds `element` with no layout weight.
    fn add<E: Element + 'static>(&mut self, element: E) -> AureaResult<()> {
        self.add_weighted(element, 0.0)
    }

    /// Adds `element` with the given layout weight.
    fn add_weighted<E: Element + 'static>(&mut self, element: E, weight: f32) -> AureaResult<()>;

    /// Adds each element with no layout weight.
    fn add_all<E, I>(&mut self, elements: I) -> AureaResult<()>
    where
        E: Element + 'static,
        I: IntoIterator<Item = E>,
    {
        self.add_all_weighted(elements, 0.0)
    }

    /// Adds each element with the given layout weight.
    fn add_all_weighted<E, I>(&mut self, elements: I, weight: f32) -> AureaResult<()>
    where
        E: Element + 'static,
        I: IntoIterator<Item = E>,
    {
        for element in elements {
            self.add_weighted(element, weight)?;
        }
        Ok(())
    }
}
