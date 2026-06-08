//! External renderer integrations.

#[cfg(feature = "wgpu")]
pub mod wgpu;

#[cfg(feature = "wgpu")]
pub use wgpu::NativeWindowHandle;

#[cfg(not(feature = "wgpu"))]
#[derive(Debug, Clone, Copy)]
pub enum NativeWindowHandle {
    Unsupported,
}
