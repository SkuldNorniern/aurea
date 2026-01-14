//! Text rendering for Canvas
//!
//! Platform-first approach: uses native text APIs for fast, low-memory text rendering.
//! Especially important for mobile platforms (iOS/Android).

pub mod atlas;
pub mod platform;

pub use atlas::*;
pub use platform::*;
