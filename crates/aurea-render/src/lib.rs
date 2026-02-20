//! Aurea's native rendering framework.
//!
//! Module boundaries:
//! - display_list: records draw commands with metadata (bounds, cache keys)
//! - command: draw command types shared by display list and raster
//! - cpu: rasterizer executes commands, tile-based with damage
//! - interaction: hit testing on display list items

mod command;
mod display_list;
mod interaction;
mod renderer;
mod surface;
mod types;
mod viewport;

pub mod cpu;
pub mod gpu;
pub mod text;

pub use command::DrawCommand;
pub use display_list::*;
pub use interaction::*;
pub use renderer::*;
pub use cpu::CpuRasterizer;
pub use gpu::GpuRasterizer;
pub use surface::*;
pub use types::*;
pub use viewport::*;
