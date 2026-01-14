//! External renderer integrations
//!
//! This module provides integration support for external rendering APIs,
//! enabling hybrid rendering: Aurea native widgets (CPU rasterizer) + external GPU content.
//!
//! Note: These integrations are for external renderers. Aurea's internal Canvas rendering
//! uses CPU rasterizer with event-driven invalidation, not GPU rendering.

#[cfg(feature = "wgpu")]
pub mod wgpu;

// Re-export types that are always available
#[cfg(feature = "wgpu")]
pub use wgpu::NativeWindowHandle;

// Placeholder for when wgpu feature is not enabled
#[cfg(not(feature = "wgpu"))]
#[derive(Debug, Clone, Copy)]
pub enum NativeWindowHandle {
    Unsupported,
}
