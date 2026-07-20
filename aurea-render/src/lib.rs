//! Aurea's native rendering framework.

mod batch;
mod command;
mod display_list;
mod gpu2d;
mod interaction;
mod numeric;
mod renderer;
mod surface;
mod types;
mod viewport;

pub mod cpu;
pub mod text;

#[cfg(feature = "zengpu")]
pub mod zengpu;

#[cfg(feature = "wgpu")]
pub mod wgpu_backend;

pub use batch::{
    CircleInstance, DrawRef, GradientInstance, ImageDraw, RectInstance, RenderBatches, TextDraw,
};
pub use command::DrawCommand;
pub use cpu::CpuRasterizer;
pub use display_list::*;
pub use gpu2d::{Gpu2dBackend, Gpu2dRenderer};
pub use interaction::*;
pub use renderer::*;
pub use surface::*;
pub use types::*;
pub use viewport::*;

#[cfg(feature = "zengpu")]
pub use zengpu::{ZenGpuContext, ZenGpuRenderer};

#[cfg(feature = "wgpu")]
pub use wgpu_backend::WgpuRenderer;
