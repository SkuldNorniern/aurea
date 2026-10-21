//! Pixels to draw: images, and the coverage masks text is drawn from.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "zengpu")]
use zengpu_hal::TextureHandle;

/// Image for rendering.
#[derive(Debug, Clone)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8 pixel data.
    pub data: Arc<[u8]>,
    id: u64,
}

impl Image {
    /// An image of `width` by `height` RGBA8 pixels.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            width,
            height,
            data: data.into(),
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Identity of the pixel data; stable across clones.
    pub fn id(&self) -> u64 {
        self.id
    }
}

/// How an image is filtered when drawn at a size other than its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Sampling {
    /// Blends the nearest four pixels: smooth when enlarged, but shrinking far below the image's
    /// size skips pixels.
    #[default]
    Linear,
    /// The nearest pixel, for hard pixel edges in a zoomed-in editor.
    Nearest,
    /// [`Linear`](Self::Linear) when enlarging; when shrinking, every pixel a destination pixel
    /// covers is averaged, for clean zoomed-out views.
    Smooth,
}

/// A texture the app made on the renderer's GPU device, drawn with
/// [`DrawingContext::draw_texture`](crate::DrawingContext::draw_texture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpuTexture {
    #[cfg(feature = "zengpu")]
    pub(crate) handle: TextureHandle,
    /// Slot index in the texture table.
    pub index: u32,
    /// Slot generation, to catch reuse.
    pub generation: u32,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
}

#[cfg(feature = "zengpu")]
impl GpuTexture {
    /// A ZenGPU texture of `width` by `height` texels.
    pub fn from_zengpu(texture: TextureHandle, width: u32, height: u32) -> Self {
        Self {
            handle: texture,
            index: texture.index(),
            generation: texture.generation(),
            width,
            height,
        }
    }

    /// The ZenGPU handle back.
    pub fn zengpu(self) -> TextureHandle {
        self.handle
    }
}

/// Subpixel (LCD/ClearType-style) text coverage mask.
#[derive(Debug, Clone)]
pub struct GlyphMask {
    /// Width in device pixels.
    pub width: u32,
    /// Height in device pixels.
    pub height: u32,
    /// Per-pixel subpixel coverage, 3 bytes/pixel in R, G, B stripe order.
    pub coverage: Arc<[u8]>,
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

    #[cfg(feature = "zengpu")]
    #[test]
    fn gpu_texture_preserves_the_complete_backend_handle() {
        use zengpu_hal::{SlotMap, marker::Texture};
        let mut first = SlotMap::<Texture, ()>::new();
        let mut second = SlotMap::<Texture, ()>::new();
        for handle in [first.insert(()), second.insert(())] {
            let wrapped = GpuTexture::from_zengpu(handle, 4, 4);
            assert_eq!(wrapped.zengpu(), handle);
            assert_eq!(wrapped.index, handle.index());
            assert_eq!(wrapped.generation, handle.generation());
        }
    }

    #[test]
    fn glyph_mask_ids_are_unique_and_survive_cloning() {
        let a = GlyphMask::new(1, 1, vec![1, 2, 3].into());
        let b = GlyphMask::new(1, 1, vec![1, 2, 3].into());
        assert_ne!(a.id(), b.id());
        assert_eq!(a.id(), a.clone().id());
    }
}
