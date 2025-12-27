//! CPU raster backend with tile-based rendering
//!
//! This module provides a CPU-first rasterization backend optimized for
//! low memory usage and partial redraw. It uses a tile-based backing store
//! to enable incremental updates and bounded memory consumption.

pub mod tile;
pub mod cache;
pub mod rasterizer;
pub mod context;

pub use tile::*;
pub use cache::*;
pub use rasterizer::*;
pub use context::*;

