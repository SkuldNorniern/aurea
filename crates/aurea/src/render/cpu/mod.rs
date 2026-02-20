//! CPU raster backend with tile-based rendering.
//!
//! Records draw commands into a display list, then rasterizes only tiles that
//! intersect the damage region. Uses a fixed tile size and optional cache to
//! keep memory and redraw cost bounded.

pub mod blend;
pub mod cache;
pub mod context;
pub mod hit_test;
pub mod path;
pub mod rasterizer;
pub mod scanline;
pub mod tile;

pub use cache::*;
pub use context::*;
pub use hit_test::*;
pub use path::*;
pub use rasterizer::*;
pub use scanline::*;
pub use tile::*;
