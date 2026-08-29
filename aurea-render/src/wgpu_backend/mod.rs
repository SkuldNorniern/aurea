//! wgpu peer 2D backend (feature `wgpu`).
//!
//! Implements [`Gpu2dBackend`](crate::gpu2d::Gpu2dBackend) so that
//! [`WgpuRenderer`] (`Gpu2dRenderer<WgpuBackend>`) draws through a
//! `wgpu::Surface` using the same shared texture-cache and display-list
//! lowering as [`ZenGpuRenderer`](crate::zengpu::ZenGpuRenderer). The caller
//! owns device/queue/surface creation; this module only consumes them.
//!
//! The workspace denies absolute paths, and this module is the exception.
//! wgpu is written `wgpu::Device`, `wgpu::BufferUsages` and so on wherever it
//! is used, including its own examples, and pulling sixty type names into
//! scope to satisfy the lint would make this harder to read against the
//! upstream API, not easier.
#![allow(clippy::absolute_paths)]

mod backend;
mod buffer;
mod shaders;

pub use backend::WgpuRenderer;
