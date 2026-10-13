//! Glyphs by font face and glyph id: the keys rasterizers cache them under, the font data shaping
//! and outlines read, and grayscale coverage.

use std::sync::Arc;

use harfrust::Font;
use harfrust::font::Blob;

/// One font face a rasterizer has loaded, by its own numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceId(pub u32);

/// A glyph of a face at a size, for glyph caches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    face: FaceId,
    glyph: u16,
    size_bits: u32,
}

impl GlyphKey {
    pub fn new(face: FaceId, glyph: u16, size: f32) -> Self {
        Self {
            face,
            glyph,
            size_bits: size.to_bits(),
        }
    }
}

/// A face's font file, for shaping and glyph outlines.
pub struct FaceData {
    bytes: Arc<Vec<u8>>,
    index: u32,
    font: Font,
}

impl FaceData {
    /// The face at `index` of the font file `bytes` (0 unless it is a collection), or `None` when
    /// it holds no font.
    pub fn new(bytes: Arc<Vec<u8>>, index: u32) -> Option<Self> {
        let blob: Arc<dyn AsRef<[u8]> + Send + Sync> = bytes.clone();
        let font = Font::new(Blob::from(blob), index)?;
        Some(Self { bytes, index, font })
    }

    /// The font file the face is in.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Which face of the file it is.
    pub fn index(&self) -> u32 {
        self.index
    }

    pub(crate) fn shaping_font(&self) -> &Font {
        &self.font
    }

    /// Design units per em, which glyph positions are in before scaling.
    pub fn units_per_em(&self) -> f32 {
        f32::from(self.font.units_per_em().max(1))
    }
}

/// One glyph's grayscale coverage, for apps that composite text themselves: no subpixel colour
/// fringes, so it suits any background, scale or rotation.
#[derive(Debug, Clone, Default)]
pub struct GlyphCoverage {
    /// Width in device pixels.
    pub width: u32,
    /// Height in device pixels.
    pub height: u32,
    /// X offset from the pen to the bitmap's left edge.
    pub left: i32,
    /// Y offset from the baseline to the bitmap's top edge, down positive.
    pub top: i32,
    /// One byte per pixel, row by row from the top.
    pub coverage: Vec<u8>,
}
