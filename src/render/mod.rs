//! Aurea's native rendering framework for custom drawing.
//!
//! Provides the framework that can natively replace external backends
//! (e.g. Skia/Vello): CPU rasterizer, display list, tile-based redraw.
//! Custom drawing runs within native windows and canvases.

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
