use super::native::NativeElement;
use super::traits::{Container, Element};
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::any::Any;
use std::os::raw::{c_int, c_void};

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

    /// The gap between children, in logical pixels.
    pub fn set_spacing(&mut self, spacing: u16) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_box_set_spacing(self.handle.handle(), c_int::from(spacing)) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }

    /// The margin inside the edges, in logical pixels.
    pub fn set_padding(&mut self, padding: u16) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_box_set_padding(self.handle.handle(), c_int::from(padding)) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
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
    fn add_weighted<E: Element + 'static>(&mut self, element: E, weight: f32) -> AureaResult<()> {
        let result = unsafe { ng_platform_box_add(self.handle.handle(), element.handle(), weight) };

        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }

        element.released_to_parent();

        self._children.push(Box::new(element));
        Ok(())
    }
}

#[cfg(test)]
mod container_ownership_tests {
    use super::{Orientation, Stack};
    use crate::{AureaResult, Container, Window};

    #[test]
    #[ignore = "creates native elements; run with --ignored"]
    fn a_container_takes_ownership_of_what_it_is_given() -> AureaResult<()> {
        use crate::elements::{Button, Element, Label};

        let _window = Window::new("ownership", 200, 100)?;

        let loose = Label::new("loose")?;
        assert!(!loose.handle().is_null());
        drop(loose);

        let mut stack = Stack::new(Orientation::Vertical)?;
        let adopted = Button::new("adopted")?;
        let handle = adopted.handle();
        stack.add(adopted)?;

        assert!(!stack.handle().is_null());
        assert!(!handle.is_null());

        drop(stack);
        Ok(())
    }

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

    #[test]
    #[ignore = "creates native widgets; run with --ignored"]
    fn ownership_survives_repetition() -> AureaResult<()> {
        use crate::elements::{Button, Label};

        let mut window = Window::new("stress", 300, 200)?;

        for round in 0..200 {
            drop(Label::new("loose")?);

            let mut stack = Stack::new(Orientation::Vertical)?;
            stack.add(Button::new("adopted")?)?;
            stack.add(Label::new("also adopted")?)?;
            drop(stack);

            let mut outer = Stack::new(Orientation::Vertical)?;
            let mut inner = Stack::new(Orientation::Horizontal)?;
            inner.add(Label::new("deep")?)?;
            outer.add(inner)?;
            drop(outer);

            let mut content = Stack::new(Orientation::Vertical)?;
            content.add(Label::new(&format!("round {round}"))?)?;
            window.set_content(content)?;
        }

        window.set_title("done")?;
        Ok(())
    }
}
