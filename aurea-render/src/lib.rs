//! Aurea's native rendering framework.
//!
//! Module boundaries:
//! - display_list: records draw commands with metadata (bounds, cache keys)
//! - command: draw command types shared by display list and raster
//! - batch: backend-agnostic 2D batches lowered from a display list (GPU path)
//! - cpu: rasterizer executes commands, tile-based with damage
//! - interaction: hit testing on display list items

mod batch;
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

#[cfg(feature = "zengpu")]
pub mod zengpu;

pub use batch::{CircleInstance, GradientInstance, RectInstance, RenderBatches};
pub use command::DrawCommand;
pub use cpu::CpuRasterizer;
pub use display_list::*;
pub use gpu::GpuRasterizer;
pub use interaction::*;
pub use renderer::*;
pub use surface::*;
pub use types::*;
pub use viewport::*;

#[cfg(feature = "zengpu")]
pub use zengpu::ZenGpuRenderer;
