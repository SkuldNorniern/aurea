//! What a frame comes to: the pixels a renderer produced, and whether the frame reached its target.

use std::marker::PhantomData;
use std::slice;

/// A renderer's finished pixels, borrowed for as long as the renderer is.
#[derive(Debug, Clone, Copy)]
pub struct FrameOutput<'a> {
    pixels: *const u8,
    size: usize,
    pub width: u32,
    pub height: u32,
    _borrow: PhantomData<&'a [u8]>,
}

impl<'a> FrameOutput<'a> {
    /// Describes `pixels`, which must stay valid as long as `'a`.
    ///
    /// # Safety
    ///
    /// `pixels` must point to `size` initialised bytes that stay valid, and
    /// are not written, for as long as `'a`.
    pub unsafe fn new(pixels: *const u8, size: usize, width: u32, height: u32) -> Option<Self> {
        if pixels.is_null() || size == 0 {
            return None;
        }
        Some(Self {
            pixels,
            size,
            width,
            height,
            _borrow: PhantomData,
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
        // SAFETY: `new` takes pixels that stay valid, `size` bytes long, for
        unsafe { slice::from_raw_parts(self.pixels, self.size) }
    }
}

/// What [`Renderer::end_frame`] did with the frame, so whoever drives the renderer knows whether to
/// try again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameOutcome {
    /// New pixels are on their way to the target.
    Presented,
    /// Nothing changed since the last frame; what the target shows is still right.
    Unchanged,
    /// The target could not take a frame now (out of date, timed out, hidden).
    Retry,
    /// The target is gone and the renderer must be made again before it can present.
    Recreate,
    /// The target has no size.
    Suspended,
}

impl FrameOutcome {
    /// Whether the frame still has to be drawn later.
    pub fn needs_redraw(self) -> bool {
        matches!(self, Self::Retry | Self::Recreate | Self::Suspended)
    }
}
