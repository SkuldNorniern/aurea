use crate::AureaResult;
use crate::render::Rect;
use std::os::raw::c_void;

/// Base trait for all native GUI elements.
pub trait Element {
    /// Return the native handle for this element.
    fn handle(&self) -> *mut c_void;

    /// Tells the element that a container has taken it into its native
    /// hierarchy and will free it.
    ///
    /// Containers call this when a child is added. An element that owns a
    /// native handle must stop freeing it: the platform frees a container's
    /// children along with the container, so freeing it again would be a
    /// double free.
    ///
    /// There is deliberately no default. A wrapper around another element
    /// would silently inherit an empty one and leave the element underneath
    /// still believing it owns its handle, which is exactly the double free
    /// this exists to prevent. Forward it to whatever holds the handle:
    ///
    /// ```rust,ignore
    /// impl Element for MyWrapper {
    ///     fn handle(&self) -> *mut c_void {
    ///         self.inner.borrow().handle()
    ///     }
    ///
    ///     fn released_to_parent(&self) {
    ///         self.inner.borrow().released_to_parent();
    ///     }
    /// }
    /// ```
    ///
    /// An element that owns nothing native implements it as an empty body.
    fn released_to_parent(&self);

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
