//! Pixel-level drawing: coverage, blending, scan conversion, strokes and the shapes drawn
//! analytically.

pub(crate) mod band;
pub mod blend;
pub(crate) mod circle;
pub mod clip;
pub(crate) mod coverage;
pub(crate) mod fill;
pub(crate) mod glyph;
pub(crate) mod gradient;
pub(crate) mod image;
pub(crate) mod item;
pub(crate) mod mask;
pub mod path;
pub(crate) mod pixel;
pub(crate) mod rect;
pub(crate) mod round_rect;
pub(crate) mod sample;
pub mod stroke;
