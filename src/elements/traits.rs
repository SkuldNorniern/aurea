use crate::AureaResult;
use crate::render::Rect;
use std::os::raw::c_void;

/// Base trait for all native GUI elements.
pub trait Element {
    /// Return the native handle for this element.
    fn handle(&self) -> *mut c_void;

    fn invalidate(&self, rect: Option<Rect>) {
        if let Some(r) = rect {
            self.invalidate_rect(r);
        } else {
            self.invalidate_all();
        }
    }

    fn invalidate_all(&self) {
        unsafe {
            self.invalidate_platform(None);
        }
    }

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

    fn request_layout(&self) {}
}

/// A container element that can hold child elements.
///
/// Children must be `'static` so the container can keep them alive (preventing
/// their `Drop` from running) until the container itself is dropped.
pub trait Container: Element {
    fn add<E: Element + 'static>(&mut self, element: E) -> AureaResult<()> {
        self.add_weighted(element, 0.0)
    }

    fn add_weighted<E: Element + 'static>(&mut self, element: E, weight: f32) -> AureaResult<()>;

    fn add_all<E, I>(&mut self, elements: I) -> AureaResult<()>
    where
        E: Element + 'static,
        I: IntoIterator<Item = E>,
    {
        self.add_all_weighted(elements, 0.0)
    }

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

/// Common element properties used during construction.
#[derive(Debug, Clone)]
pub struct ElementProps<'a> {
    pub title: &'a str,
    pub width: i32,
    pub height: i32,
}
