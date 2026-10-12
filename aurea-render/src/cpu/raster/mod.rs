//! Pixel-level drawing: coverage, blending, scan conversion, strokes and the
//! shapes drawn analytically. The rasterizer above decides what to draw
//! where; this is how.

pub mod blend;
pub mod clip;
pub mod path;
pub(crate) mod round_rect;
pub(crate) mod sample;
pub mod scanline;
pub mod stroke;
