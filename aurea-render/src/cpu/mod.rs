//! CPU raster backend with damage-aware display-list rendering.
//!
//! Records draw commands into a display list, then rasterizes only items that
//! intersect the damage region into a persistent flat framebuffer.

mod damage;
pub mod raster;
pub mod rasterizer;
mod tiles;

// The primitives moved under `raster`; their old paths still reach them.
pub use raster::{blend, clip, path, scanline, stroke};

pub use clip::ClipBox;
// Recording moved up a level; kept reachable here for existing callers.
pub use crate::recording::{RecordingContext, measure_text};
// Moved to the scene; kept reachable here.
pub use crate::scene::hit_test::{self, *};
pub use path::*;
pub use rasterizer::*;
pub use scanline::*;
