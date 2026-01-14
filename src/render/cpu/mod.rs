//! CPU raster backend with tile-based rendering
//!
//! This module provides a CPU-first rasterization backend optimized for
//! low memory usage and partial redraw. It uses a tile-based backing store
//! to enable incremental updates and bounded memory consumption.

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
