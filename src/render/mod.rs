//! Rendering API for custom drawing operations
//!
//! This module provides support for integrating Skia and Vello rendering
//! backends into Aurea applications, enabling custom drawing operations
//! within native windows.

mod types;
mod surface;
mod renderer;
mod canvas;
mod viewport;
mod display_list;

#[cfg(feature = "cpu-raster")]
pub mod cpu;

pub use types::*;
pub use surface::*;
pub use renderer::*;
pub use canvas::*;
pub use viewport::*;
pub use display_list::*;

