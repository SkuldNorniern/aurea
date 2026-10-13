//! Text rasterization orchestration.
//!
//! Defines the [`PlatformTextRasterizer`] backend seam, the [`SubpixelGlyph`]
//! exchange type, and [`TextRenderer`], which shapes text (see
//! [`crate::text::shape`]) and lays the shaped glyphs' subpixel coverage into
//! a single [`GlyphMask`]. Concrete backends live in sibling modules:
//! - `directwrite_backend` — hinted ClearType via DirectWrite (Windows only).
//! - `fontdue_backend` — cross-platform fallback (no hinting).
//!
//! [`get_platform_rasterizer`] picks the best available backend per platform.

use crate::geometry::Path;
use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped};
use crate::text::glyph::{FaceData, FaceId, GlyphCoverage};
use crate::text::outline::glyph_outline;
use crate::text::shape::{ShapedText, shape_text};
use crate::text::{LruCache, MASK_BYTES};
use crate::types::{Font, FontStyle, FontWeight, GlyphMask, TextMetrics};
use aurea_foundation::{AureaResult, lock};
use std::sync::{Arc, Mutex};

/// Borrowed font reference for the text-rendering hot path.
///
/// Carries the font family as `&str` plus an explicit size, so callers don't
/// need to allocate a new [`Font`] (which owns a `family: String`) just to
/// apply a scale factor before rasterizing or measuring text.
#[derive(Clone, Copy)]
pub struct FontRef<'a> {
    pub family: &'a str,
    pub size: f32,
    pub weight: FontWeight,
    pub style: FontStyle,
}

impl<'a> FontRef<'a> {
    /// Borrow `font`'s family/weight/style with an explicit (e.g.
    /// scale-adjusted) `size`.
    pub fn with_size(font: &'a Font, size: f32) -> Self {
        Self {
            family: font.family.trim(),
            size,
            weight: font.weight,
            style: font.style,
        }
    }
}

impl<'a> From<&'a Font> for FontRef<'a> {
    fn from(font: &'a Font) -> Self {
        Self::with_size(font, font.size)
    }
}

/// A single glyph rasterized to device-resolution RGB subpixel coverage.
///
/// Coordinates are in device pixels. The mask is colourless: `coverage` holds
/// three bytes per pixel (R, G, B subpixel stripes); the text colour is applied
/// at composite time. Backends are expected to return *hinted* coverage where
/// the platform supports it.
#[derive(Clone)]
pub struct SubpixelGlyph {
    /// Bitmap width in device pixels.
    pub width: u32,
    /// Bitmap height in device pixels.
    pub height: u32,
    /// X offset from the pen origin to the bitmap's left edge.
    pub left: i32,
    /// Y offset from the baseline to the bitmap's top edge (down = positive).
    pub top: i32,
    /// Subpixel coverage, `width * height * 3` bytes in R, G, B order.
    pub coverage: Vec<u8>,
}

/// Platform text rasterizer trait — the backend seam. Backends find and load
/// faces and turn glyph ids into coverage; shaping is shared.
pub trait PlatformTextRasterizer: Send + Sync {
    /// The face for `font`: its family in its weight and style when
    /// installed, else the first installed fallback.
    fn face(&self, font: FontRef) -> AureaResult<FaceId>;

    /// The face that draws `ch` in `font`: `primary` when it has the glyph,
    /// else the first fallback that does, else `primary`.
    fn face_for_char(&self, font: FontRef, primary: FaceId, ch: char) -> FaceId;

    /// The face's font file, for shaping and outlines.
    fn face_data(&self, face: FaceId) -> AureaResult<Arc<FaceData>>;

    /// The face's ascent and descent at `size`, both positive.
    fn face_metrics(&self, face: FaceId, size: f32) -> (f32, f32);

    /// `glyph` of `face` at `size` as hinted RGB subpixel coverage (cached).
    fn rasterize_subpixel(
        &self,
        face: FaceId,
        glyph: u16,
        size: f32,
    ) -> AureaResult<Arc<SubpixelGlyph>>;

    /// `glyph` of `face` at `size` as grayscale coverage. By default the
    /// average of the subpixel coverage.
    fn rasterize_gray(&self, face: FaceId, glyph: u16, size: f32) -> AureaResult<GlyphCoverage> {
        let g = self.rasterize_subpixel(face, glyph, size)?;
        Ok(GlyphCoverage {
            width: g.width,
            height: g.height,
            left: g.left,
            top: g.top,
            coverage: g
                .coverage
                .as_chunks::<3>()
                .0
                .iter()
                .map(|[r, g, b]| {
                    let sum = u16::from(*r) + u16::from(*g) + u16::from(*b);
                    u8::try_from(sum / 3).unwrap_or(u8::MAX)
                })
                .collect(),
        })
    }
}

/// Get the best available platform text rasterizer.
///
/// Windows uses DirectWrite (hinted ClearType); everything else uses fontdue.
pub fn get_platform_rasterizer() -> Box<dyn PlatformTextRasterizer> {
    #[cfg(windows)]
    {
        use crate::text::directwrite_backend::DirectWriteRasterizer;
        match DirectWriteRasterizer::new() {
            Ok(dw) => return Box::new(dw),
            Err(_) => {
                // Fall through to the fontdue backend if DirectWrite init fails.
            }
        }
    }
    use crate::text::fontdue_backend::FontDbTextRasterizer;
    Box::new(FontDbTextRasterizer::new())
}

/// Cache key for a run of text in a font.
///
/// A `u64` fingerprint of the text + normalized family + size/weight/style,
/// so looking up a cache (the common case, including hits) never allocates
/// a `String`. The owned text is only needed to render on a miss.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RunKey {
    hash: u64,
}

impl RunKey {
    fn new(text: &str, font: FontRef) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        // ASCII case folding rather than Unicode: font family names are ASCII
        // in practice, and `to_lowercase` per character is a real cost on a
        // path taken once per text item per frame. A family name that differs
        // only in non-ASCII case gets its own cache entry instead of sharing
        // one, which is a wasted entry and not a wrong result.
        for byte in font.family.as_bytes() {
            byte.to_ascii_lowercase().hash(&mut hasher);
        }
        font.size.to_bits().hash(&mut hasher);
        font.weight.hash(&mut hasher);
        font.style.hash(&mut hasher);
        Self {
            hash: hasher.finish(),
        }
    }
}

/// Text renderer: owns a backend, shapes text, and lays shaped glyphs into
/// masks.
pub struct TextRenderer {
    rasterizer: Box<dyn PlatformTextRasterizer>,
    /// Shaped lines, which measuring and drawing the same text share. LRU
    /// cap: 512 entries.
    shaped: Mutex<LruCache<RunKey, Arc<ShapedText>>>,
    /// Text masks. LRU cap: 512 entries and [`MASK_BYTES`]: unchanged text
    /// is not drawn again every frame.
    masks: Mutex<LruCache<RunKey, (GlyphMask, f32, f32)>>,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self::with_rasterizer(get_platform_rasterizer())
    }

    pub(crate) fn with_rasterizer(rasterizer: Box<dyn PlatformTextRasterizer>) -> Self {
        Self {
            rasterizer,
            shaped: Mutex::new(LruCache::new(512)),
            masks: Mutex::new(LruCache::with_budget(512, MASK_BYTES)),
        }
    }

    /// `text` in `font` as one shaped line: glyph ids, the faces that draw
    /// them (fallbacks included), clusters and positions.
    pub fn shape(&self, text: &str, font: FontRef) -> AureaResult<Arc<ShapedText>> {
        let key = RunKey::new(text, font);
        if let Some(hit) = lock(&self.shaped).get(&key).cloned() {
            return Ok(hit);
        }
        let shaped = Arc::new(shape_text(text, font, self.rasterizer.as_ref())?);
        lock(&self.shaped).insert(key, shaped.clone());
        Ok(shaped)
    }

    /// Shape a text run into a subpixel (LCD) coverage mask.
    ///
    /// Returns the mask plus the run's ascent and padding so the caller can
    /// position it: the mask's top-left sits at `(point.x - pad, point.y -
    /// ascent - pad)`. Per-glyph coverage comes pre-hinted from the backend;
    /// this lays the shaped glyphs along the pen and max-combines overlaps.
    pub fn render_text_subpixel(
        &self,
        text: &str,
        font: FontRef,
    ) -> AureaResult<(GlyphMask, f32, f32)> {
        let key = RunKey::new(text, font);
        if let Some(cached) = lock(&self.masks).get(&key).cloned() {
            return Ok(cached);
        }
        let result = self.compute_mask(&*self.shape(text, font)?)?;
        let bytes = result.0.coverage.len();
        lock(&self.masks).insert_sized(key, result.clone(), bytes);
        Ok(result)
    }

    fn compute_mask(&self, shaped: &ShapedText) -> AureaResult<(GlyphMask, f32, f32)> {
        let pad = 3.0f32;
        let ascent = shaped.ink_ascent.max(0.0);
        let height = ascent + shaped.ink_descent.max(0.0);
        let dev_w = f32_to_u32_clamped((shaped.width + pad * 2.0).ceil().max(1.0));
        let dev_h = f32_to_u32_clamped((height + pad * 2.0).ceil().max(1.0));
        let mut coverage = vec![0u8; (dev_w * dev_h * 3) as usize];

        let baseline = f32_to_i32_clamped((ascent + pad).round());
        for (pen, glyph) in shaped.pen_positions() {
            let g = self
                .rasterizer
                .rasterize_subpixel(glyph.face, glyph.id, shaped.size)?;
            if g.width == 0 || g.height == 0 {
                continue;
            }
            let gx = f32_to_i32_clamped((pad + pen + glyph.offset_x).round()) + g.left;
            let gy = baseline + f32_to_i32_clamped(glyph.offset_y.round()) + g.top;
            let gw = g.width as i32;
            for row in 0..g.height as i32 {
                let dy = gy + row;
                if dy < 0 || dy >= dev_h as i32 {
                    continue;
                }
                for col in 0..gw {
                    let dx = gx + col;
                    if dx < 0 || dx >= dev_w as i32 {
                        continue;
                    }
                    let si = ((row * gw + col) * 3).unsigned_abs() as usize;
                    let di = ((dy * dev_w as i32 + dx) * 3).unsigned_abs() as usize;
                    for c in 0..3 {
                        if g.coverage[si + c] > coverage[di + c] {
                            coverage[di + c] = g.coverage[si + c];
                        }
                    }
                }
            }
        }

        Ok((GlyphMask::new(dev_w, dev_h, coverage.into()), ascent, pad))
    }

    /// Measure text dimensions, from the same shaping drawing uses.
    pub fn measure_text(&self, text: &str, font: FontRef) -> AureaResult<TextMetrics> {
        let shaped = self.shape(text, font)?;
        Ok(TextMetrics {
            width: shaped.width,
            height: (shaped.ascent + shaped.descent).max(0.0),
            ascent: shaped.ascent,
            descent: shaped.descent,
            advance: shaped.width,
        })
    }

    /// A shaped glyph's grayscale coverage at `size`, for apps that
    /// composite text themselves.
    pub fn glyph_coverage(&self, face: FaceId, glyph: u16, size: f32) -> AureaResult<GlyphCoverage> {
        self.rasterizer.rasterize_gray(face, glyph, size)
    }

    /// A shaped glyph's outline at `size`, pen at the origin on the
    /// baseline; `None` when it has none.
    pub fn glyph_outline(&self, face: FaceId, glyph: u16, size: f32) -> AureaResult<Option<Path>> {
        Ok(glyph_outline(&*self.rasterizer.face_data(face)?, glyph, size))
    }

    /// The font file behind a face, for apps with their own text pipeline.
    pub fn face_data(&self, face: FaceId) -> AureaResult<Arc<FaceData>> {
        self.rasterizer.face_data(face)
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font(family: &str) -> Font {
        Font::new(family, 24.0)
    }

    #[test]
    fn measure_text_returns_positive_for_non_empty() {
        let r = TextRenderer::new();
        let f = font("");
        let metrics = r.measure_text("Hello", (&f).into()).expect("measure");
        assert!(metrics.width > 0.0, "width should be positive");
        assert!(metrics.ascent > 0.0, "ascent should be positive");
        assert!(metrics.advance > 0.0, "advance should be positive");
    }

    #[test]
    fn subpixel_glyphs_have_rgb_stride() {
        let r = TextRenderer::new();
        let f = font("");
        let shaped = r.shape("A", (&f).into()).expect("shape");
        let g = shaped.glyphs[0];
        let sub = r
            .rasterizer
            .rasterize_subpixel(g.face, g.id, 24.0)
            .expect("rasterize");
        assert_eq!(sub.coverage.len(), (sub.width * sub.height * 3) as usize);
        let gray = r.glyph_coverage(g.face, g.id, 24.0).expect("gray");
        assert_eq!(gray.coverage.len(), (gray.width * gray.height) as usize);
        assert!(gray.coverage.iter().any(|&c| c > 0));
    }

    #[test]
    fn text_shapes_to_one_glyph_per_letter_with_clusters() {
        let r = TextRenderer::new();
        let f = font("");
        let shaped = r.shape("abc", (&f).into()).expect("shape");
        let clusters: Vec<usize> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, [0, 1, 2]);
        assert!(shaped.glyphs.iter().all(|g| g.id != 0 && g.advance > 0.0));
        assert_eq!(r.measure_text("abc", (&f).into()).expect("measure").width, shaped.width);
    }

    /// A combining accent becomes a mark positioned on its base, not a
    /// glyph of its own width.
    #[test]
    fn combining_marks_cost_no_advance() {
        let r = TextRenderer::new();
        let f = font("");
        let plain = r.measure_text("e", (&f).into()).expect("measure").width;
        let accented = r.measure_text("e\u{0301}", (&f).into()).expect("measure").width;
        assert!((accented - plain).abs() < 0.5, "{accented} vs {plain}");
    }

    /// Hangul in a Latin UI font comes from a fallback face that has it,
    /// when one is installed.
    #[test]
    fn mixed_latin_and_hangul_use_two_faces() {
        let r = TextRenderer::new();
        let f = font("");
        let shaped = r.shape("A가", (&f).into()).expect("shape");
        let (latin, hangul) = (shaped.glyphs[0], shaped.glyphs[1]);
        assert_eq!(hangul.cluster, 1);
        if hangul.id != 0 {
            assert_ne!(latin.face, hangul.face, "the UI font has no Hangul");
            let (mask, _, _) = r.render_text_subpixel("A가", (&f).into()).expect("mask");
            assert!(mask.coverage.iter().any(|&c| c > 0));
        }
    }

    #[test]
    fn right_to_left_text_is_laid_out_from_the_right() {
        let r = TextRenderer::new();
        let f = font("");
        // "a" then two Hebrew letters: visually a, then the Hebrew reversed.
        let shaped = r.shape("a\u{05D0}\u{05D1}", (&f).into()).expect("shape");
        let clusters: Vec<usize> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, [0, 3, 1]);
        assert!(shaped.glyphs[1].rtl && !shaped.glyphs[0].rtl);
    }

    #[test]
    fn glyphs_have_outlines() {
        let r = TextRenderer::new();
        let f = font("");
        let shaped = r.shape("O ", (&f).into()).expect("shape");
        let o = r
            .glyph_outline(shaped.glyphs[0].face, shaped.glyphs[0].id, 24.0)
            .expect("face")
            .expect("an O has an outline");
        assert!(o.commands.len() > 4);
        let space = r
            .glyph_outline(shaped.glyphs[1].face, shaped.glyphs[1].id, 24.0)
            .expect("face");
        assert!(space.is_none());
    }
}
