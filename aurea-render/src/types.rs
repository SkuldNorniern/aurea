//! Common types for rendering operations, gathered from where each one
//! lives so `crate::types::*` keeps reaching all of them.

pub use crate::geometry::{FillRule, Path, PathCommand, Point, Rect, Transform};
pub use crate::image::{GlyphMask, GpuTexture, Image, Sampling};
pub use crate::paint::{
    BlendMode, Color, Dash, GradientStop, Hex, Hsv, LineCap, LineJoin, LinearGradient, Paint,
    PaintStyle, RadialGradient,
};
pub use crate::text::font::{Font, FontFeature, FontStyle, FontVariation, FontWeight, TextMetrics};

/// Which renderer draws a canvas.
///
/// Aurea rasterizes itself; there is no Skia or Vello underneath.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RendererBackend {
    /// CPU rasterizer (tile-based, display list, partial redraw)
    #[default]
    Cpu,
    /// ZenGPU Vulkan renderer hosted directly in the native canvas surface.
    ///
    /// Experimental. It does not yet cover every draw command the CPU
    /// rasterizer does, and it has no partial repaint, so the CPU backend is
    /// still the one with full fidelity.
    #[cfg(feature = "zengpu")]
    ZenGpu,
}

/// Interactive element ID for hit testing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractiveId(pub u64);
