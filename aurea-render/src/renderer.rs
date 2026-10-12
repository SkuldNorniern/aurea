//! The renderer contract: what drives a frame from recording to its
//! target. [`DrawingContext`] and the frame types live beside it and are
//! re-exported here, where they have always been reachable.

pub use crate::drawing::DrawingContext;
pub use crate::frame::{FrameOutcome, FrameOutput};

use crate::display_list::DisplayList;
use crate::surface::{Surface, SurfaceInfo};
use crate::types::Rect;
use aurea_foundation::AureaResult;

/// A renderer belongs to the thread that draws with it.
///
/// Not `Send` or `Sync`: it holds a native surface, and the toolkits behind
/// those are thread-affine. It used to require both, which forced everything
/// reachable from a draw callback to be `Send + Sync` as well — including the
/// canvas, which then had to be shared through a mutex that bought nothing.
pub trait Renderer {
    fn init(&mut self, surface: Surface, info: SurfaceInfo) -> AureaResult<()>;
    /// A new logical size at the current scale factor.
    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()>;
    /// A new logical size and scale factor together: the one call to make
    /// whenever either changes. A scale change alone resizes the pixel
    /// target, which a separate `init` and `resize` left to chance. A zero
    /// size suspends the renderer until it gets a real one.
    fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()>;
    /// Starts a frame and hands back a context to record into.
    ///
    /// The context borrows the renderer, so it cannot outlive it and the
    /// renderer cannot be touched again until the context is dropped.
    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext + '_>>;
    fn end_frame(&mut self) -> AureaResult<FrameOutcome>;
    fn cleanup(&mut self);

    /// Set damage region for partial redraw (optional, defaults to full canvas)
    /// This is called before end_frame() to specify which region needs redrawing
    fn set_damage(&mut self, _damage: Option<Rect>) {
        // Default implementation does nothing (full redraw)
    }

    /// Optional: access the display list for hit testing (CPU renderer only).
    fn display_list(&self) -> Option<&DisplayList> {
        None
    }

    /// Physical-pixel rect that was repainted by the last `end_frame` call.
    /// `None` means the whole frame was repainted (or unknown). Used by the
    /// platform layer to compute how much of the IOSurface double-buffer needs
    /// refreshing; defaults to `None` for renderers that don't track tile damage.
    /// The pixels this renderer produced, if it keeps any.
    ///
    /// Asked of a renderer rather than read from an ambient slot, so what
    /// comes back belongs to this renderer. The slot was per thread, and with
    /// two canvases on the UI thread the one that rendered last was what the
    /// other published: a canvas with no draw callback would put its
    /// neighbour's pixels on screen.
    ///
    /// `None` for a renderer that presents its own frames, such as a GPU
    /// backend, and for one whose buffer is gone after a resize.
    fn frame_output(&self) -> Option<FrameOutput<'_>> {
        None
    }

    fn last_frame_damage(&self) -> Option<Rect> {
        None
    }
}
