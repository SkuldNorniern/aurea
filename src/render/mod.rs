//! Aurea's native rendering framework for custom drawing.
//!
//! Module boundaries:
//! - display_list: records draw commands with metadata (bounds, cache keys)
//! - command: draw command types shared by display list and raster
//! - cpu: rasterizer executes commands, tile-based with damage
//! - interaction: hit testing on display list items
//! - Damage tracking lives in runtime/damage.rs

mod canvas;
mod command;
mod display_list;
mod interaction;
mod renderer;
mod surface;
mod types;
mod viewport;

pub mod cpu;
pub mod text;

pub use canvas::*;
pub use command::DrawCommand;
pub use display_list::*;
pub use interaction::*;
pub use renderer::*;
pub use surface::*;
pub use types::*;
pub use viewport::*;
