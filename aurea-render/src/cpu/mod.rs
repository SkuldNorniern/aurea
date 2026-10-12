//! CPU raster backend with damage-aware display-list rendering.
//!
//! Records draw commands into a display list, then rasterizes only items that
//! intersect the damage region into a persistent flat framebuffer.

pub mod blend;
pub mod clip;
mod damage;
pub mod path;
pub mod rasterizer;
mod round_rect;
pub mod scanline;
pub mod stroke;
mod tiles;

pub use clip::ClipBox;
// Recording moved up a level; kept reachable here for existing callers.
pub use crate::recording::{RecordingContext, measure_text};
// Moved to the scene; kept reachable here.
pub use crate::scene::hit_test::{self, *};
pub use path::*;
pub use rasterizer::*;
pub use scanline::*;
