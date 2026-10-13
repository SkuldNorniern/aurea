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

#[cfg(test)]
mod container_ownership_tests {
    use super::{Orientation, Stack};
    use crate::{AureaResult, Container, Window};

    /// A widget frees its native element when it is dropped, and stops doing so
    /// once a container has taken it: the platform frees a container's children
    /// along with the container, so freeing it again would be a double free.
    #[test]
    #[ignore = "creates native elements; run with --ignored"]
    fn a_container_takes_ownership_of_what_it_is_given() -> AureaResult<()> {
        use crate::elements::{Button, Element, Label};

        let _window = Window::new("ownership", 200, 100)?;

        let loose = Label::new("loose")?;
        assert!(!loose.handle().is_null());
        // Nothing has adopted it, so dropping it destroys the native label.
        drop(loose);

        let mut stack = Stack::new(Orientation::Vertical)?;
        let adopted = Button::new("adopted")?;
        let handle = adopted.handle();
        stack.add(adopted)?;

        // Still a live native element: the stack holds it now.
        assert!(!stack.handle().is_null());
        assert!(!handle.is_null());

        // Dropping the stack frees the stack and its child together. Doing it
        // twice is what the handover prevents; this must not fault.
        drop(stack);
        Ok(())
    }

    /// A wrapper that forwards ownership behaves like the widget underneath. The
    /// trait method has no default, so a wrapper that forgets does not compile.
    #[test]
    #[ignore = "creates native elements; run with --ignored"]
    fn wrapper_elements_forward_ownership() -> AureaResult<()> {
        use crate::elements::{Divider, Spacer};

        let _window = Window::new("wrappers", 300, 200)?;

        let mut stack = Stack::new(Orientation::Vertical)?;
        stack.add(Spacer::new()?)?;
        stack.add(Divider::horizontal(200)?)?;
        drop(stack);
        Ok(())
    }

    /// Ownership held over many create/drop cycles.
    ///
    /// A double free or a leak usually survives one round trip and shows up under
    /// repetition, so this exercises the shapes that transfer ownership: a loose
    /// widget, an adopted one, nesting, and replacing a window's content.
    #[test]
    #[ignore = "creates native widgets; run with --ignored"]
    fn ownership_survives_repetition() -> AureaResult<()> {
        use crate::elements::{Button, Label};

        let mut window = Window::new("stress", 300, 200)?;

        for round in 0..200 {
            // Created and dropped without ever being adopted: this one frees
            // itself.
            drop(Label::new("loose")?);

            // Adopted, then the parent is dropped: the parent frees it.
            let mut stack = Stack::new(Orientation::Vertical)?;
            stack.add(Button::new("adopted")?)?;
            stack.add(Label::new("also adopted")?)?;
            drop(stack);

            // Nested containers, dropped from the outside in.
            let mut outer = Stack::new(Orientation::Vertical)?;
            let mut inner = Stack::new(Orientation::Horizontal)?;
            inner.add(Label::new("deep")?)?;
            outer.add(inner)?;
            drop(outer);

            // Replacing content destroys what was there before.
            let mut content = Stack::new(Orientation::Vertical)?;
            content.add(Label::new(&format!("round {round}"))?)?;
            window.set_content(content)?;
        }

        // Still usable after all that.
        window.set_title("done")?;
        Ok(())
    }
}
