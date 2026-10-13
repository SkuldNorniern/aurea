//! Aurea's native rendering framework for custom drawing.

mod canvas;

pub(crate) use canvas::notify_canvas_destroyed;

pub use aurea_render::cpu::{face_data, glyph_coverage, glyph_outline, measure_text, shape_text};
pub use aurea_render::text::{FaceData, FaceId, GlyphCoverage, ShapedGlyph, ShapedText};
pub use aurea_render::vector::{Icon, SvgError, VectorImage, parse_path_data};
pub use aurea_render::{
    BlendMode, ClickCallback, Color, CpuRasterizer, Dash, DisplayIndex, DisplayItem, DisplayList,
    DrawCommand, DrawingContext, Font, FontStyle, FontWeight, GpuTexture, GradientStop, Hex,
    HoverCallback, Hsv, Image, InteractionRegistry, InteractiveId, LineCap, LineJoin,
    LinearGradient, Paint, PaintStyle, Path, PathCommand, Point, RadialGradient, Rect, Renderer,
    RendererBackend, Sampling, Surface, SurfaceInfo, TextMetrics, Transform, Viewport,
};
pub use canvas::*;

#[cfg(feature = "zengpu")]
pub use aurea_render::{GpuChoice, ZenGpuContext, ZenGpuRenderer};
