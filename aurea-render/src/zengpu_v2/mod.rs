//! Staging area for the unified-API ZenGPU painter (plan.md P8-A steps 3-7).
//!
//! Built additively alongside the production [`crate::zengpu`]/
//! [`crate::zengpu_surface`] painter, which keeps running unchanged. A
//! capstone commit swaps `ZenGpuBackend` over to this module, deletes the old
//! files, and renames this directory to `zengpu/`.
//!
//! Not yet wired into anything — `#![allow(dead_code)]` until the capstone.
#![allow(dead_code)]

mod buffer;
mod pipelines;
mod shaders;
mod surface;
mod texture_cache;
