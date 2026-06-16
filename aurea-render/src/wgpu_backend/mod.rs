//! wgpu peer 2D backend (feature `wgpu`).
//!
//! Implements [`Gpu2dBackend`](crate::gpu2d::Gpu2dBackend) so that
//! [`WgpuRenderer`] (`Gpu2dRenderer<WgpuBackend>`) draws through a
//! `wgpu::Surface` using the same shared texture-cache and display-list
//! lowering as [`ZenGpuRenderer`](crate::zengpu::ZenGpuRenderer). The caller
//! owns device/queue/surface creation; this module only consumes them.

mod backend;
mod buffer;
mod shaders;

pub use backend::WgpuRenderer;
