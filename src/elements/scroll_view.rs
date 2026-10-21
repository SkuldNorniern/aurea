use super::native::NativeElement;
use super::traits::Element;
use crate::render::Rect;
use crate::{AureaError, AureaResult, Platform, ffi::*};
use std::any::Any;
use std::boxed::Box as StdBox;
use std::os::raw::c_void;

/// When a [`ScrollView`] shows its scroll bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarPolicy {
    /// Only while the content overflows.
    #[default]
    Auto,
    /// Always; disabled while the content fits, so the width never changes.
    Always,
}

/// A native container that scrolls one content element vertically.
pub struct ScrollView {
    handle: NativeElement,
    content: Option<StdBox<dyn Any>>,
}

impl ScrollView {
    /// An empty scroll view.
    pub fn new() -> AureaResult<Self> {
        let handle = unsafe { ng_platform_create_scroll_view() };
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(Self {
            handle: NativeElement::new(handle),
            content: None,
        })
    }

    /// A scroll view showing `content`.
    pub fn with_content<E: Element + 'static>(content: E) -> AureaResult<Self> {
        let mut scroll = Self::new()?;
        scroll.set_content(content)?;
        Ok(scroll)
    }

    /// Replaces the content; the old content is destroyed.
    pub fn set_content<E: Element + 'static>(&mut self, content: E) -> AureaResult<()> {
        let result =
            unsafe { ng_platform_scroll_view_set_content(self.handle.handle(), content.handle()) };
        if result != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        content.released_to_parent();
        self.content = Some(StdBox::new(content));
        Ok(())
    }

    /// Sets when the scroll bar shows.
    pub fn set_scrollbar(&mut self, policy: ScrollbarPolicy) -> AureaResult<()> {
        let code = match policy {
            ScrollbarPolicy::Auto => NG_SCROLLBAR_AUTO,
            ScrollbarPolicy::Always => NG_SCROLLBAR_ALWAYS,
        };
        match unsafe { ng_platform_scroll_view_set_scrollbar(self.handle.handle(), code) } {
            0 => Ok(()),
            NG_ERROR_UNSUPPORTED => Err(AureaError::Unsupported {
                operation: "ScrollView::set_scrollbar",
                platform: Platform::current(),
            }),
            _ => Err(AureaError::ElementOperationFailed),
        }
    }
}

impl Element for ScrollView {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {}
}

#[cfg(test)]
mod scroll_view_tests {
    use crate::{AureaResult, Window};

    #[test]
    #[ignore = "creates native elements; run with --ignored"]
    fn a_scroll_view_replaces_its_content() -> AureaResult<()> {
        use crate::elements::{Label, ScrollView};

        let _window = Window::new("scroll ownership", 300, 200)?;

        let mut scroll = ScrollView::with_content(Label::new("first")?)?;
        scroll.set_content(Label::new("second")?)?;
        drop(scroll);
        Ok(())
    }
}
