//! Backend-agnostic GPU-2D rendering core.
//!
//! [`Gpu2dRenderer`] implements aurea's [`Renderer`] trait **once**, over any
//! [`Gpu2dBackend`]. It owns the display list, batch lowering, texture-cache
//! LRU policy, and display-list resolution; the backend supplies only
//! device-touching primitives. ZenGPU is the first backend on this core; wgpu
//! follows on the same seam, so cache/record machinery exists once instead of
//! once per backend.
//!
//! Laid out like the modular `cpu/` backend: one concern per piece, a thin
//! device boundary, and "add a backend = implement one trait".
//!
//! ZenGPU is the backend Aurea is built around and where GPU work goes. wgpu
//! is here so Aurea can render inside an application already built on wgpu,
//! which is a different job from being the fast path. What they share is the
//! lowering above this line, not a ceiling: a shared trait should not stop
//! ZenGPU using something wgpu cannot express.

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

pub use backend::Gpu2dBackend;
pub use frame_plan::{FramePlan, GradientPlanEntry, ImagePlanEntry, TextPlanEntry};
pub use renderer::Gpu2dRenderer;
