//! SwiftUI-in-Aurea: host element that mounts NSHostingView on macOS.

use super::native::NativeElement;
use super::traits::Element;
#[cfg(target_os = "macos")]
use crate::ffi::*;
use crate::render::Rect;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
#[cfg(target_os = "macos")]
use std::os::raw::c_int;
use std::os::raw::c_void;

/// Error when SwiftUI host is not available (non-macOS or Swift library not linked).
#[derive(Debug, Clone)]
pub struct SwiftUIHostNotAvailable;

impl Display for SwiftUIHostNotAvailable {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "SwiftUI host element is only available on macOS with the Swift host library linked"
        )
    }
}

impl Error for SwiftUIHostNotAvailable {}

/// Host element that embeds a SwiftUI view hierarchy inside an Aurea window.
pub struct SwiftUIHost {
    handle: NativeElement,
}

impl SwiftUIHost {
    /// Create a SwiftUI host with the given dimensions.
    pub fn new(width: u32, height: u32) -> Result<Self, SwiftUIHostNotAvailable> {
        #[cfg(target_os = "macos")]
        {
            let w = width as c_int;
            let h = height as c_int;
            if w <= 0 || h <= 0 {
                return Err(SwiftUIHostNotAvailable);
            }
            let handle = unsafe { ng_platform_create_swiftui_host(w, h) };
            if handle.is_null() {
                return Err(SwiftUIHostNotAvailable);
            }
            Ok(Self {
                handle: NativeElement::new(handle),
            })
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (width, height);
            Err(SwiftUIHostNotAvailable)
        }
    }
}

impl Element for SwiftUIHost {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swiftui_host_unavailable_on_non_macos() {
        #[cfg(not(target_os = "macos"))]
        assert!(SwiftUIHost::new(100, 100).is_err());
    }

    #[test]
    fn swiftui_host_invalid_size_returns_err() {
        #[cfg(target_os = "macos")]
        assert!(SwiftUIHost::new(0, 100).is_err());
        #[cfg(target_os = "macos")]
        assert!(SwiftUIHost::new(100, 0).is_err());
    }
}
