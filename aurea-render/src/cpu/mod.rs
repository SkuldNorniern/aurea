//! CPU raster backend with damage-aware display-list rendering.

pub mod blend;
pub mod clip;
mod damage;
pub mod path;
pub mod rasterizer;
mod round_rect;
pub mod scanline;
pub mod stroke;
mod tiles;

pub use crate::recording::{RecordingContext, measure_text};
pub use crate::scene::hit_test::{self, *};
pub use clip::ClipBox;
pub use path::*;
pub use rasterizer::*;
pub use scanline::*;
