//! Backdrop effects on the CPU: the box under a shape blurred, saturated, brightened or tinted.
//! Without the `effects` feature a backdrop draws nothing.

#[cfg(not(feature = "effects"))]
mod off;
#[cfg(feature = "effects")]
mod pass;

#[cfg(not(feature = "effects"))]
pub(crate) use off::{EffectScratch, backdrop, grow_over, next_backdrop};
#[cfg(feature = "effects")]
pub(crate) use pass::{EffectScratch, backdrop, grow_over, next_backdrop};
