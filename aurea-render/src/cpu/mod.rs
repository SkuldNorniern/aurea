//! CPU raster backend with damage-aware display-list rendering.

mod damage;
pub mod raster;
pub mod rasterizer;
mod tiles;

pub use raster::{blend, clip, path, scanline, stroke};

pub use crate::recording::{RecordingContext, measure_text};
pub use crate::scene::hit_test::{self, *};
pub use clip::ClipBox;
pub use path::*;
pub use rasterizer::*;
pub use scanline::*;
