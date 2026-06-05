//! Text rendering for Canvas.
//!
//! `platform` orchestrates the backend seam and run shaping. Concrete glyph
//! rasterizers are modular:
//! - `directwrite_backend` — hinted ClearType via DirectWrite (Windows only).
//! - `fontdue_backend` — cross-platform fallback (fontdb/fontdue, no hinting).

pub mod atlas;
pub mod platform;

mod fontdue_backend;
#[cfg(windows)]
mod directwrite_backend;

pub use atlas::*;
pub use platform::*;
