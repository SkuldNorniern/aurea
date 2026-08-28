//! CPU raster backend with damage-aware display-list rendering.

pub mod blend;
pub mod clip;
pub mod context;
pub mod hit_test;
pub mod path;
pub mod rasterizer;
pub mod scanline;
pub mod stroke;

pub use clip::ClipBox;
pub use context::*;
pub use hit_test::*;
pub use path::*;
pub use rasterizer::*;
pub use scanline::*;
