//! Aurea's native rendering framework for custom drawing.

mod canvas;

pub use aurea_render::{
    BlendMode, CURRENT_BUFFER, ClickCallback, Color, CpuRasterizer, DisplayItem, DisplayList,
    DrawCommand, DrawingContext, Font, FontStyle, FontWeight, GradientStop, HoverCallback, Image,
    InteractionRegistry, InteractiveId, LinearGradient, NodeId, Paint, PaintStyle, Path,
    PathCommand, Point, RadialGradient, Rect, Renderer, RendererBackend, Surface, SurfaceInfo,
    TextMetrics, Transform, Viewport,
};
pub use canvas::*;

#[cfg(feature = "zengpu")]
pub use aurea_render::ZenGpuRenderer;
