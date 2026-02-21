//! SwiftUI-in-Aurea: host element that mounts NSHostingView on macOS.
//!
//! Requires linking a Swift library that provides `ng_macos_create_swiftui_host_impl`.
//! See `examples/swiftui_host/` for the Swift implementation and build instructions.

use super::traits::Element;
use crate::ffi::*;
use std::os::raw::{c_int, c_void};

/// Error when SwiftUI host is not available (non-macOS or Swift library not linked).
#[derive(Debug, Clone)]
pub struct SwiftUIHostNotAvailable;

impl std::fmt::Display for SwiftUIHostNotAvailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SwiftUI host element is only available on macOS with the Swift host library linked"
        )
    }
}

impl std::error::Error for SwiftUIHostNotAvailable {}

/// Host element that embeds a SwiftUI view hierarchy inside an Aurea window.
/// macOS only; returns `Err` on other platforms or when the Swift implementation is not linked.
pub struct SwiftUIHost {
    handle: *mut c_void,
}

impl SwiftUIHost {
    /// Create a SwiftUI host with the given dimensions.
    /// The host shows a placeholder SwiftUI view. Link the Swift library from
    /// `examples/swiftui_host/` to provide custom content.
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
            Ok(Self { handle })
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
        self.handle
    }

    unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
        // SwiftUI manages its own updates; no-op.
    }
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
