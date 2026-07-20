//! External renderer integrations.

#[cfg(feature = "wgpu")]
pub mod wgpu;

pub use crate::platform::handles::NativeWindowHandle;
