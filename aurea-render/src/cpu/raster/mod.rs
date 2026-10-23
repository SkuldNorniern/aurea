//! Pixel-level drawing: coverage, blending, scan conversion, strokes and the shapes drawn
//! analytically.

pub mod blend;
pub mod clip;
pub(crate) mod fill;
pub mod path;
pub(crate) mod round_rect;
pub(crate) mod sample;
pub mod stroke;
