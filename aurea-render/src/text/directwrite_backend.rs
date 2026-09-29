//! DirectWrite glyph rasterizer (Windows) — hinted ClearType subpixel coverage.
//!
//! This is what makes small text crisp: DirectWrite applies font hinting (stem
//! grid-fitting) and returns a ClearType 3x1 alpha texture, i.e. exactly the RGB
//! subpixel coverage our `GlyphMask` pipeline consumes. It is the same engine
//! VS Code, Windows Terminal, and the OS itself render text with.

use super::super::types::{FontStyle, FontWeight, TextMetrics};
use super::atlas::{GlyphBitmap, GlyphKey};
use super::platform::{FontRef, PlatformTextRasterizer, SubpixelGlyph};
use aurea_foundation::{AureaError, AureaResult, lock};
use std::collections::HashMap;
use std::mem::zeroed;
use std::ptr;
use std::sync::{Arc, Mutex};

use dwrote::{
    FontCollection, FontFace, FontStretch as DwStretch, FontStyle as DwStyle,
    FontWeight as DwWeight, GlyphRunAnalysis,
};
use winapi::um::dcommon::DWRITE_MEASURING_MODE_NATURAL;
use winapi::um::dwrite::{
    DWRITE_FONT_METRICS, DWRITE_GLYPH_RUN, DWRITE_RENDERING_MODE_NATURAL,
    DWRITE_TEXTURE_CLEARTYPE_3x1, IDWriteFontFace,
};

/// A `u64` fingerprint of the family + weight/style, so `resolve_face` never
/// allocates a `String` on a cache hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FaceKey {
    family_hash: u64,
    weight: FontWeight,
    style: FontStyle,
}

impl FaceKey {
    fn from_font(font: FontRef) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        font.family.hash(&mut hasher);
        Self {
            family_hash: hasher.finish(),
            weight: font.weight,
            style: font.style,
        }
    }
}

/// A resolved font face plus the metrics needed for layout.
struct FaceEntry {
    face: FontFace,
    units_per_em: f32,
    ascent: f32,
    descent: f32,
}

/// Families tried in order for a character the requested font has no glyph
/// for: UI text first, then CJK, then symbols.
const FALLBACK_FAMILIES: &[&str] = &[
    "Segoe UI",
    "Malgun Gothic",
    "Microsoft YaHei",
    "Yu Gothic UI",
    "Microsoft JhengHei",
    "Nirmala UI",
    "Leelawadee UI",
    "Ebrima",
    "Segoe UI Symbol",
    "Segoe UI Emoji",
    "Segoe UI Historic",
];

pub struct DirectWriteRasterizer {
    collection: FontCollection,
    faces: Mutex<HashMap<FaceKey, Arc<FaceEntry>>>,
    glyphs: Mutex<HashMap<GlyphKey, Arc<SubpixelGlyph>>>,
}

// DirectWrite objects are thread-safe (agile); access is additionally serialized
// through the caches' mutexes.
unsafe impl Send for DirectWriteRasterizer {}
unsafe impl Sync for DirectWriteRasterizer {}

impl DirectWriteRasterizer {
    pub fn new() -> AureaResult<Self> {
        let collection = FontCollection::system();
        Ok(Self {
            collection,
            faces: Mutex::new(HashMap::new()),
            glyphs: Mutex::new(HashMap::new()),
        })
    }

    fn resolve_face(&self, font: FontRef) -> AureaResult<Arc<FaceEntry>> {
        let key = FaceKey::from_font(font);
        if let Some(cached) = lock(&self.faces).get(&key).cloned() {
            return Ok(cached);
        }

        let weight = match font.weight {
            FontWeight::Bold => DwWeight::Bold,
            FontWeight::Normal => DwWeight::Regular,
        };
        let style = match font.style {
            FontStyle::Italic => DwStyle::Italic,
            FontStyle::Normal => DwStyle::Normal,
        };

        // Try the requested family, then sensible monospace/UI fallbacks.
        let candidates = [font.family, "Consolas", "Cascadia Mono", "Segoe UI"];
        let mut family = None;
        for name in candidates {
            if name.is_empty() {
                continue;
            }
            if let Ok(Some(f)) = self.collection.font_family_by_name(name) {
                family = Some(f);
                break;
            }
        }
        let family = family.ok_or(AureaError::RenderingFailed)?;

        let dw_font = family
            .first_matching_font(weight, DwStretch::Normal, style)
            .map_err(|_| AureaError::RenderingFailed)?;
        let face = dw_font.create_font_face();

        // Pull design metrics straight off the IDWriteFontFace COM object so we
        // do not depend on a particular dwrote wrapper shape.
        let mut fm: DWRITE_FONT_METRICS = unsafe { zeroed() };
        unsafe {
            let raw: *mut IDWriteFontFace = face.as_ptr();
            (*raw).GetMetrics(&mut fm);
        }

        #[allow(clippy::arc_with_non_send_sync)]
        let entry = Arc::new(FaceEntry {
            face,
            units_per_em: f32::from(fm.designUnitsPerEm.max(1)),
            ascent: f32::from(fm.ascent),
            descent: f32::from(fm.descent),
        });
        lock(&self.faces).insert(key, entry.clone());
        Ok(entry)
    }

    fn glyph_index(entry: &FaceEntry, char_code: u32) -> u16 {
        entry
            .face
            .glyph_indices(&[char_code])
            .ok()
            .and_then(|indices| indices.first().copied())
            .unwrap_or(0)
    }

    /// The face that draws `char_code` and its glyph there: the requested
    /// face when it has one, otherwise the first fallback family that does.
    /// Without this a character outside the font, like Hangul in Segoe UI,
    /// draws as an empty box.
    fn face_for_char(
        &self,
        font: FontRef,
        primary: Arc<FaceEntry>,
        char_code: u32,
    ) -> (Arc<FaceEntry>, u16) {
        let index = Self::glyph_index(&primary, char_code);
        let blank = char::from_u32(char_code).is_none_or(|c| c.is_whitespace() || c.is_control());
        if index != 0 || blank {
            return (primary, index);
        }
        for family in FALLBACK_FAMILIES {
            let fallback = FontRef { family, ..font };
            if let Ok(entry) = self.resolve_face(fallback) {
                let index = Self::glyph_index(&entry, char_code);
                if index != 0 {
                    return (entry, index);
                }
            }
        }
        (primary, 0)
    }

    fn glyph_advance(&self, entry: &FaceEntry, glyph_index: u16, size: f32) -> f32 {
        let metrics = entry.face.design_glyph_metrics(&[glyph_index], false);
        match metrics.ok().and_then(|metrics| metrics.first().copied()) {
            Some(m) => m.advanceWidth as f32 / entry.units_per_em * size,
            None => 0.0,
        }
    }
}

impl PlatformTextRasterizer for DirectWriteRasterizer {
    fn rasterize_glyph(&self, _font: FontRef, _char_code: u32) -> AureaResult<GlyphBitmap> {
        // The subpixel path is the supported one for DirectWrite; the legacy
        // grayscale bitmap path is not used by the tile renderer.
        Err(AureaError::RenderingFailed)
    }

    fn rasterize_subpixel(&self, font: FontRef, char_code: u32) -> AureaResult<Arc<SubpixelGlyph>> {
        let key = GlyphKey::new(font, char_code);
        if let Some(cached) = lock(&self.glyphs).get(&key).cloned() {
            return Ok(cached);
        }

        let (entry, glyph_index) = self.face_for_char(font, self.resolve_face(font)?, char_code);
        let advance = self.glyph_advance(&entry, glyph_index, font.size);

        let glyph_index_arr = [glyph_index];
        let face_ptr = unsafe { entry.face.as_ptr() };
        let run = DWRITE_GLYPH_RUN {
            fontFace: face_ptr,
            fontEmSize: font.size,
            glyphCount: 1,
            glyphIndices: glyph_index_arr.as_ptr(),
            glyphAdvances: ptr::null(),
            glyphOffsets: ptr::null(),
            isSideways: 0,
            bidiLevel: 0,
        };

        let analysis = GlyphRunAnalysis::create(
            &run,
            1.0,
            None,
            DWRITE_RENDERING_MODE_NATURAL,
            DWRITE_MEASURING_MODE_NATURAL,
            0.0,
            0.0,
        )
        .map_err(|_| AureaError::RenderingFailed)?;

        let bounds = analysis
            .get_alpha_texture_bounds(DWRITE_TEXTURE_CLEARTYPE_3x1)
            .map_err(|_| AureaError::RenderingFailed)?;

        let w = (bounds.right - bounds.left).max(0);
        let h = (bounds.bottom - bounds.top).max(0);

        let glyph = if w == 0 || h == 0 {
            SubpixelGlyph {
                width: 0,
                height: 0,
                left: 0,
                top: 0,
                advance,
                coverage: Vec::new(),
            }
        } else {
            let coverage = analysis
                .create_alpha_texture(DWRITE_TEXTURE_CLEARTYPE_3x1, bounds)
                .map_err(|_| AureaError::RenderingFailed)?;
            SubpixelGlyph {
                width: u32::try_from(w).expect("bounds width is non-negative"),
                height: u32::try_from(h).expect("bounds height is non-negative"),
                left: bounds.left,
                top: bounds.top,
                advance,
                coverage,
            }
        };

        let glyph = Arc::new(glyph);
        lock(&self.glyphs).insert(key, glyph.clone());
        Ok(glyph)
    }

    fn measure_text(&self, text: &str, font: FontRef) -> AureaResult<TextMetrics> {
        let entry = self.resolve_face(font)?;
        let scale = font.size / entry.units_per_em;
        let ascent = entry.ascent * scale;
        let descent = entry.descent * scale;

        let mut advance = 0.0f32;
        if !text.is_empty() {
            let cps: Vec<u32> = text.chars().map(u32::from).collect();
            let indices = entry
                .face
                .glyph_indices(&cps)
                .map_err(|_| AureaError::RenderingFailed)?;
            if !indices.is_empty() {
                let metrics = entry
                    .face
                    .design_glyph_metrics(&indices, false)
                    .map_err(|_| AureaError::RenderingFailed)?;
                for ((m, &index), &cp) in metrics.iter().zip(&indices).zip(&cps) {
                    advance += if index == 0 {
                        // Drawn from a fallback face, so measured there too.
                        let (face, index) = self.face_for_char(font, entry.clone(), cp);
                        self.glyph_advance(&face, index, font.size)
                    } else {
                        m.advanceWidth as f32 * scale
                    };
                }
            }
        }

        Ok(TextMetrics {
            width: advance,
            height: (ascent + descent).max(0.0),
            ascent,
            descent,
            advance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Font;

    #[test]
    fn hangul_in_segoe_ui_comes_from_a_fallback() {
        let r = DirectWriteRasterizer::new().expect("directwrite");
        let font = Font::new("Segoe UI", 14.0);
        let font: FontRef = (&font).into();
        let primary = r.resolve_face(font).expect("segoe ui");
        let (face, index) = r.face_for_char(font, primary.clone(), u32::from('가'));
        assert_ne!(index, 0);
        assert!(!Arc::ptr_eq(&face, &primary));

        let glyph = r.rasterize_subpixel(font, u32::from('가')).expect("glyph");
        assert!(glyph.coverage.iter().any(|&c| c > 0));
    }
}
