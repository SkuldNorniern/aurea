//! Rendering API for custom drawing operations
//!
//! This module provides support for integrating Skia and Vello rendering
//! backends into Aurea applications, enabling custom drawing operations
//! within native windows.

mod canvas;
mod display_list;
mod interaction;
mod renderer;
mod surface;
mod types;
mod viewport;

pub mod cpu;
pub mod text;

pub use canvas::*;
pub use display_list::*;
pub use interaction::*;
pub use renderer::*;
pub use surface::*;
pub use types::*;
pub use viewport::*;
