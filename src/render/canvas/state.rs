//! What a canvas keeps between frames, and its identity.

#[cfg(feature = "zengpu")]
use std::sync::Arc;

#[cfg(feature = "zengpu")]
use aurea_render::ZenGpuContext;
use aurea_render::{Color, Rect};
use aurea_runtime::DamageRegion;

use super::DrawCallback;

/// Asked for the context to draw on once the canvas's device is lost.
#[cfg(feature = "zengpu")]
pub(crate) type GpuLostHandler = Box<dyn FnMut(&Arc<ZenGpuContext>) -> Option<Arc<ZenGpuContext>>>;

/// All per-frame mutable canvas properties in one lock.
/// Renderer lives in a separate Arc<Mutex<>> so render_frame can release this
/// lock before invoking the draw callback.
pub(crate) struct CanvasState {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub damage: DamageRegion,
    pub background_color: Color,
    pub draw_callback: Option<DrawCallback>,
    pub needs_redraw: bool,
    /// Physical-pixel damage rect from the previous rendered frame. Used to
    /// compute how much of the IOSurface double-buffer needs refreshing this
    /// frame (back surface is always 2 frames stale, so we union N and N-1).
    pub prev_frame_damage: Option<Rect>,
    /// The device a ZenGPU canvas draws with, once known: given at creation,
    /// or made when its renderer first is.
    #[cfg(feature = "zengpu")]
    pub gpu_context: Option<Arc<ZenGpuContext>>,
    /// Set by [`Canvas::on_gpu_lost`](super::Canvas::on_gpu_lost).
    #[cfg(feature = "zengpu")]
    pub gpu_lost: Option<GpuLostHandler>,
}

/// A canvas's identity, for as long as the process runs.
///
/// Allocated when the canvas is created and never given out again. The native
/// handle used to be the identity, and the platform is free to hand the same
/// address back for the next canvas — so work queued against a canvas that
/// has since gone would have redrawn whichever one took its place.
///
/// `Send + Sync`, whereas [`Canvas`](super::Canvas) is not, so this is what a
/// background callback holds to ask a canvas to redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanvasId(pub(super) u64);

impl CanvasId {
    /// The raw value, for logging or as a key of the caller's own.
    pub fn get(self) -> u64 {
        self.0
    }
}
