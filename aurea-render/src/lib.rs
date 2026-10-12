//! Aurea's native rendering framework.
//!
//! Module boundaries:
//! - display_list: records draw commands with metadata (bounds, cache keys)
//! - command: draw command types shared by display list and raster
//! - gpu: display list lowered to batches, the shared GPU renderer, and its backends
//! - recording: resolves draw calls into display list items, for both executors
//! - cpu: rasterizer executes commands, tile-based with damage
//! - interaction: hit testing on display list items
//!
//! # How a frame gets drawn
//!
//! A caller records into a [`RecordingContext`], which resolves each draw
//! against the current transform, clip, opacity and blend mode and stores the
//! result on the item. Nothing is left as pending state: partial repaint
//! renders an arbitrary subset of the list, so anything that only existed
//! between a push and a pop would go missing exactly when a frame is repainted
//! in pieces.
//!
//! Each item also carries a cache key over everything that affects its pixels.
//! The rasterizer diffs this frame's keys against the last frame's positionally,
//! marks the tiles that changed, and clips every draw to that region — so an
//! item spanning changed and unchanged tiles repaints only the changed part.
//! An unchanged frame is detected before any pixel is touched.
//!
//! The geometry an item holds is already resolved, which is why the transform
//! is not part of its cache key: two transforms that put a shape in the same
//! place produce the same pixels and should share a key.

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
pub mod vector;

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
// The module kept its old name at the root.
pub use gpu::wgpu::{self as wgpu_backend, WgpuRenderer};
