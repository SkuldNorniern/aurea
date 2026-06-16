//! ZenGPU 2D renderer backend on the unified graphics API.
//!
//! Replaces `zengpu.rs` + `zengpu_surface.rs`: the unified HAL
//! (`GraphicsPipelineDesc`, `RenderCommands`, `Surface`, `GpuDevice`) drives
//! all rendering — no raw `ash`/`vk` types appear here.

mod backend;
mod buffer;
mod pipelines;
mod shaders;
mod surface;
mod texture_cache;

pub use backend::{ZenGpuBackend, ZenGpuContext, ZenGpuRenderer};
