//! Text rasterization orchestration.

use crate::geometry::Path;
use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped};
use crate::text::glyph::{FaceData, FaceId, GlyphCoverage};
use crate::text::layout::{LayoutOptions, TextLayout, layout_text};
use crate::text::outline::glyph_outline;
use crate::text::shape::{ShapedGlyph, ShapedText, shape_line};
use crate::text::{
    CacheKind, LAYOUT_BYTES, LruCache, MASK_BYTES, SHAPED_BYTES, layout_bytes, shared,
};
use crate::types::{
    Font, FontFeature, FontMetrics, FontStyle, FontVariation, FontWeight, GlyphMask, TextMetrics,
};
use aurea_foundation::{AureaResult, lock};
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::sync::{Arc, Mutex};

/// Borrowed font reference for the text-rendering hot path.
#[derive(Clone, Copy)]
pub struct FontRef<'a> {
    /// Font family name.
    pub family: &'a str,
    /// Font size in pixels.
    pub size: f32,
    /// Font weight.
    pub weight: FontWeight,
    /// Font style.
    pub style: FontStyle,
    /// OpenType feature settings.
    pub features: &'a [FontFeature],
    /// Variable font axis settings.
    pub variations: &'a [FontVariation],
    /// Whether the text runs vertically.
    pub vertical: bool,
}

impl<'a> FontRef<'a> {
    /// Borrow `font`'s family/weight/style with an explicit (e.g. scale-adjusted) `size`.
    pub fn with_size(font: &'a Font, size: f32) -> Self {
        Self {
            family: font.family.trim(),
            size,
            weight: font.weight,
            style: font.style,
            features: &font.features,
            variations: &font.variations,
            vertical: font.vertical,
        }
    }

    pub(crate) fn variation_hash(&self) -> u64 {
        variation_hash(self.variations)
    }
}

impl<'a> From<&'a Font> for FontRef<'a> {
    fn from(font: &'a Font) -> Self {
        Self::with_size(font, font.size)
    }
}

pub(crate) fn variation_hash(variations: &[FontVariation]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    for v in variations {
        v.tag.hash(&mut hasher);
        v.value.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

/// A single glyph rasterized to device-resolution RGB subpixel coverage.
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

/// Platform text rasterizer trait — the backend seam.
pub trait PlatformTextRasterizer: Send + Sync {
    /// The face for `font`: its family in its weight and style when installed, else the first
    /// installed fallback.
    fn face(&self, font: FontRef) -> AureaResult<FaceId>;

    /// The face that draws `ch` in `font`: `primary` when it has the glyph, else the first fallback
    /// that does, else `primary`.
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

    /// `glyph` of `face` at `size` as grayscale coverage.
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
pub fn get_platform_rasterizer() -> Box<dyn PlatformTextRasterizer> {
    #[cfg(windows)]
    {
        use crate::text::directwrite_backend::DirectWriteRasterizer;
        if let Ok(dw) = DirectWriteRasterizer::new() {
            return Box::new(dw);
        }
    }
    use crate::text::fontdue_backend::FontDbTextRasterizer;
    Box::new(FontDbTextRasterizer::new())
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RunKey {
    hash: u64,
}

fn hash_font(font: FontRef, hasher: &mut impl Hasher) {
    for byte in font.family.as_bytes() {
        byte.to_ascii_lowercase().hash(hasher);
    }
    font.size.to_bits().hash(hasher);
    font.weight.hash(hasher);
    font.style.hash(hasher);
    font.features.hash(hasher);
    font.variation_hash().hash(hasher);
    font.vertical.hash(hasher);
}

impl RunKey {
    fn new(text: &str, font: FontRef, rtl: Option<bool>) -> Self {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        rtl.hash(&mut hasher);
        hash_font(font, &mut hasher);
        Self {
            hash: hasher.finish(),
        }
    }

    fn layout(text: &str, font: FontRef, options: LayoutOptions) -> Self {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        options.max_width.map(f32::to_bits).hash(&mut hasher);
        options.align.hash(&mut hasher);
        options.line_height.to_bits().hash(&mut hasher);
        options.word_break.hash(&mut hasher);
        hash_font(font, &mut hasher);
        Self {
            hash: hasher.finish(),
        }
    }
}

fn blit_max(
    coverage: &mut [u8],
    (dev_w, dev_h): (u32, u32),
    g: &SubpixelGlyph,
    (gx, gy): (i32, i32),
) {
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
                coverage[di + c] = coverage[di + c].max(g.coverage[si + c]);
            }
        }
    }
}

/// Text renderer: owns a backend, shapes text, and lays shaped glyphs into masks.
pub struct TextRenderer {
    rasterizer: Box<dyn PlatformTextRasterizer>,
    shaped: Arc<Mutex<LruCache<RunKey, Arc<ShapedText>>>>,
    masks: Arc<Mutex<MaskCache>>,
    layouts: Arc<Mutex<LruCache<RunKey, TextLayout>>>,
}

type MaskCache = LruCache<RunKey, (GlyphMask, f32, f32)>;

/// Paragraphs kept in the layout cache before the oldest go.
const LAYOUT_CAP: usize = 256;

impl TextRenderer {
    /// Creates a renderer with the platform's rasterizer.
    pub fn new() -> Self {
        Self::with_rasterizer(get_platform_rasterizer())
    }

    pub(crate) fn with_rasterizer(rasterizer: Box<dyn PlatformTextRasterizer>) -> Self {
        Self {
            rasterizer,
            shaped: shared(CacheKind::Shape, LruCache::with_budget(512, SHAPED_BYTES)),
            masks: shared(CacheKind::Mask, LruCache::with_budget(512, MASK_BYTES)),
            layouts: shared(
                CacheKind::Layout,
                LruCache::with_budget(LAYOUT_CAP, LAYOUT_BYTES),
            ),
        }
    }

    /// `text` in `font` as one shaped line: glyph ids, the faces that draw them (fallbacks
    /// included), clusters and positions.
    pub fn shape(&self, text: &str, font: FontRef) -> AureaResult<Arc<ShapedText>> {
        self.shape_directed(text, font, None)
    }

    /// As [`Self::shape`], for a line of a paragraph read right to left when `rtl`, or found from
    /// the text when `None`.
    pub fn shape_directed(
        &self,
        text: &str,
        font: FontRef,
        rtl: Option<bool>,
    ) -> AureaResult<Arc<ShapedText>> {
        let key = RunKey::new(text, font, rtl);
        if let Some(hit) = lock(&self.shaped).get(&key).cloned() {
            return Ok(hit);
        }
        let shaped = Arc::new(shape_line(text, font, rtl, self.rasterizer.as_ref())?);
        let bytes = shaped.glyphs.len() * size_of::<ShapedGlyph>();
        lock(&self.shaped).insert_sized(key, shaped.clone(), bytes);
        Ok(shaped)
    }

    /// Shape a text run into a subpixel (LCD) coverage mask.
    pub fn render_text_subpixel(
        &self,
        text: &str,
        font: FontRef,
    ) -> AureaResult<(GlyphMask, f32, f32)> {
        self.render_text_directed(text, font, None)
    }

    /// As [`Self::render_text_subpixel`], for a line of a paragraph read right to left when `rtl`,
    /// or found from the text when `None`.
    pub fn render_text_directed(
        &self,
        text: &str,
        font: FontRef,
        rtl: Option<bool>,
    ) -> AureaResult<(GlyphMask, f32, f32)> {
        let key = RunKey::new(text, font, rtl);
        if let Some(cached) = lock(&self.masks).get(&key).cloned() {
            return Ok(cached);
        }
        let result = self.compute_mask(&*self.shape_directed(text, font, rtl)?)?;
        let bytes = result.0.coverage.len();
        lock(&self.masks).insert_sized(key, result.clone(), bytes);
        Ok(result)
    }

    fn compute_mask(&self, shaped: &ShapedText) -> AureaResult<(GlyphMask, f32, f32)> {
        let pad = 3.0f32;
        let ascent = if shaped.vertical {
            0.0
        } else {
            shaped.ink_ascent.max(0.0)
        };
        let (w, h) = if shaped.vertical {
            (shaped.size, shaped.width)
        } else {
            (shaped.width, ascent + shaped.ink_descent.max(0.0))
        };
        let dev_w = f32_to_u32_clamped((w + pad * 2.0).ceil().max(1.0));
        let dev_h = f32_to_u32_clamped((h + pad * 2.0).ceil().max(1.0));
        let mut coverage = vec![0u8; (dev_w * dev_h * 3) as usize];

        let baseline = (ascent + pad).round();
        let centre = pad + shaped.size / 2.0;
        for (pen, glyph) in shaped.pen_positions() {
            let g = self
                .rasterizer
                .rasterize_subpixel(glyph.face, glyph.id, shaped.size)?;
            if g.width == 0 || g.height == 0 {
                continue;
            }
            let (x, y) = if shaped.vertical {
                (centre + glyph.offset_x, pad + pen + glyph.offset_y)
            } else {
                (pad + pen + glyph.offset_x, baseline + glyph.offset_y)
            };
            let gx = f32_to_i32_clamped(x.round()) + g.left;
            let gy = f32_to_i32_clamped(y.round()) + g.top;
            blit_max(&mut coverage, (dev_w, dev_h), &g, (gx, gy));
        }

        Ok((GlyphMask::new(dev_w, dev_h, coverage.into()), ascent, pad))
    }

    /// `font`'s own metrics at its size; ascent and descent are the ones text measures with.
    pub fn font_metrics(&self, font: FontRef) -> AureaResult<FontMetrics> {
        let face = self.rasterizer.face(font)?;
        let (ascent, descent) = self.rasterizer.face_metrics(face, font.size);
        let metrics = self.rasterizer.face_data(face)?.metrics(font.size);
        Ok(FontMetrics {
            ascent,
            descent,
            ..metrics
        })
    }

    /// Whether `font`'s own face draws `ch`; when not, text falls back to another face for it.
    pub fn font_covers(&self, font: FontRef, ch: char) -> AureaResult<bool> {
        let face = self.rasterizer.face(font)?;
        Ok(self.rasterizer.face_data(face)?.has_char(ch))
    }

    /// Measure text dimensions, from the same shaping drawing uses.
    pub fn measure_text(&self, text: &str, font: FontRef) -> AureaResult<TextMetrics> {
        let shaped = self.shape(text, font)?;
        if shaped.vertical {
            return Ok(TextMetrics {
                width: shaped.size,
                height: shaped.width,
                ascent: 0.0,
                descent: 0.0,
                advance: shaped.width,
            });
        }
        Ok(TextMetrics {
            width: shaped.width,
            height: (shaped.ascent + shaped.descent).max(0.0),
            ascent: shaped.ascent,
            descent: shaped.descent,
            advance: shaped.width,
        })
    }

    /// `text` laid out in lines by `options`.
    pub fn layout(
        &self,
        text: &str,
        font: FontRef,
        options: LayoutOptions,
    ) -> AureaResult<TextLayout> {
        let key = RunKey::layout(text, font, options);
        if let Some(hit) = lock(&self.layouts).get(&key).cloned() {
            return Ok(hit);
        }
        let layout = layout_text(text, font, options, self)?;
        let bytes = layout_bytes(&layout);
        lock(&self.layouts).insert_sized(key, layout.clone(), bytes);
        Ok(layout)
    }

    /// A shaped glyph's grayscale coverage at `size`, for apps that composite text themselves.
    pub fn glyph_coverage(
        &self,
        face: FaceId,
        glyph: u16,
        size: f32,
    ) -> AureaResult<GlyphCoverage> {
        self.rasterizer.rasterize_gray(face, glyph, size)
    }

    /// A shaped glyph's outline at `size`, pen at the origin on the baseline; `None` when it has
    /// none.
    pub fn glyph_outline(&self, face: FaceId, glyph: u16, size: f32) -> AureaResult<Option<Path>> {
        Ok(glyph_outline(
            &*self.rasterizer.face_data(face)?,
            glyph,
            size,
        ))
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
    use crate::text::cache_stats;

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
    fn fonts_say_their_metrics_and_what_they_cover() {
        let r = TextRenderer::new();
        let f = font("");
        let m = r.font_metrics((&f).into()).expect("metrics");
        let measured = r.measure_text("Hello", (&f).into()).expect("measure");
        assert_eq!((m.ascent, m.descent), (measured.ascent, measured.descent));
        assert!(m.line_gap >= 0.0 && m.underline_thickness > 0.0 && m.underline_offset > 0.0);
        assert!(r.font_covers((&f).into(), 'A').expect("covers"));
        assert!(!r.font_covers((&f).into(), '\u{10FFFD}').expect("covers"));
        #[cfg(windows)]
        {
            // 350 of 2048 units, which terminals add to every row.
            let m = r
                .font_metrics((&font("Consolas")).into())
                .expect("consolas");
            assert!((m.line_gap - 24.0 * 350.0 / 2048.0).abs() < 0.01, "{m:?}");
            assert!(m.monospace);
        }
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
        assert_eq!(
            r.measure_text("abc", (&f).into()).expect("measure").width,
            shaped.width
        );
    }

    #[test]
    fn combining_marks_cost_no_advance() {
        let r = TextRenderer::new();
        let f = font("");
        let plain = r.measure_text("e", (&f).into()).expect("measure").width;
        let accented = r
            .measure_text("e\u{0301}", (&f).into())
            .expect("measure")
            .width;
        assert!((accented - plain).abs() < 0.5, "{accented} vs {plain}");
    }

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
        let shaped = r.shape("a\u{05D0}\u{05D1}", (&f).into()).expect("shape");
        let clusters: Vec<usize> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(clusters, [0, 3, 1]);
        assert!(shaped.glyphs[1].rtl && !shaped.glyphs[0].rtl);
    }

    #[cfg(windows)]
    #[test]
    fn features_switch_ligatures() {
        let r = TextRenderer::new();
        let on = Font::new("Calibri", 24.0);
        let off = on.clone().with_feature("liga", 0);
        let ligated = r.shape("fi", (&on).into()).expect("shape").glyphs.len();
        let apart = r.shape("fi", (&off).into()).expect("shape").glyphs.len();
        assert_eq!((ligated, apart), (1, 2));
    }

    #[cfg(windows)]
    #[test]
    fn variations_move_a_variable_font() {
        let r = TextRenderer::new();
        let light = Font::new("Bahnschrift", 24.0).with_variation("wght", 300.0);
        let heavy = light.clone().with_variation("wght", 700.0);
        let ink = |font: &Font| {
            let shaped = r.shape("H", font.into()).expect("shape");
            let g = shaped.glyphs[0];
            let cov = r.glyph_coverage(g.face, g.id, 24.0).expect("coverage");
            cov.coverage.iter().map(|&c| u32::from(c)).sum::<u32>()
        };
        let (light_ink, heavy_ink) = (ink(&light), ink(&heavy));
        assert!(heavy_ink > light_ink * 5 / 4, "{heavy_ink} vs {light_ink}");
    }

    #[test]
    fn vertical_text_runs_down_a_column() {
        let r = TextRenderer::new();
        let f = Font::new("", 24.0).with_vertical(true);
        let shaped = r.shape("日本", (&f).into()).expect("shape");
        assert!(shaped.vertical);
        assert!(shaped.glyphs.iter().all(|g| g.advance > 0.0));
        let m = r.measure_text("日本", (&f).into()).expect("measure");
        assert_eq!((m.width, m.ascent), (24.0, 0.0));
        assert!(m.height > 30.0);
        let (mask, ascent, _) = r.render_text_subpixel("日本", (&f).into()).expect("mask");
        assert_eq!(ascent, 0.0);
        assert!(mask.height > mask.width);
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

    #[test]
    fn drawing_the_same_text_twice_raises_the_hits() {
        let r = TextRenderer::new();
        let f = font("");
        let before = cache_stats();
        r.render_text_subpixel("cached", (&f).into()).expect("mask");
        let once = cache_stats();
        r.render_text_subpixel("cached", (&f).into()).expect("mask");
        let twice = cache_stats();
        assert!(once.mask.misses > before.mask.misses);
        assert!(twice.mask.hits > once.mask.hits, "{once:?} then {twice:?}");
        assert!(twice.mask.entries >= 1);
    }

    #[test]
    fn shaped_runs_count_their_glyph_bytes() {
        let r = TextRenderer::new();
        let f = font("");
        let shaped = r.shape("Hello", (&f).into()).expect("shape");
        let bytes = lock(&r.shaped).bytes();
        assert_eq!(bytes, shaped.glyphs.len() * size_of::<ShapedGlyph>());
    }
}
