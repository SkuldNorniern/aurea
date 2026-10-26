//! Glyphs by font face and glyph id: the keys rasterizers cache them under, the font data shaping
//! and outlines read, and grayscale coverage.

use std::sync::Arc;

use harfrust::font::{Blob, NormalizedCoord, Variation};
use harfrust::{Font, Tag};
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::Decoration;
use skrifa::raw::TableProvider;
use skrifa::{FontRef as SkrifaFont, GlyphId, MetadataProvider};

use crate::types::{FontMetrics, FontVariation};

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
    /// Key for `glyph` of `face` at `size`.
    pub fn new(face: FaceId, glyph: u16, size: f32) -> Self {
        Self {
            face,
            glyph,
            size_bits: size.to_bits(),
        }
    }
}

/// A face's font file, at one position on its variable axes, for shaping and glyph outlines.
pub struct FaceData {
    bytes: Arc<Vec<u8>>,
    index: u32,
    font: Font,
}

impl FaceData {
    /// The face at `index` of the font file `bytes` (0 unless it is a collection) at `variations`,
    /// or `None` when it holds no font.
    pub fn new(bytes: Arc<Vec<u8>>, index: u32, variations: &[FontVariation]) -> Option<Self> {
        let blob: Arc<dyn AsRef<[u8]> + Send + Sync> = bytes.clone();
        let mut font = Font::new(Blob::from(blob), index)?;
        if !variations.is_empty() {
            font = font
                .instance_builder()
                .variations(
                    variations
                        .iter()
                        .map(|v| Variation::new(Tag::new(&v.tag), v.value)),
                )
                .build();
        }
        Some(Self { bytes, index, font })
    }

    /// Where on its axes the face is, normalized; empty at the defaults.
    pub fn coords(&self) -> &[NormalizedCoord] {
        self.font.normalized_coords()
    }

    pub(crate) fn advance_deltas(&self, glyphs: &[u32]) -> Vec<f32> {
        if self.coords().is_empty() {
            return Vec::new();
        }
        let Ok(font) = SkrifaFont::from_index(&self.bytes, self.index) else {
            return Vec::new();
        };
        if font.hvar().is_ok() {
            return Vec::new();
        }
        let varied = font.glyph_metrics(Size::unscaled(), self.coords());
        let plain = font.glyph_metrics(Size::unscaled(), LocationRef::default());
        glyphs
            .iter()
            .map(|&g| {
                let g = GlyphId::new(g);
                varied.advance_width(g).unwrap_or(0.0) - plain.advance_width(g).unwrap_or(0.0)
            })
            .collect()
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

    /// Whether the face itself has a glyph for `ch`, not a fallback.
    pub fn has_char(&self, ch: char) -> bool {
        SkrifaFont::from_index(&self.bytes, self.index)
            .is_ok_and(|font| font.charmap().map(ch).is_some_and(|g| g != GlyphId::NOTDEF))
    }

    /// The face's line gap, cap and x height, underline, and whether it is monospace, at `size`
    /// pixels; ascent and descent are left to the rasterizer, which measures with its own.
    pub(crate) fn metrics(&self, size: f32) -> FontMetrics {
        let Ok(font) = SkrifaFont::from_index(&self.bytes, self.index) else {
            return FontMetrics {
                underline_offset: size / 10.0,
                underline_thickness: (size / 14.0).max(1.0),
                ..FontMetrics::default()
            };
        };
        let m = font.metrics(Size::new(size), LocationRef::new(self.coords()));
        let underline = m.underline.unwrap_or(Decoration {
            offset: -size / 10.0,
            thickness: size / 14.0,
        });
        FontMetrics {
            ascent: m.ascent,
            descent: -m.descent,
            line_gap: m.leading.max(0.0),
            cap_height: m.cap_height,
            x_height: m.x_height,
            underline_offset: -underline.offset,
            underline_thickness: underline.thickness.max(f32::EPSILON),
            monospace: m.is_monospace,
        }
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
