//! Recording draw calls into a display list.
//!
//! [`RecordingContext`] is the one [`DrawingContext`](crate::DrawingContext)
//! both executors read from: it resolves each draw against the transform,
//! clip, opacity and blend mode in force, into physical pixels, and stores
//! the result with its bounds and cache key. The CPU rasterizer and the GPU
//! batches consume the same items, so the two draw the same scene.

mod context;

pub use context::*;
