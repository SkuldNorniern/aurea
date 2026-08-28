//! Aurea's native rendering framework.
//!
//! Module boundaries:
//! - display_list: records draw commands with metadata (bounds, cache keys)
//! - command: draw command types shared by display list and raster
//! - batch: backend-agnostic 2D batches lowered from a display list (GPU path)
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

mod batch;
mod command;
mod display_list;
mod gpu2d;
mod interaction;
mod numeric;
mod renderer;
mod surface;
mod types;
mod viewport;

pub mod cpu;
pub mod text;

#[cfg(feature = "zengpu")]
pub mod zengpu;

#[cfg(feature = "wgpu")]
pub mod wgpu_backend;

pub use batch::{
    CircleInstance, DrawRef, GradientInstance, ImageDraw, RectInstance, RenderBatches, TextDraw,
};
pub use command::DrawCommand;
pub use cpu::CpuRasterizer;
pub use display_list::*;
pub use gpu2d::{Gpu2dBackend, Gpu2dRenderer};
pub use interaction::*;
pub use renderer::*;
pub use surface::*;
pub use types::*;
pub use viewport::*;

#[cfg(feature = "zengpu")]
pub use zengpu::{ZenGpuContext, ZenGpuRenderer};

#[cfg(feature = "wgpu")]
pub use wgpu_backend::WgpuRenderer;
