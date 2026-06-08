//! DirectWrite glyph rasterizer (Windows) — hinted ClearType subpixel coverage.
//!
//! This is what makes small text crisp: DirectWrite applies font hinting (stem
//! grid-fitting) and returns a ClearType 3x1 alpha texture, i.e. exactly the RGB
//! subpixel coverage our `GlyphMask` pipeline consumes. It is the same engine
//! VS Code, Windows Terminal, and the OS itself render text with.

use super::super::types::{Font, FontStyle, FontWeight, TextMetrics};
use super::atlas::{GlyphBitmap, GlyphKey};
use super::platform::{PlatformTextRasterizer, SubpixelGlyph};
use aurea_core::{AureaError, AureaResult};
use std::collections::HashMap;
use std::ptr;
use std::sync::{Arc, Mutex};

use dwrote::{
    FontCollection, FontStretch as DwStretch, FontStyle as DwStyle, FontWeight as DwWeight,
    GlyphRunAnalysis,
};
use winapi::um::dcommon::DWRITE_MEASURING_MODE_NATURAL;
use winapi::um::dwrite::{
    DWRITE_FONT_METRICS, DWRITE_GLYPH_RUN, DWRITE_RENDERING_MODE_NATURAL,
    DWRITE_TEXTURE_CLEARTYPE_3x1, IDWriteFontFace,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FaceKey {
    family: String,
    weight: FontWeight,
    style: FontStyle,
}

impl FaceKey {
    fn from_font(font: &Font) -> Self {
        Self {
            family: font.family.clone(),
            weight: font.weight,
            style: font.style,
        }
    }
}

/// A resolved font face plus the metrics needed for layout.
struct FaceEntry {
    face: dwrote::FontFace,
    units_per_em: f32,
    ascent: f32,
    descent: f32,
}

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

    fn resolve_face(&self, font: &Font) -> AureaResult<Arc<FaceEntry>> {
        let key = FaceKey::from_font(font);
        if let Some(cached) = aurea_core::lock(&self.faces).get(&key).cloned() {
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
        let candidates = [font.family.trim(), "Consolas", "Cascadia Mono", "Segoe UI"];
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
        let mut fm: DWRITE_FONT_METRICS = unsafe { std::mem::zeroed() };
        unsafe {
            let raw: *mut IDWriteFontFace = face.as_ptr();
            (*raw).GetMetrics(&mut fm);
        }

        let entry = Arc::new(FaceEntry {
            face,
            units_per_em: fm.designUnitsPerEm.max(1) as f32,
            ascent: fm.ascent as f32,
            descent: fm.descent as f32,
        });
        aurea_core::lock(&self.faces).insert(key, entry.clone());
        Ok(entry)
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
    fn rasterize_glyph(&self, _font: &Font, _char_code: u32) -> AureaResult<GlyphBitmap> {
        // The subpixel path is the supported one for DirectWrite; the legacy
        // grayscale bitmap path is not used by the tile renderer.
        Err(AureaError::RenderingFailed)
    }

    fn rasterize_subpixel(&self, font: &Font, char_code: u32) -> AureaResult<Arc<SubpixelGlyph>> {
        let key = GlyphKey::new(font, char_code);
        if let Some(cached) = aurea_core::lock(&self.glyphs).get(&key).cloned() {
            return Ok(cached);
        }

        let entry = self.resolve_face(font)?;
        let cp = [char_code];
        let indices = entry
            .face
            .glyph_indices(&cp)
            .map_err(|_| AureaError::RenderingFailed)?;
        let glyph_index = indices.first().copied().unwrap_or(0);
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
                width: w as u32,
                height: h as u32,
                left: bounds.left,
                top: bounds.top,
                advance,
                coverage,
            }
        };

        let glyph = Arc::new(glyph);
        aurea_core::lock(&self.glyphs).insert(key, glyph.clone());
        Ok(glyph)
    }

    fn measure_text(&self, text: &str, font: &Font) -> AureaResult<TextMetrics> {
        let entry = self.resolve_face(font)?;
        let scale = font.size / entry.units_per_em;
        let ascent = entry.ascent * scale;
        let descent = entry.descent * scale;

        let mut advance = 0.0f32;
        if !text.is_empty() {
            let cps: Vec<u32> = text.chars().map(|c| c as u32).collect();
            let indices = entry
                .face
                .glyph_indices(&cps)
                .map_err(|_| AureaError::RenderingFailed)?;
            if !indices.is_empty() {
                let metrics = entry
                    .face
                    .design_glyph_metrics(&indices, false)
                    .map_err(|_| AureaError::RenderingFailed)?;
                for m in &metrics {
                    advance += m.advanceWidth as f32 * scale;
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
