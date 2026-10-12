//! Pixels to draw: images, and the coverage masks text is drawn from.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Image for rendering
///
/// Pixel data is reference-counted so that drawing the same image repeatedly
/// (the common case for icons/sprites in a redrawn frame) is an O(1) Arc
/// clone rather than a full pixel-buffer memcpy.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Arc<[u8]>,
}

impl Image {
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            data: data.into(),
        }
    }
}

/// Subpixel (LCD/ClearType-style) text coverage mask.
///
/// Stores three coverage samples per device pixel — one per RGB subpixel stripe
/// — so antialiased text gains ~3x effective horizontal resolution. The mask is
/// colourless; the text colour is supplied at composite time and blended per
/// channel in linear light against the destination.
///
/// The coverage bytes are reference-counted so cached masks can be shared across
/// frames with an O(1) clone (just an Arc ref-count bump).
#[derive(Debug, Clone)]
pub struct GlyphMask {
    /// Width in device pixels.
    pub width: u32,
    /// Height in device pixels.
    pub height: u32,
    /// Per-pixel subpixel coverage, 3 bytes/pixel in R, G, B stripe order.
    pub coverage: Arc<[u8]>,
    /// Process-unique identity for these coverage bytes.
    ///
    /// Clones share it, so a mask served from the glyph/run caches keeps the
    /// same identity across frames, while every freshly rasterized mask gets a
    /// new one. Cache keys hash this rather than the `Arc` address: an evicted
    /// mask can be replaced by a different one at the same address, which would
    /// make a changed run of text hash as unchanged.
    id: u64,
}

impl GlyphMask {
    /// Builds a mask and assigns it a fresh identity.
    pub fn new(width: u32, height: u32, coverage: Arc<[u8]>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            width,
            height,
            coverage,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Identity of the coverage bytes; stable across clones.
    pub fn id(&self) -> u64 {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_mask_ids_are_unique_and_survive_cloning() {
        let a = GlyphMask::new(1, 1, vec![1, 2, 3].into());
        let b = GlyphMask::new(1, 1, vec![1, 2, 3].into());
        assert_ne!(a.id(), b.id());
        assert_eq!(a.id(), a.clone().id());
    }
}
