//! Backend-neutral frame description produced by the resolve step.

use crate::types::Color;

use crate::gpu::batch::DrawRef;
use crate::types::{Path, Rect};
use std::sync::Arc;

/// A resolved gradient instance: LUT texture slot assigned.
#[derive(Clone, Copy, Default)]
pub struct GradientPlanEntry {
    /// Destination rectangle.
    pub rect: [f32; 4],
    /// Start point, or centre and radius.
    pub a: [f32; 4],
    /// End point, for linear gradients.
    pub b: [f32; 4],
    /// LUT texture slot.
    pub slot: u32,
}

/// A resolved image instance: RGBA texture slot assigned, UV mapped.
#[derive(Clone, Copy, Default)]
pub struct ImagePlanEntry {
    /// Cross terms: x-axis y and y-axis x, then padding.
    pub affine: [f32; 4],
    /// Destination rectangle.
    pub rect: [f32; 4],
    /// Source UV rectangle.
    pub uv: [f32; 4],
    /// Tint colour, RGBA.
    pub tint: [f32; 4],
    /// The source's size in texels, then 1 for nearest filtering, else 0.
    pub sampling: [f32; 4],
    /// Texture slot.
    pub slot: u32,
}

/// A resolved text instance: coverage-mask texture slot assigned.
#[derive(Clone, Copy, Default)]
pub struct TextPlanEntry {
    /// Destination rectangle.
    pub rect: [f32; 4],
    /// Text colour, RGBA.
    pub color: [f32; 4],
    /// Mask texture slot.
    pub slot: u32,
}

/// The complete, resolved description of one frame.
#[derive(Default)]
pub struct FramePlan {
    /// Resolved gradient instances (LUT slot filled in).
    pub gradients: Vec<GradientPlanEntry>,
    /// Resolved image instances (texture slot filled in).
    pub images: Vec<ImagePlanEntry>,
    /// Resolved text instances (mask texture slot filled in).
    pub texts: Vec<TextPlanEntry>,
    /// Painter-order draw stream (indices into per-kind arrays above or into the rect/circle arrays
    /// on the `RenderBatches` side, which the backend receives alongside this plan).
    pub order: Vec<DrawRef>,
    /// The clip for each entry in [`Self::order`], in physical pixels.
    pub clips: Vec<Option<Rect>>,
    /// Device-space path clips aligned with [`Self::order`].
    pub path_clips: Vec<Option<Arc<[Path]>>>,
    /// Background clear colour, if any.
    pub clear: Option<Color>,
    /// Physical width/height for viewport setup.
    pub viewport_width: u32,
    /// Physical height in pixels.
    pub viewport_height: u32,
}

impl FramePlan {
    /// Creates an empty plan.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all per-frame data while keeping heap allocations alive.
    pub fn reset(&mut self) {
        self.gradients.clear();
        self.images.clear();
        self.texts.clear();
        self.order.clear();
        self.clips.clear();
        self.path_clips.clear();
        self.clear = None;
    }
}
