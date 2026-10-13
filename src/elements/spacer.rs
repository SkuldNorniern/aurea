//! Layout spacer for distributing space in containers.
//!
//! Use with `add_weighted` to create flexible gaps between elements.

use super::Label;
use super::traits::Element;
use crate::AureaResult;
use crate::render::Rect;
use std::os::raw::c_void;

/// Invisible spacer that expands to fill available space.
///
/// Add with weight > 0 to a Box to create flexible spacing.
/// Example: `box.add_weighted(Spacer::new()?, 1.0)?`
pub struct Spacer {
    inner: Label,
}

impl Spacer {
    /// Create a new spacer.
    pub fn new() -> AureaResult<Self> {
        let inner = Label::new("")?;
        Ok(Self { inner })
    }
}

impl Element for Spacer {
    fn handle(&self) -> *mut c_void {
        self.inner.handle()
    }

    fn released_to_parent(&self) {
        // The label underneath owns the native element, so it is the one that
        // has to stop freeing it.
        self.inner.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, rect: Option<Rect>) {
        use super::traits::Element;
        unsafe {
            <Label as Element>::invalidate_platform(&self.inner, rect);
        }
    }
}

#[cfg(test)]
mod spacer_tests {
    use crate::{AureaResult, Window};

    /// Same for a spacer, which is a native label underneath.
    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn spacers_can_be_created() -> AureaResult<()> {
        use crate::elements::Spacer;

        let _window = Window::new("spacers", 200, 100)?;

        assert!(Spacer::new().is_ok());
        Ok(())
    }
}
