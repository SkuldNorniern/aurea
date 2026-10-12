//! wgpu compatibility 2D backend (feature `wgpu`).
#![allow(clippy::absolute_paths)]

mod backend;
mod buffer;
mod shaders;

pub use backend::WgpuRenderer;
