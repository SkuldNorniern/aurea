//! ZenGPU 2D renderer backend on the unified graphics API.

mod backend;
mod buffer;
mod clips;
mod pipelines;
mod shaders;
mod surface;

pub use backend::{GpuChoice, ZenGpuBackend, ZenGpuContext, ZenGpuImage, ZenGpuRenderer};
