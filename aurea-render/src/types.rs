//! Common types for rendering operations, gathered from where each one lives so `crate::types::*`
//! keeps reaching all of them.

pub use crate::geometry::{Path, PathCommand, Point, Rect, Transform};
pub use crate::image::{GlyphMask, Image};
pub use crate::paint::{
    BlendMode, Color, GradientStop, LinearGradient, Paint, PaintStyle, RadialGradient,
};
pub use crate::text::font::{Font, FontStyle, FontWeight, TextMetrics};

/// Which renderer draws a canvas.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RendererBackend {
    /// CPU rasterizer (tile-based, display list, partial redraw).
    #[default]
    Cpu,
    /// ZenGPU Vulkan renderer hosted directly in the native canvas surface.
    #[cfg(feature = "zengpu")]
    ZenGpu,
}

/// Interactive element ID for hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractiveId(pub u64);
