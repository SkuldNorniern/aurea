//! Aurea's native rendering framework.

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

pub use batch::{RectInstance, RenderBatches};
pub use command::DrawCommand;
pub use cpu::CpuRasterizer;
pub use display_list::*;
pub use gpu::GpuRasterizer;
pub use interaction::*;
pub use renderer::*;
pub use surface::*;
pub use types::*;
pub use viewport::*;
