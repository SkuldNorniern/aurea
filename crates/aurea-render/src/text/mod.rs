//! Text rendering for Canvas.
//!
//! `platform` orchestrates the backend seam and run shaping. Concrete glyph
//! rasterizers are modular:
//! - `directwrite_backend` — hinted ClearType via DirectWrite (Windows only).
//! - `fontdue_backend` — cross-platform fallback (fontdb/fontdue, no hinting).

pub mod atlas;
pub mod platform;

#[cfg(windows)]
mod directwrite_backend;
mod fontdue_backend;

pub use atlas::*;
pub use platform::*;
