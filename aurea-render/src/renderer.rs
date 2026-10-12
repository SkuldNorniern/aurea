//! The renderer contract: what drives a frame from recording to its target.

pub use crate::drawing::DrawingContext;
pub use crate::frame::{FrameOutcome, FrameOutput};

use crate::display_list::DisplayList;
use crate::surface::{Surface, SurfaceInfo};
use crate::types::Rect;
use aurea_foundation::AureaResult;

/// A renderer belongs to the thread that draws with it.
pub trait Renderer {
    fn init(&mut self, surface: Surface, info: SurfaceInfo) -> AureaResult<()>;
    /// A new logical size at the current scale factor.
    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()>;
    /// A new logical size and scale factor together: the one call to make whenever either changes.
    fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()>;
    /// Starts a frame and hands back a context to record into.
    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext + '_>>;
    fn end_frame(&mut self) -> AureaResult<FrameOutcome>;
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

    fn last_frame_damage(&self) -> Option<Rect> {
        None
    }
}
