//! External renderer integrations
//!
//! This module provides integration support for external rendering APIs,
//! enabling hybrid rendering: Aurea native widgets (CPU rasterizer) + external GPU content.
//!
//! Note: These integrations are for external renderers. Aurea's internal Canvas rendering
//! uses CPU rasterizer with event-driven invalidation, not GPU rendering.

#[cfg(feature = "wgpu")]
pub mod wgpu;

pub use crate::platform::handles::NativeWindowHandle;
