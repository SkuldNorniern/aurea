//! Aurea's native rendering framework.

mod drawing;
mod frame;
mod geometry;
mod gpu;
mod image;
mod interaction;
mod numeric;
mod paint;
mod renderer;
mod scene;
mod surface;
mod types;
mod viewport;

pub mod capabilities;
pub mod cpu;
pub mod recording;
pub mod text;

pub use capabilities::{Fallback, GpuFeatures};
pub use cpu::CpuRasterizer;
pub use gpu::batch::{
    DrawRef, GradientInstance, ImageDraw, RectInstance, RenderBatches, ShapeInstance, TextDraw,
};
pub use gpu::{
    FramePlan, Gpu2dBackend, Gpu2dRenderer, GradientPlanEntry, ImagePlanEntry, TextPlanEntry,
};
pub use interaction::*;
pub use renderer::*;
pub use scene::command::DrawCommand;
pub use scene::display_list::*;
pub use surface::*;
pub use types::*;
pub use viewport::*;

#[cfg(feature = "zengpu")]
pub use gpu::zengpu::{self, ZenGpuContext, ZenGpuRenderer};

#[cfg(feature = "wgpu")]
pub use gpu::wgpu::{self as wgpu_backend, WgpuRenderer};
