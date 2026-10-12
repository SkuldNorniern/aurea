//! CPU raster backend with damage-aware display-list rendering.
//!
//! Records draw commands into a display list, then rasterizes only items that
//! intersect the damage region into a persistent flat framebuffer.

pub mod blend;
pub mod clip;
pub mod hit_test;
pub mod path;
pub mod rasterizer;
mod round_rect;
pub mod scanline;
pub mod stroke;

pub use clip::ClipBox;
// Recording moved up a level; kept reachable here for existing callers.
pub use crate::recording::{RecordingContext, measure_text};
pub use hit_test::*;
pub use path::*;
pub use rasterizer::*;
pub use scanline::*;
