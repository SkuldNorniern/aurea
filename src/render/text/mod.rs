//! Text rendering for Canvas
//!
//! Uses system fonts via fontdb/fontdue for CPU text rendering.
//! Platform-native rasterizers can replace this later for tighter OS integration.

pub mod atlas;
pub mod platform;

pub use atlas::*;
pub use platform::*;
