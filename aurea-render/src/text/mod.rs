//! Text rendering for Canvas.

pub mod atlas;
pub mod platform;

#[cfg(windows)]
mod directwrite_backend;
mod fontdue_backend;

pub use atlas::*;
pub use platform::*;
