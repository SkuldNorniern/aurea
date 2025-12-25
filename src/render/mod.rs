//! Rendering API for custom drawing operations
//!
//! This module provides support for integrating Skia and Vello rendering
//! backends into Aurea applications, enabling custom drawing operations
//! within native windows.

mod types;
mod surface;
mod renderer;
mod canvas;

pub use types::*;
pub use surface::*;
pub use renderer::*;
pub use canvas::*;

