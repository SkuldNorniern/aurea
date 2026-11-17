//! Text rasterization orchestration.

mod cache;
mod mask;

use crate::geometry::Path;
use crate::text::file_backend::FontFileRasterizer;
use crate::text::glyph::{FaceData, FaceId, GlyphCoverage};
use crate::text::layout::{LayoutOptions, TextLayout, layout_text};
use crate::text::outline::glyph_outline;
use crate::text::shape::{ShapedGlyph, ShapedText, shape_line};
use crate::text::system::native_rasterizer;
use crate::text::{CacheKind, LAYOUT_BYTES, MASK_BYTES, SHAPED_BYTES, layout_bytes};
use crate::types::{
    Font, FontFeature, FontMetrics, FontStyle, FontVariation, FontWeight, GlyphMask, TextMetrics,
};
use aurea_foundation::AureaResult;
use cache::{Ident, Memo, Mode};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::sync::Arc;

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

/// A run's mask, the ascent above its top edge, and the pad around its ink.
type RunMask = (GlyphMask, f32, f32);

/// Platform text rasterizer trait — the backend seam.
pub trait PlatformTextRasterizer: Send + Sync {
    /// The face for `font`: its family in its weight and style when registered or installed, else
    /// the first installed fallback.
    fn face(&self, font: FontRef) -> AureaResult<FaceId>;

    /// The face that draws `ch` in `font`: `primary` when it has the glyph, else a registered font
    /// that does, else the first fallback that does, else `primary`.
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

    /// The family names installed on the system, sorted.
    fn system_families(&self) -> Vec<String>;

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
    if let Some(native) = native_rasterizer() {
        return native;
    }
    Box::new(FontFileRasterizer::new())
}

/// Paragraphs kept in the layout cache before the oldest go.
const LAYOUT_CAP: usize = 256;
/// Runs kept in the shape and mask caches before the oldest go.
const RUN_CAP: usize = 512;

/// Text renderer: owns a backend, shapes text, and lays shaped glyphs into masks.
pub struct TextRenderer {
    rasterizer: Box<dyn PlatformTextRasterizer>,
    shaped: Memo<Arc<ShapedText>>,
    masks: Memo<RunMask>,
    layouts: Memo<TextLayout>,
}

impl TextRenderer {
    /// Creates a renderer with the platform's rasterizer.
    pub fn new() -> Self {
        Self::with_rasterizer(get_platform_rasterizer())
    }

    pub(crate) fn with_rasterizer(rasterizer: Box<dyn PlatformTextRasterizer>) -> Self {
        Self {
            rasterizer,
            shaped: Memo::new(CacheKind::Shape, RUN_CAP, SHAPED_BYTES),
            masks: Memo::new(CacheKind::Mask, RUN_CAP, MASK_BYTES),
            layouts: Memo::new(CacheKind::Layout, LAYOUT_CAP, LAYOUT_BYTES),
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
        let id = Ident::new(text, font, Mode::Run(rtl));
        if let Some(hit) = self.shaped.get(id, text) {
            return Ok(hit);
        }
        let shaped = Arc::new(shape_line(text, font, rtl, self.rasterizer.as_ref())?);
        let bytes = shaped.glyphs.len() * size_of::<ShapedGlyph>();
        self.shaped.put(id, text, shaped.clone(), bytes);
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
        let id = Ident::new(text, font, Mode::Run(rtl));
        if let Some(hit) = self.masks.get(id, text) {
            return Ok(hit);
        }
        let shaped = self.shape_directed(text, font, rtl)?;
        let run = mask::draw_run(self.rasterizer.as_ref(), &shaped)?;
        self.masks.put(id, text, run.clone(), run.0.coverage.len());
        Ok(run)
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
        let mode = Mode::Layout {
            max_width: options.max_width.map(f32::to_bits),
            align: options.align,
            line_height: options.line_height.to_bits(),
            word_break: options.word_break,
        };
        let id = Ident::new(text, font, mode);
        if let Some(hit) = self.layouts.get(id, text) {
            return Ok(hit);
        }
        let layout = layout_text(text, font, options, self)?;
        self.layouts
            .put(id, text, layout.clone(), layout_bytes(&layout));
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

    /// The rows a glyph paints as visible ink at `size`: the first, down from the baseline, and how
    /// many. Text draws from subpixel coverage, whose rows can sit apart from
    /// [`Self::glyph_coverage`]'s. `None` for a glyph with no ink.
    pub fn glyph_ink_rows(
        &self,
        face: FaceId,
        glyph: u16,
        size: f32,
    ) -> AureaResult<Option<(i32, u32)>> {
        let g = self.rasterizer.rasterize_subpixel(face, glyph, size)?;
        let Some((first, rows)) = mask::ink_rows(&g.coverage, g.width) else {
            return Ok(None);
        };
        Ok(Some((g.top + first, rows)))
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
    use crate::text::{cache_stats, fonts};

    #[test]
    fn a_mask_too_big_to_make_has_no_size() {
        assert_eq!(mask::mask_bytes(100, 20), Some(6000));
        assert_eq!(mask::mask_bytes(9000, 9000), Some(243_000_000));
        assert_eq!(mask::mask_bytes(20_000, 20_000), None);
        assert_eq!(mask::mask_bytes(u32::MAX, u32::MAX), None);
    }

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
    fn same_key_different_text_or_font_is_a_miss() {
        let f = font("");
        let id = Ident::new("abc", (&f).into(), Mode::Run(None));
        let memo = Memo::new(CacheKind::Shape, 8, SHAPED_BYTES);
        memo.put(id, "abc", 1u32, 0);
        assert_eq!(memo.get(id, "abc"), Some(1));
        assert_eq!(memo.get(id, "abd"), None);
        let other_font = Ident {
            font: id.font ^ 1,
            ..id
        };
        assert_eq!(memo.get(other_font, "abc"), None);
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
        #[cfg(all(windows, feature = "system-fonts"))]
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

    #[cfg(all(windows, feature = "system-fonts"))]
    #[test]
    fn features_switch_ligatures() {
        let r = TextRenderer::new();
        let on = Font::new("Calibri", 24.0);
        let off = on.clone().with_feature("liga", 0);
        let ligated = r.shape("fi", (&on).into()).expect("shape").glyphs.len();
        let apart = r.shape("fi", (&off).into()).expect("shape").glyphs.len();
        assert_eq!((ligated, apart), (1, 2));
    }

    #[cfg(all(windows, feature = "system-fonts"))]
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
        let _still = fonts::TEST_GUARD.read();
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
        assert_eq!(
            r.shaped.bytes(),
            shaped.glyphs.len() * size_of::<ShapedGlyph>() + "Hello".len()
        );
    }
}
