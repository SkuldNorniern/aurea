//! Pixels to draw: images, and the coverage masks text is drawn from.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

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
    /// The data this was worked out for, by address, and whether it was all opaque.
    opaque: Arc<OnceLock<(usize, bool)>>,
}

impl Image {
    /// An image of `width` by `height` RGBA8 pixels.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self::from_shared(width, height, data.into())
    }

    /// An image of `width` by `height` RGBA8 pixels in a buffer that is already shared, so no
    /// copy is made.
    pub fn from_shared(width: u32, height: u32, data: Arc<[u8]>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            width,
            height,
            data,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            opaque: Arc::default(),
        }
    }

    /// Identity of the pixel data; stable across clones.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Says whether every pixel is at full alpha, for a maker that already knows (a tile from a
    /// worker, say), so the first draw doesn't read the whole image to find out. A wrong `true`
    /// hides what is drawn under the image's see-through pixels.
    #[must_use]
    pub fn with_opaque(mut self, opaque: bool) -> Self {
        let known = OnceLock::new();
        let _ = known.set((self.data.as_ptr() as usize, opaque));
        self.opaque = Arc::new(known);
        self
    }

    /// Whether the data covers every pixel at full alpha. Read once per data and kept, or taken
    /// from [`with_opaque`](Self::with_opaque), so renderers can skip what an opaque image hides.
    pub fn is_opaque(&self) -> bool {
        let fits = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4))
            .is_some_and(|len| len > 0 && self.data.len() >= len);
        let scan = || self.data.iter().skip(3).step_by(4).all(|&a| a == 255);
        let at = self.data.as_ptr() as usize;
        fits && match *self.opaque.get_or_init(|| (at, scan())) {
            (seen, opaque) if seen == at => opaque,
            _ => scan(),
        }
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
    fn opacity_follows_the_pixel_data() {
        let mut image = Image::new(2, 1, vec![9, 9, 9, 255, 1, 2, 3, 255]);
        assert!(image.is_opaque());
        assert!(image.clone().is_opaque());
        image.data = vec![9, 9, 9, 255, 1, 2, 3, 254].into();
        assert!(!image.is_opaque(), "new data is read again");
        assert!(
            !Image::new(2, 2, vec![255; 8]).is_opaque(),
            "short data covers nothing"
        );
        assert!(!Image::new(0, 0, Vec::new()).is_opaque());
    }

    #[test]
    fn a_hint_replaces_the_scan_but_not_the_size_check() {
        let see_through = Image::new(1, 1, vec![0, 0, 0, 0]);
        assert!(
            see_through.clone().with_opaque(true).is_opaque(),
            "taken on trust"
        );
        let solid = Image::new(1, 1, vec![0, 0, 0, 255]);
        assert!(!solid.with_opaque(false).is_opaque());
        assert!(
            !Image::new(2, 2, vec![255; 4]).with_opaque(true).is_opaque(),
            "short data"
        );
        let mut hinted = Image::new(1, 1, vec![0, 0, 0, 255]).with_opaque(true);
        hinted.data = vec![0, 0, 0, 7].into();
        assert!(!hinted.is_opaque(), "new data is read again");
    }

    #[test]
    fn glyph_mask_ids_are_unique_and_survive_cloning() {
        let a = GlyphMask::new(1, 1, vec![1, 2, 3].into());
        let b = GlyphMask::new(1, 1, vec![1, 2, 3].into());
        assert_ne!(a.id(), b.id());
        assert_eq!(a.id(), a.clone().id());
    }
}
