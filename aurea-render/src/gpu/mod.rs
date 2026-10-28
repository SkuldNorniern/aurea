//! Backend-agnostic GPU-2D rendering core.

mod backend;
pub(crate) mod batch;
mod frame_plan;
mod renderer;
mod resolve;
mod texture_cache;

#[cfg(feature = "wgpu")]
pub mod wgpu;
#[cfg(feature = "zengpu")]
pub mod zengpu;

pub use backend::{Gpu2dBackend, PixelFormat};
pub use frame_plan::{FramePlan, GradientPlanEntry, ImagePlanEntry, TextPlanEntry};
pub use renderer::Gpu2dRenderer;
