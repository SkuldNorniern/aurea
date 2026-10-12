//! Aurea's native rendering framework for custom drawing.

mod canvas;
pub mod graph;

pub use aurea_render::cpu::measure_text;
pub use aurea_render::{
    BlendMode, ClickCallback, Color, CpuRasterizer, DisplayIndex, DisplayItem, DisplayList,
    DrawCommand, DrawingContext, Font, FontStyle, FontWeight, GpuTexture, GradientStop,
    HoverCallback, Image, InteractionRegistry, InteractiveId, LinearGradient, Paint, PaintStyle,
    Path, PathCommand, Point, RadialGradient, Rect, Renderer, RendererBackend, Sampling, Surface,
    SurfaceInfo, TextMetrics, Transform, Viewport,
};
pub use canvas::*;

#[cfg(feature = "zengpu")]
pub use aurea_render::{ZenGpuContext, ZenGpuRenderer};
