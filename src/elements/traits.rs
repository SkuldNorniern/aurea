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

    unsafe fn invalidate_platform(&self, rect: Option<Rect>);

    fn request_layout(&self) {}
}

/// A container element that can hold child elements.
pub trait Container: Element {
    fn add<E: Element>(&mut self, element: E) -> AureaResult<()> {
        self.add_weighted(element, 0.0)
    }

    fn add_weighted<E: Element>(&mut self, element: E, weight: f32) -> AureaResult<()>;

    fn add_all<E, I>(&mut self, elements: I) -> AureaResult<()>
    where
        E: Element,
        I: IntoIterator<Item = E>,
    {
        self.add_all_weighted(elements, 0.0)
    }

    fn add_all_weighted<E, I>(&mut self, elements: I, weight: f32) -> AureaResult<()>
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

/// Common element properties used during construction.
#[derive(Debug, Clone)]
pub struct ElementProps<'a> {
    pub title: &'a str,
    pub width: i32,
    pub height: i32,
}
