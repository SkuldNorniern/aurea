//! CPU raster backend with damage-aware display-list rendering.

pub mod blend;
pub mod clip;
pub mod hit_test;
pub mod path;
pub mod rasterizer;
mod round_rect;
pub mod scanline;
pub mod stroke;

pub use crate::recording::{RecordingContext, measure_text};
pub use clip::ClipBox;
pub use hit_test::*;
pub use path::*;
pub use rasterizer::*;
pub use scanline::*;
