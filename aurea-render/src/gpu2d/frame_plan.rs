//! Backend-neutral frame description produced by the resolve step.
//!
//! [`FramePlan`] carries per-kind instance arrays with texture slots already
//! resolved (each `slot` is the shader-visible bindless index returned by
//! the backend's `upload_image` call), plus the painter-order stream and
//! the clear colour. The backend's `present_frame` consumes this without
//! touching the cache or display list.

use crate::types::Color;

use crate::batch::DrawRef;

/// A resolved gradient instance: LUT texture slot assigned.
#[derive(Clone, Copy, Default)]
pub struct GradientPlanEntry {
    pub rect: [f32; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub slot: u32,
}

/// A resolved image instance: RGBA texture slot assigned, UV mapped.
#[derive(Clone, Copy, Default)]
pub struct ImagePlanEntry {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub tint: [f32; 4],
    pub slot: u32,
}

/// A resolved text instance: coverage-mask texture slot assigned.
#[derive(Clone, Copy, Default)]
pub struct TextPlanEntry {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub slot: u32,
}

/// The complete, resolved description of one frame.
///
/// Produced by `gpu2d::resolve` from a lowered [`RenderBatches`] and the
/// backend-agnostic texture cache. Passed to [`Gpu2dBackend::present_frame`]
/// which owns only the GPU upload + record + present.
///
/// [`RenderBatches`]: crate::batch::RenderBatches
#[derive(Default)]
pub struct FramePlan {
    /// Resolved gradient instances (LUT slot filled in).
    pub gradients: Vec<GradientPlanEntry>,
    /// Resolved image instances (texture slot filled in).
    pub images: Vec<ImagePlanEntry>,
    /// Resolved text instances (mask texture slot filled in).
    pub texts: Vec<TextPlanEntry>,
    /// Painter-order draw stream (indices into per-kind arrays above or into
    /// the rect/circle arrays on the `RenderBatches` side, which the backend
    /// receives alongside this plan).
    pub order: Vec<DrawRef>,
    /// Background clear colour, if any.
    pub clear: Option<Color>,
    /// Physical width/height for viewport setup.
    pub viewport_width: u32,
    pub viewport_height: u32,
}

impl FramePlan {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all per-frame data while keeping heap allocations alive.
    pub fn reset(&mut self) {
        self.gradients.clear();
        self.images.clear();
        self.texts.clear();
        self.order.clear();
        self.clear = None;
    }
}
