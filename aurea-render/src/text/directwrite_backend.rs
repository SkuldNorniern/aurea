//! DirectWrite glyph rasterizer (Windows) — hinted ClearType subpixel coverage.

use super::super::types::{FontStyle, FontVariation, FontWeight};
use super::glyph::{FaceData, FaceId, GlyphKey};
use super::platform::{FontRef, PlatformTextRasterizer, SubpixelGlyph};
use super::{GLYPH_BYTES, LruCache};
use aurea_foundation::{AureaError, AureaResult, lock};
use std::collections::HashMap;
use std::mem::zeroed;
use std::ptr;
use std::sync::{Arc, Mutex, OnceLock};

use dwrote::{
    FontCollection, FontFace, FontStretch as DwStretch, FontStyle as DwStyle,
    FontWeight as DwWeight, GlyphRunAnalysis,
};
use winapi::um::dcommon::DWRITE_MEASURING_MODE_NATURAL;
use winapi::um::dwrite::{
    DWRITE_FONT_METRICS, DWRITE_FONT_SIMULATIONS_NONE, DWRITE_GLYPH_RUN,
    DWRITE_RENDERING_MODE_NATURAL, DWRITE_TEXTURE_CLEARTYPE_3x1, IDWriteFontFace,
};
use winapi::um::dwrite_3::DWRITE_FONT_AXIS_VALUE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FaceKey {
    family_hash: u64,
    weight: FontWeight,
    style: FontStyle,
    variations: u64,
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
            variations: font.variation_hash(),
        }
    }
}

struct FaceEntry {
    face: FontFace,
    ascent: f32,
    descent: f32,
    units_per_em: f32,
    variations: Vec<FontVariation>,
    data: OnceLock<Option<Arc<FaceData>>>,
}

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
    faces: Mutex<Vec<Arc<FaceEntry>>>,
    by_key: Mutex<HashMap<FaceKey, FaceId>>,
    glyphs: Mutex<LruCache<GlyphKey, Arc<SubpixelGlyph>>>,
}

// SAFETY: DirectWrite factory objects and font faces are free-threaded, and
unsafe impl Send for DirectWriteRasterizer {}
unsafe impl Sync for DirectWriteRasterizer {}

impl DirectWriteRasterizer {
    pub fn new() -> AureaResult<Self> {
        Ok(Self {
            collection: FontCollection::system(),
            faces: Mutex::new(Vec::new()),
            by_key: Mutex::new(HashMap::new()),
            glyphs: Mutex::new(LruCache::with_budget(512, GLYPH_BYTES)),
        })
    }

    fn get(&self, face: FaceId) -> AureaResult<Arc<FaceEntry>> {
        lock(&self.faces)
            .get(face.0 as usize)
            .cloned()
            .ok_or(AureaError::RenderingFailed)
    }

    fn resolve(&self, font: FontRef) -> AureaResult<FaceEntry> {
        let weight = match font.weight {
            FontWeight::Bold => DwWeight::Bold,
            FontWeight::Normal => DwWeight::Regular,
        };
        let style = match font.style {
            FontStyle::Italic => DwStyle::Italic,
            FontStyle::Normal => DwStyle::Normal,
        };
        let family = candidates(font.family)
            .into_iter()
            .filter(|name| !name.is_empty())
            .find_map(|name| self.collection.font_family_by_name(name).ok().flatten())
            .ok_or(AureaError::RenderingFailed)?;
        let mut face = family
            .first_matching_font(weight, DwStretch::Normal, style)
            .map_err(|_| AureaError::RenderingFailed)?
            .create_font_face();
        if !font.variations.is_empty()
            && let Some(varied) = vary(&face, font.variations)
        {
            face = varied;
        }

        let mut fm: DWRITE_FONT_METRICS = unsafe { zeroed() };
        unsafe {
            let raw: *mut IDWriteFontFace = face.as_ptr();
            (*raw).GetMetrics(&mut fm);
        }
        Ok(FaceEntry {
            face,
            ascent: f32::from(fm.ascent),
            descent: f32::from(fm.descent),
            units_per_em: f32::from(fm.designUnitsPerEm.max(1)),
            variations: font.variations.to_vec(),
            data: OnceLock::new(),
        })
    }

    fn has_glyph(&self, face: FaceId, ch: char) -> bool {
        self.get(face).is_ok_and(|entry| {
            entry
                .face
                .glyph_indices(&[u32::from(ch)])
                .ok()
                .and_then(|indices| indices.first().copied())
                .is_some_and(|index| index != 0)
        })
    }
}

impl PlatformTextRasterizer for DirectWriteRasterizer {
    fn face(&self, font: FontRef) -> AureaResult<FaceId> {
        let key = FaceKey::from_font(font);
        if let Some(&id) = lock(&self.by_key).get(&key) {
            return Ok(id);
        }
        let entry = self.resolve(font)?;
        let mut faces = lock(&self.faces);
        let id = FaceId(u32::try_from(faces.len()).map_err(|_| AureaError::RenderingFailed)?);
        #[allow(clippy::arc_with_non_send_sync)]
        faces.push(Arc::new(entry));
        lock(&self.by_key).insert(key, id);
        Ok(id)
    }

    fn face_for_char(&self, font: FontRef, primary: FaceId, ch: char) -> FaceId {
        if ch.is_whitespace() || ch.is_control() || self.has_glyph(primary, ch) {
            return primary;
        }
        FALLBACK_FAMILIES
            .iter()
            .filter_map(|family| self.face(FontRef { family, ..font }).ok())
            .find(|&face| self.has_glyph(face, ch))
            .unwrap_or(primary)
    }

    fn face_data(&self, face: FaceId) -> AureaResult<Arc<FaceData>> {
        let entry = self.get(face)?;
        entry
            .data
            .get_or_init(|| {
                let file = entry.face.files().ok()?.into_iter().next()?;
                let bytes = Arc::new(file.font_file_bytes().ok()?);
                FaceData::new(bytes, entry.face.get_index(), &entry.variations).map(Arc::new)
            })
            .clone()
            .ok_or(AureaError::RenderingFailed)
    }

    fn face_metrics(&self, face: FaceId, size: f32) -> (f32, f32) {
        self.get(face).map_or((size * 0.8, size * 0.2), |e| {
            let scale = size / e.units_per_em;
            (e.ascent * scale, e.descent * scale)
        })
    }

    fn rasterize_subpixel(
        &self,
        face: FaceId,
        glyph: u16,
        size: f32,
    ) -> AureaResult<Arc<SubpixelGlyph>> {
        let key = GlyphKey::new(face, glyph, size);
        if let Some(cached) = lock(&self.glyphs).get(&key).cloned() {
            return Ok(cached);
        }

        let entry = self.get(face)?;
        let glyph_index_arr = [glyph];
        let face_ptr = unsafe { entry.face.as_ptr() };
        let run = DWRITE_GLYPH_RUN {
            fontFace: face_ptr,
            fontEmSize: size,
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
                coverage,
            }
        };

        let bytes = glyph.coverage.len();
        let glyph = Arc::new(glyph);
        lock(&self.glyphs).insert_sized(key, glyph.clone(), bytes);
        Ok(glyph)
    }
}

fn vary(face: &FontFace, variations: &[FontVariation]) -> Option<FontFace> {
    let mut axes = face.variations().ok()?;
    if axes.is_empty() {
        return None;
    }
    for v in variations {
        let tag = u32::from_le_bytes(v.tag);
        match axes.iter_mut().find(|a| a.axisTag == tag) {
            Some(axis) => axis.value = v.value,
            None => axes.push(DWRITE_FONT_AXIS_VALUE {
                axisTag: tag,
                value: v.value,
            }),
        }
    }
    face.create_font_face_with_variations(DWRITE_FONT_SIMULATIONS_NONE, &axes)
}

fn candidates(family: &str) -> [&str; 4] {
    match family.to_ascii_lowercase().as_str() {
        "" | "sans" | "sans-serif" | "system-ui" | "ui-sans-serif" => {
            ["Segoe UI Variable Text", "Segoe UI", "Arial", "Consolas"]
        }
        "serif" | "ui-serif" => ["Cambria", "Times New Roman", "Segoe UI", "Consolas"],
        "mono" | "monospace" | "ui-monospace" => {
            ["Cascadia Mono", "Consolas", "Courier New", "Segoe UI"]
        }
        _ => [family, "Segoe UI", "Consolas", "Cascadia Mono"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Font;

    #[test]
    fn generic_names_pick_the_system_faces() {
        assert_eq!(candidates("Sans")[1], "Segoe UI");
        assert_eq!(candidates("monospace")[0], "Cascadia Mono");
        assert_eq!(candidates("Malgun Gothic")[0], "Malgun Gothic");
    }

    #[test]
    fn hangul_in_segoe_ui_comes_from_a_fallback() {
        let r = DirectWriteRasterizer::new().expect("directwrite");
        let font = Font::new("Segoe UI", 14.0);
        let font: FontRef = (&font).into();
        let primary = r.face(font).expect("segoe ui");
        let face = r.face_for_char(font, primary, '가');
        assert_ne!(face, primary);
        assert!(r.has_glyph(face, '가'));
    }

    #[test]
    fn shaping_reads_the_face_directwrite_draws() {
        let r = DirectWriteRasterizer::new().expect("directwrite");
        let font = Font::new("Segoe UI", 14.0);
        let face = r.face((&font).into()).expect("segoe ui");
        let data = r.face_data(face).expect("font file");
        let ours = data.shaping_font().charmap().map_unicode('A').map(|g| g.to_u32());
        let dw = r
            .get(face)
            .expect("face")
            .face
            .glyph_indices(&[u32::from('A')])
            .expect("indices");
        assert_eq!(ours, Some(u32::from(dw[0])));
    }

    #[test]
    fn glyphs_at_many_sizes_stay_bounded() {
        let r = DirectWriteRasterizer::new().expect("directwrite");
        let font = Font::new("Segoe UI", 14.0);
        let face = r.face((&font).into()).expect("segoe ui");
        for step in 0..1024u16 {
            r.rasterize_subpixel(face, 58, 8.0 + f32::from(step) * 0.25)
                .expect("glyph");
        }
        let glyphs = lock(&r.glyphs);
        assert!(glyphs.len() <= 512);
        assert!(glyphs.bytes() <= GLYPH_BYTES);
    }
}
