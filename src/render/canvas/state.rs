//! What a canvas keeps between frames, and its identity.

#[cfg(feature = "zengpu")]
use std::sync::Arc;

#[cfg(feature = "zengpu")]
use aurea_render::ZenGpuContext;
use aurea_render::{Color, Rect};
use aurea_runtime::DamageRegion;

use super::DrawCallback;

#[cfg(feature = "zengpu")]
pub(crate) type GpuLostHandler = Box<dyn FnMut(&Arc<ZenGpuContext>) -> Option<Arc<ZenGpuContext>>>;

pub(crate) struct CanvasState {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub damage: DamageRegion,
    pub background_color: Color,
    pub draw_callback: Option<DrawCallback>,
    pub needs_redraw: bool,
    /// Physical-pixel damage rect from the previous rendered frame.
    pub prev_frame_damage: Option<Rect>,
    /// The device a ZenGPU canvas draws with, once known: given at creation, or made when its
    /// renderer first is.
    #[cfg(feature = "zengpu")]
    pub gpu_context: Option<Arc<ZenGpuContext>>,
    /// Automatically selected contexts share a device on their UI thread.
    #[cfg(feature = "zengpu")]
    pub gpu_context_default: bool,
    /// Set by [`Canvas::on_gpu_lost`](super::Canvas::on_gpu_lost).
    #[cfg(feature = "zengpu")]
    pub gpu_lost: Option<GpuLostHandler>,
}

/// A canvas's identity, for as long as the process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanvasId(pub(super) u64);

impl CanvasId {
    /// The raw value, for logging or as a key of the caller's own.
    pub fn get(self) -> u64 {
        self.0
    }
}
