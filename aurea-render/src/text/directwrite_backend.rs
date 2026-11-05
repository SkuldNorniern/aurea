//! DirectWrite glyph rasterizer (Windows) — hinted ClearType subpixel coverage.

use super::super::types::{FontStyle, FontVariation, FontWeight};
use super::dwrite::{FontCollection, FontFace, FontFile};
use super::fonts;
use super::glyph::{FaceData, FaceId, FontBytes, GlyphKey};
use super::platform::{FontRef, PlatformTextRasterizer, SubpixelGlyph};
use super::{CacheKind, GLYPH_BYTES, GLYPH_CAP, LruCache, shared};
use aurea_foundation::{AureaError, AureaResult, lock};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FaceKey {
    family_hash: u64,
    weight: FontWeight,
    style: FontStyle,
    variations: u64,
    generation: u64,
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
            generation: fonts::generation(),
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
    /// Set for a registered font, which has no file DirectWrite could read back.
    registered: Option<FontBytes>,
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

/// Entries kept in the char-to-face cache before it starts over.
const CHAR_FACE_CAP: usize = 4096;

pub struct DirectWriteRasterizer {
    collection: FontCollection,
    faces: Mutex<Vec<Arc<FaceEntry>>>,
    by_key: Mutex<HashMap<FaceKey, FaceId>>,
    glyphs: Arc<Mutex<LruCache<GlyphKey, Arc<SubpixelGlyph>>>>,
    /// Chosen face per (primary face, char, generation). A primary face already fixes the weight, style and
    /// variations the fallbacks are resolved with, so the pair fully determines the answer.
    char_faces: Mutex<HashMap<(FaceId, char, u64), FaceId>>,
    /// Font file bytes by path, shared by every face read from that file.
    file_bytes: Mutex<HashMap<PathBuf, Weak<Vec<u8>>>>,
}

impl DirectWriteRasterizer {
    pub fn new() -> AureaResult<Self> {
        Ok(Self {
            collection: FontCollection::system().ok_or(AureaError::RenderingFailed)?,
            faces: Mutex::new(Vec::new()),
            by_key: Mutex::new(HashMap::new()),
            glyphs: shared(
                CacheKind::Glyph,
                LruCache::with_budget(GLYPH_CAP, GLYPH_BYTES),
            ),
            char_faces: Mutex::new(HashMap::new()),
            file_bytes: Mutex::new(HashMap::new()),
        })
    }

    fn shared_bytes(&self, file: &FontFile) -> Option<Arc<Vec<u8>>> {
        let Some(path) = file.path() else {
            return file.bytes().map(Arc::new);
        };
        let mut cache = lock(&self.file_bytes);
        if let Some(bytes) = cache.get(&path).and_then(Weak::upgrade) {
            return Some(bytes);
        }
        let bytes = Arc::new(file.bytes()?);
        cache.retain(|_, weak| weak.strong_count() > 0);
        cache.insert(path, Arc::downgrade(&bytes));
        Some(bytes)
    }

    fn get(&self, face: FaceId) -> AureaResult<Arc<FaceEntry>> {
        lock(&self.faces)
            .get(face.0 as usize)
            .cloned()
            .ok_or(AureaError::RenderingFailed)
    }

    fn resolve(&self, font: FontRef) -> AureaResult<FaceEntry> {
        let family = candidates(font.family)
            .into_iter()
            .filter(|name| !name.is_empty())
            .find_map(|name| self.collection.family_by_name(name))
            .ok_or(AureaError::RenderingFailed)?;
        let face = family
            .matching_face(font.weight.number(), font.style == FontStyle::Italic)
            .ok_or(AureaError::RenderingFailed)?;
        Ok(entry(face, font.variations.to_vec(), None))
    }

    fn registered(face: &fonts::Face, font: FontRef) -> Option<FaceEntry> {
        let native = FontFace::from_bytes(face.bytes.clone(), face.index)?;
        Some(entry(
            native,
            face.variations(font),
            Some(face.bytes.clone()),
        ))
    }

    fn has_glyph(&self, face: FaceId, ch: char) -> bool {
        self.get(face)
            .is_ok_and(|entry| entry.face.glyph_index(ch).is_some())
    }
}

fn entry(
    face: FontFace,
    variations: Vec<FontVariation>,
    registered: Option<FontBytes>,
) -> FaceEntry {
    let face = if variations.is_empty() {
        face
    } else {
        face.varied(&variations).unwrap_or(face)
    };
    let (ascent, descent, units_per_em) = face.metrics();
    FaceEntry {
        face,
        ascent,
        descent,
        units_per_em,
        variations,
        data: OnceLock::new(),
        registered,
    }
}

impl PlatformTextRasterizer for DirectWriteRasterizer {
    fn face(&self, font: FontRef) -> AureaResult<FaceId> {
        let key = FaceKey::from_font(font);
        if let Some(&id) = lock(&self.by_key).get(&key) {
            return Ok(id);
        }
        let entry = match fonts::find(font).and_then(|f| Self::registered(&f, font)) {
            Some(entry) => entry,
            None => self.resolve(font)?,
        };
        let mut faces = lock(&self.faces);
        let id = FaceId(u32::try_from(faces.len()).map_err(|_| AureaError::RenderingFailed)?);
        faces.push(Arc::new(entry));
        lock(&self.by_key).insert(key, id);
        Ok(id)
    }

    fn face_for_char(&self, font: FontRef, primary: FaceId, ch: char) -> FaceId {
        if ch.is_whitespace() || ch.is_control() {
            return primary;
        }
        let key = (primary, ch, fonts::generation());
        if let Some(&face) = lock(&self.char_faces).get(&key) {
            return face;
        }
        let face = if self.has_glyph(primary, ch) {
            primary
        } else {
            let registered = fonts::covering(font, ch).map(|f| f.family.clone());
            registered
                .iter()
                .map(String::as_str)
                .chain(FALLBACK_FAMILIES.iter().copied())
                .filter_map(|family| self.face(FontRef { family, ..font }).ok())
                .filter(|&face| face != primary)
                .find(|&face| self.has_glyph(face, ch))
                .unwrap_or(primary)
        };
        let mut cache = lock(&self.char_faces);
        if cache.len() >= CHAR_FACE_CAP {
            cache.clear();
        }
        cache.insert(key, face);
        face
    }

    fn face_data(&self, face: FaceId) -> AureaResult<Arc<FaceData>> {
        let entry = self.get(face)?;
        entry
            .data
            .get_or_init(|| {
                let bytes = match &entry.registered {
                    Some(bytes) => bytes.clone(),
                    None => {
                        let file = entry.face.file()?;
                        self.shared_bytes(&file)?
                    }
                };
                FaceData::new(bytes, entry.face.index(), &entry.variations).map(Arc::new)
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
        let glyph = self
            .get(face)?
            .face
            .subpixel_glyph(glyph, size)
            .ok_or(AureaError::RenderingFailed)?;
        let bytes = glyph.coverage.len();
        let glyph = Arc::new(glyph);
        lock(&self.glyphs).insert_sized(key, glyph.clone(), bytes);
        Ok(glyph)
    }
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
    fn numeric_face_selection_distinguishes_light_regular_and_semibold() {
        let raster = DirectWriteRasterizer::new().expect("directwrite");
        let font = Font::new("Segoe UI", 18.0);
        let light = font.clone().with_weight(FontWeight::from_number(300));
        let semibold = font.clone().with_weight(FontWeight::from_number(600));
        let regular_face = raster.face((&font).into()).expect("regular");
        let light_face = raster.face((&light).into()).expect("light");
        let semibold_face = raster.face((&semibold).into()).expect("semibold");
        let data = |face| raster.face_data(face).expect("face bytes");
        let weight = |face| {
            let data = data(face);
            use skrifa::raw::TableProvider;
            let parsed = skrifa::FontRef::from_index(data.bytes(), data.index()).expect("font");
            parsed.os2().expect("weight table").us_weight_class()
        };
        assert!(weight(light_face) < weight(regular_face));
        assert!(weight(semibold_face) > weight(regular_face));
    }

    #[test]
    fn generic_names_pick_the_system_faces() {
        assert_eq!(candidates("Sans")[1], "Segoe UI");
        assert_eq!(candidates("monospace")[0], "Cascadia Mono");
        assert_eq!(candidates("Malgun Gothic")[0], "Malgun Gothic");
    }

    #[test]
    fn faces_of_one_collection_file_share_bytes() {
        let r = DirectWriteRasterizer::new().expect("directwrite");
        // Cambria and Cambria Math are two faces of cambria.ttc.
        if r.collection.family_by_name("Cambria Math").is_none() {
            return;
        }
        let text = Font::new("Cambria", 14.0);
        let math = Font::new("Cambria Math", 14.0);
        let a = r.face((&text).into()).expect("cambria");
        let b = r.face((&math).into()).expect("cambria math");
        assert_ne!(a, b);
        let da = r.face_data(a).expect("bytes");
        let db = r.face_data(b).expect("bytes");
        assert_eq!(da.bytes().as_ptr(), db.bytes().as_ptr());
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
    fn each_char_keeps_its_face_after_the_cache_warms() {
        let _still = fonts::TEST_GUARD.read();
        let r = DirectWriteRasterizer::new().expect("directwrite");
        let font = Font::new("Segoe UI", 14.0);
        let font: FontRef = (&font).into();
        let primary = r.face(font).expect("segoe ui");
        let text = "Hi 안녕 🙂 세계";
        let cold: Vec<FaceId> = text
            .chars()
            .map(|ch| r.face_for_char(font, primary, ch))
            .collect();
        let warm: Vec<FaceId> = text
            .chars()
            .map(|ch| r.face_for_char(font, primary, ch))
            .collect();
        assert_eq!(cold, warm);
    }

    #[test]
    fn shaping_reads_the_face_directwrite_draws() {
        let r = DirectWriteRasterizer::new().expect("directwrite");
        let font = Font::new("Segoe UI", 14.0);
        let face = r.face((&font).into()).expect("segoe ui");
        let data = r.face_data(face).expect("font file");
        let ours = data
            .shaping_font()
            .charmap()
            .map_unicode('A')
            .map(|g| g.to_u32());
        let dw = r.get(face).expect("face").face.glyph_index('A');
        assert_eq!(ours, dw.map(u32::from));
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
