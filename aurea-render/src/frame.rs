//! What a frame comes to: the pixels a renderer produced, and whether the
//! frame reached its target.

use std::mem::size_of_val;
use std::slice;

/// A renderer's finished pixels, borrowed for as long as the renderer is.
///
/// The pointer is the renderer's own buffer. It stays valid until the
/// renderer next draws or is resized, which is why this borrows it.
#[derive(Debug, Clone, Copy)]
pub struct FrameOutput<'a> {
    /// Start of the pixel data, for handing to native code.
    pixels: *const u8,
    /// Length in bytes.
    size: usize,
    pub width: u32,
    pub height: u32,
    words: &'a [u32],
}

impl<'a> FrameOutput<'a> {
    /// Borrows packed `0xAARRGGBB` pixels matching the supplied extent.
    ///
    /// The frame cannot outlive its storage:
    /// ```compile_fail
    /// use aurea_render::FrameOutput;
    /// let frame = {
    ///     let pixels = vec![0u32];
    ///     FrameOutput::new(&pixels, 1, 1).unwrap()
    /// };
    /// let _ = frame.bytes();
    /// ```
    pub fn new(words: &'a [u32], width: u32, height: u32) -> Option<Self> {
        let count = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?;
        if count == 0 || words.len() != count {
            return None;
        }
        Some(Self {
            pixels: words.as_ptr().cast(),
            size: size_of_val(words),
            width,
            height,
            words,
        })
    }
    /// Start of the pixel data.
    pub fn pixels(&self) -> *const u8 {
        self.pixels
    }
    /// Length of the pixel data in bytes.
    pub fn size(&self) -> usize {
        self.size
    }
    /// The pixel bytes.
    pub fn bytes(&self) -> &'a [u8] {
        // SAFETY: all u32 bytes are initialized, and the slice carries the
        // actual storage borrow. Public presentation metadata is not used
        // to construct this slice.
        unsafe { slice::from_raw_parts(self.words.as_ptr().cast(), size_of_val(self.words)) }
    }

    /// The borrowed ARGB words, without conversion or allocation.
    pub fn words(&self) -> &'a [u32] {
        self.words
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null;

    #[test]
    fn pixels_are_borrowed_and_extents_are_checked() {
        let pixels = [0xFF11_2233u32];
        let frame = FrameOutput::new(&pixels, 1, 1).expect("frame");
        assert_eq!(frame.words().as_ptr(), pixels.as_ptr());
        assert_eq!(frame.bytes(), pixels[0].to_ne_bytes());
        assert!(FrameOutput::new(&pixels, 2, 1).is_none());
        assert!(FrameOutput::new(&pixels, u32::MAX, u32::MAX).is_none());
        assert!(FrameOutput::new(&[], 0, 0).is_none());
    }

    #[test]
    fn presentation_metadata_cannot_invalidate_safe_pixel_access() {
        let pixels = [123u32];
        let mut frame = FrameOutput::new(&pixels, 1, 1).expect("frame");
        frame.pixels = null();
        frame.size = usize::MAX;
        assert_eq!(frame.words(), &pixels);
        assert_eq!(frame.bytes(), pixels[0].to_ne_bytes());
    }
}

/// What [`Renderer::end_frame`] did with the frame, so whoever drives the
/// renderer knows whether to try again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameOutcome {
    /// New pixels are on their way to the target.
    Presented,
    /// Nothing changed since the last frame; what the target shows is
    /// still right.
    Unchanged,
    /// The target could not take a frame now (out of date, timed out,
    /// hidden). Draw again on the next frame.
    Retry,
    /// The target is gone and the renderer must be made again before it
    /// can present.
    Recreate,
    /// The target has no size. Nothing is drawn until it is configured with
    /// one again.
    Suspended,
}

impl FrameOutcome {
    /// Whether the frame still has to be drawn later.
    pub fn needs_redraw(self) -> bool {
        matches!(self, Self::Retry | Self::Recreate | Self::Suspended)
    }
}
