//! Aurea's native rendering framework for custom drawing.
//!
//! Re-exports from aurea-render for the display list, rasterizer, and types.
//! Canvas stays here as it bridges elements, view, and the renderer.

mod canvas;

pub use aurea_render::{
    BlendMode, ClickCallback, Color, CpuRasterizer, CURRENT_BUFFER, DrawCommand,
    DisplayItem, DisplayList, DrawingContext, Font, FontStyle, FontWeight,
    GradientStop, HoverCallback, Image, InteractionRegistry, InteractiveId,
    LinearGradient, NodeId, Paint, PaintStyle, Path, PathCommand, Point,
    RadialGradient, Rect, Renderer, RendererBackend, Surface, SurfaceInfo,
    TextMetrics, Transform, Viewport,
};
pub use canvas::*;
