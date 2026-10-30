//! The renderer contract: what drives a frame from recording to its target.

pub use crate::drawing::DrawingContext;
pub use crate::frame::{FrameOutcome, FrameOutput};

use crate::capabilities::GpuFeatures;
use crate::gpu::{GpuCacheLimits, GpuCacheStats};
use crate::scene::display_list::DisplayList;
use crate::surface::{Surface, SurfaceInfo};
use crate::types::Rect;
use aurea_foundation::AureaResult;

/// A renderer belongs to the thread that draws with it.
pub trait Renderer {
    /// Attaches a surface and its initial size.
    fn init(&mut self, surface: Surface, info: SurfaceInfo) -> AureaResult<()>;
    /// A new logical size at the current scale factor.
    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()>;
    /// A new logical size and scale factor together: the one call to make whenever either changes.
    fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()>;
    /// Starts a frame and hands back a context to record into.
    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext + '_>>;
    /// Finishes the frame and presents it.
    fn end_frame(&mut self) -> AureaResult<FrameOutcome>;
    /// Releases resources held by the renderer.
    fn cleanup(&mut self);

    /// Set damage region for partial redraw (optional, defaults to full canvas) This is called
    /// before end_frame() to specify which region needs redrawing.
    fn set_damage(&mut self, _damage: Option<Rect>) {}

    /// Optional: access the display list for hit testing (CPU renderer only).
    fn display_list(&self) -> Option<&DisplayList> {
        None
    }

    /// Physical-pixel rect that was repainted by the last `end_frame` call.
    fn frame_output(&self) -> Option<FrameOutput<'_>> {
        None
    }

    /// Damage region of the last frame, if known.
    fn last_frame_damage(&self) -> Option<Rect> {
        None
    }

    /// Frees caches and scratch the renderer can rebuild, between frames, for when the system
    /// reports memory low. The frame buffer and state a frame needs stay.
    fn trim_memory(&mut self) {}

    /// Sets cached GPU image limits between frames. CPU renderers ignore this.
    fn set_gpu_cache_limits(&mut self, _limits: GpuCacheLimits) {}

    /// Cached image payload estimates; `None` for a CPU renderer.
    fn gpu_cache_stats(&self) -> Option<GpuCacheStats> {
        None
    }

    /// What this renderer draws itself; anything else falls back or is left out, as the
    /// renderer's [`crate::Fallback`] says. The CPU rasterizer draws everything.
    fn features(&self) -> GpuFeatures {
        GpuFeatures::ALL
    }
}
