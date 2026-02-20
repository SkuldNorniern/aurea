//! Layout spacer for distributing space in containers.
//!
//! Use with `add_weighted` to create flexible gaps between elements.

use super::traits::Element;
use super::Label;
use crate::AureaResult;
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

    unsafe fn invalidate_platform(&self, rect: Option<crate::render::Rect>) {
        use super::traits::Element;
        unsafe {
            <Label as Element>::invalidate_platform(&self.inner, rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacer_creates_successfully() {
        let spacer = Spacer::new();
        assert!(spacer.is_ok());
    }
}
