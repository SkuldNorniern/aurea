//! Font files found on disk, drawn from their outlines: the backend everywhere but Windows.

use crate::numeric::f32_to_u8_clamped;
use crate::text::fonts;
use crate::text::glyph::{FaceData, FaceId, FontBytes, GlyphCoverage, GlyphKey};
use crate::text::outline::outline_coverage;
use crate::text::platform::{FontRef, PlatformTextRasterizer, SubpixelGlyph, variation_hash};
use crate::text::system::{Installed, WeakBytes, file_bytes};
use crate::text::{CacheKind, GLYPH_BYTES, GLYPH_CAP, LruCache, budget, shared};
use crate::types::{FontStyle, FontVariation, FontWeight};
use aurea_foundation::{AureaError, AureaResult, lock};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, Weak};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FontKey {
    family_hash: u64,
    weight: FontWeight,
    style: FontStyle,
    variations: u64,
    generation: u64,
}

impl FontKey {
    fn from_font(font: FontRef) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        for c in font.family.chars().flat_map(char::to_lowercase) {
            c.hash(&mut hasher);
        }
        Self {
            family_hash: hasher.finish(),
            weight: font.weight,
            style: font.style,
            variations: font.variation_hash(),
            generation: fonts::generation(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum FaceSource {
    /// A file, and which of its faces.
    File(PathBuf, u32),
    Embedded,
    Registered(u32),
}

/// How to build a face again, so one evicted from [`VARIED_FACES`] keeps its id.
struct Recipe {
    source: FaceSource,
    variations: Vec<FontVariation>,
}

/// Variation faces built at once. Each holds a parsed font, so without a bound an animated axis
/// would grow memory with every value it passes through.
const VARIED_FACES: usize = 64;

#[derive(Default)]
struct Ids {
    by_key: HashMap<(FaceSource, u64), FaceId>,
    /// Indexed by face id. Ids are handed out in order and never reused, so a cache keyed by one
    /// keeps meaning the same face after it is evicted.
    recipes: Vec<Recipe>,
}

pub struct FontFileRasterizer {
    /// The installed fonts. Read on the first family asked for by name; text that names none never
    /// pays for it.
    system: Installed,
    ids: Mutex<Ids>,
    /// Faces without variations. Few, and fallbacks rely on them, so they are never evicted.
    pinned: Mutex<HashMap<FaceId, Arc<FaceData>>>,
    /// Faces with variations, the least recently used built again when next needed.
    varied: Arc<Mutex<LruCache<FaceId, Arc<FaceData>>>>,
    /// Font file bytes by file. Weak, so the last face using a file frees it.
    file_bytes: Mutex<HashMap<PathBuf, WeakBytes>>,
    /// The face of each file asked for as a fallback, or `None` when it did not load.
    file_faces: Mutex<HashMap<PathBuf, Option<FaceId>>>,
    font_cache: Mutex<LruCache<FontKey, FaceId>>,
    subpixel_cache: Arc<Mutex<LruCache<GlyphKey, Arc<SubpixelGlyph>>>>,
}

static EMBEDDED: LazyLock<Arc<Vec<u8>>> =
    LazyLock::new(|| Arc::new(include_bytes!("../../fonts/Tuffy.ttf").to_vec()));

impl FontFileRasterizer {
    pub fn new() -> Self {
        Self {
            system: Installed::new(),
            ids: Mutex::new(Ids::default()),
            pinned: Mutex::new(HashMap::new()),
            varied: shared(CacheKind::Face, LruCache::new(VARIED_FACES)),
            file_bytes: Mutex::new(HashMap::new()),
            file_faces: Mutex::new(HashMap::new()),
            font_cache: Mutex::new(LruCache::new(32)),
            subpixel_cache: shared(
                CacheKind::Glyph,
                LruCache::with_budget(GLYPH_CAP, budget(CacheKind::Glyph, GLYPH_BYTES)),
            ),
        }
    }

    fn get(&self, face: FaceId) -> AureaResult<Arc<FaceData>> {
        if let Some(pinned) = lock(&self.pinned).get(&face) {
            return Ok(pinned.clone());
        }
        if let Some(hit) = lock(&self.varied).get(&face).cloned() {
            return Ok(hit);
        }
        let (source, variations) = {
            let ids = lock(&self.ids);
            let recipe = ids
                .recipes
                .get(face.0 as usize)
                .ok_or(AureaError::RenderingFailed)?;
            (recipe.source.clone(), recipe.variations.clone())
        };
        let built = self
            .build(&source, &variations)
            .ok_or(AureaError::RenderingFailed)?;
        let built = Arc::new(built);
        lock(&self.varied).insert(face, built.clone());
        Ok(built)
    }

    /// The bytes of `source`, shared with every face of it still alive, and which face it is.
    fn bytes(&self, source: &FaceSource) -> Option<(FontBytes, u32)> {
        match source {
            FaceSource::Embedded => Some((EMBEDDED.clone(), 0)),
            FaceSource::Registered(id) => fonts::get(*id).map(|f| (f.bytes.clone(), f.index)),
            FaceSource::File(path, index) => {
                let mut shared = lock(&self.file_bytes);
                if let Some(bytes) = shared.get(path).and_then(Weak::upgrade) {
                    return Some((bytes, *index));
                }
                let bytes = file_bytes(path)?;
                shared.retain(|_, weak| weak.strong_count() > 0);
                shared.insert(path.clone(), Arc::downgrade(&bytes));
                Some((bytes, *index))
            }
        }
    }
    fn build(&self, source: &FaceSource, variations: &[FontVariation]) -> Option<FaceData> {
        let (bytes, index) = self.bytes(source)?;
        FaceData::new(bytes, index, variations)
    }

    /// The id of `source` at `variations`, built now if it is new. A face with variations may be
    /// evicted later and is then built again under the same id.
    fn load(&self, source: FaceSource, variations: &[FontVariation]) -> Option<FaceId> {
        let key = (source, variation_hash(variations));
        let mut ids = lock(&self.ids);
        if let Some(&id) = ids.by_key.get(&key) {
            return Some(id);
        }
        let Some(face) = self.build(&key.0, variations) else {
            log::warn!("font: cannot load {:?}", key.0);
            return None;
        };
        let face = Arc::new(face);
        let id = FaceId(u32::try_from(ids.recipes.len()).ok()?);
        ids.recipes.push(Recipe {
            source: key.0.clone(),
            variations: variations.to_vec(),
        });
        ids.by_key.insert(key, id);
        if variations.is_empty() {
            lock(&self.pinned).insert(id, face);
        } else {
            lock(&self.varied).insert(id, face);
        }
        Some(id)
    }
    /// The first installed fallback face, if the system has one.
    fn system_fallback(&self) -> Option<FaceId> {
        self.system
            .fallbacks()
            .iter()
            .find_map(|path| self.load_file(path))
    }

    /// The first installed fallback face that draws `ch`.
    fn fallback_covering(&self, ch: char) -> Option<FaceId> {
        self.system
            .fallbacks()
            .iter()
            .filter_map(|path| self.load_file(path))
            .find(|&face| self.has_glyph(face, ch))
    }

    /// The face of the file at `path`, with no variations. A file that fails is not read again.
    fn load_file(&self, path: &Path) -> Option<FaceId> {
        if let Some(&hit) = lock(&self.file_faces).get(path) {
            return hit;
        }
        let face = self.load(FaceSource::File(path.to_path_buf(), 0), &[]);
        lock(&self.file_faces).insert(path.to_path_buf(), face);
        face
    }

    fn has_glyph(&self, face: FaceId, ch: char) -> bool {
        self.get(face).is_ok_and(|f| f.has_char(ch))
    }

    fn load_for_key(&self, font: FontRef) -> AureaResult<FaceId> {
        if let Some(face) = fonts::find(font)
            && let Some(id) =
                self.load(FaceSource::Registered(face.id), &face.info.variations(font))
        {
            return Ok(id);
        }
        if let Some(id) = self.load_installed(font) {
            return Ok(id);
        }
        self.system_fallback()
            .or_else(|| self.load(FaceSource::Embedded, &[]))
            .ok_or(AureaError::RenderingFailed)
    }
    /// The installed face of `font`, by the names faces give themselves.
    fn load_installed(&self, font: FontRef) -> Option<FaceId> {
        let found = self.system.find(font)?;
        self.load(FaceSource::File(found.path, found.index), &found.variations)
    }
}

impl Default for FontFileRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTextRasterizer for FontFileRasterizer {
    fn face(&self, font: FontRef) -> AureaResult<FaceId> {
        let key = FontKey::from_font(font);
        if let Some(&hit) = lock(&self.font_cache).get(&key) {
            return Ok(hit);
        }
        let id = self.load_for_key(font)?;
        lock(&self.font_cache).insert(key, id);
        Ok(id)
    }

    fn face_for_char(&self, font: FontRef, primary: FaceId, ch: char) -> FaceId {
        if ch.is_whitespace() || ch.is_control() || self.has_glyph(primary, ch) {
            return primary;
        }
        if let Some(face) = fonts::covering(font, ch)
            && let Some(id) =
                self.load(FaceSource::Registered(face.id), &face.info.variations(font))
        {
            return id;
        }
        self.fallback_covering(ch).unwrap_or(primary)
    }

    fn face_data(&self, face: FaceId) -> AureaResult<Arc<FaceData>> {
        self.get(face)
    }

    fn system_families(&self) -> Vec<String> {
        self.system.families()
    }

    fn face_metrics(&self, face: FaceId, size: f32) -> (f32, f32) {
        self.get(face).map_or((size * 0.8, size * 0.2), |f| {
            let m = f.metrics(size);
            (m.ascent, m.descent)
        })
    }

    fn rasterize_subpixel(
        &self,
        face: FaceId,
        glyph: u16,
        size: f32,
    ) -> AureaResult<Arc<SubpixelGlyph>> {
        let key = GlyphKey::new(face, glyph, size);
        if let Some(hit) = lock(&self.subpixel_cache).get(&key).cloned() {
            return Ok(hit);
        }

        let glyph = outline_subpixel(self.get(face)?.as_ref(), glyph, size);
        let bytes = glyph.coverage.len();
        let g = Arc::new(glyph);
        lock(&self.subpixel_cache).insert_sized(key, g.clone(), bytes);
        Ok(g)
    }

    fn rasterize_gray(&self, face: FaceId, glyph: u16, size: f32) -> AureaResult<GlyphCoverage> {
        Ok(outline_coverage(self.get(face)?.as_ref(), glyph, size, 1.0))
    }
}

fn outline_subpixel(data: &FaceData, glyph: u16, size: f32) -> SubpixelGlyph {
    let cov = outline_coverage(data, glyph, size, 3.0);
    if cov.width == 0 || cov.height == 0 {
        return SubpixelGlyph {
            width: 0,
            height: 0,
            left: 0,
            top: 0,
            coverage: Vec::new(),
        };
    }
    let left = cov.left.div_euclid(3);
    let shift = (cov.left - left * 3).unsigned_abs() as usize;
    let dev_w = (shift + cov.width as usize).div_ceil(3);
    let (sub_w, dev_h) = (dev_w * 3, cov.height as usize);
    let mut acc = vec![0f32; sub_w * dev_h];
    for (y, row) in cov.coverage.chunks_exact(cov.width as usize).enumerate() {
        for (x, &c) in row.iter().enumerate() {
            acc[y * sub_w + shift + x] = f32::from(c) / 255.0;
        }
    }
    SubpixelGlyph {
        width: u32::try_from(dev_w).expect("glyph width fits in u32"),
        height: cov.height,
        left,
        top: cov.top,
        coverage: lcd_filter(&acc, sub_w, dev_h),
    }
}

fn lcd_filter(acc: &[f32], sub_w: usize, dev_h: usize) -> Vec<u8> {
    const FILT: [f32; 5] = [
        8.0 / 256.0,
        77.0 / 256.0,
        86.0 / 256.0,
        77.0 / 256.0,
        8.0 / 256.0,
    ];
    let mut coverage = vec![0u8; sub_w * dev_h];
    for y in 0..dev_h {
        let row = y * sub_w;
        for x in 0..sub_w {
            let s: f32 = FILT
                .iter()
                .enumerate()
                .map(|(k, w)| {
                    let xi = x as isize + k as isize - 2;
                    if xi >= 0 && xi.unsigned_abs() < sub_w {
                        acc[row + xi.unsigned_abs()] * w
                    } else {
                        0.0
                    }
                })
                .sum();
            coverage[y * sub_w + x] = f32_to_u8_clamped((s * 255.0).round());
        }
    }
    coverage
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{KINDS, Measured, Registered, Registry, trim_registry};
    use crate::types::Font;
    use std::env::temp_dir;
    use std::fs::{create_dir_all, remove_dir_all, remove_file, write};
    use std::process::id;

    fn variation(value: f32) -> FontVariation {
        FontVariation {
            tag: *b"wght",
            value,
        }
    }

    #[test]
    fn variation_faces_stay_bounded_and_come_back_under_their_id() {
        let r = FontFileRasterizer::new();
        let plain = r.load(FaceSource::Embedded, &[]).expect("plain face");
        let first = r
            .load(FaceSource::Embedded, &[variation(100.0)])
            .expect("varied face");
        let h = r.get(first).expect("face").glyph_index('H');
        let before = r.rasterize_gray(first, h, 24.0).expect("coverage");
        for i in 0..500u16 {
            let id = r
                .load(FaceSource::Embedded, &[variation(f32::from(i) + 1000.0)])
                .expect("varied face");
            assert_ne!(id, first, "ids are not reused");
            assert!(lock(&r.varied).len() <= VARIED_FACES);
        }
        assert!(
            lock(&r.varied).get(&first).is_none(),
            "the oldest face was evicted"
        );
        assert_eq!(
            r.load(FaceSource::Embedded, &[variation(100.0)]),
            Some(first),
            "the same variation keeps its id"
        );
        let after = r.rasterize_gray(first, h, 24.0).expect("coverage");
        assert_eq!(
            (after.width, after.height, after.left, after.top),
            (before.width, before.height, before.left, before.top)
        );
        assert_eq!(after.coverage, before.coverage);
        assert_eq!(
            r.load(FaceSource::Embedded, &[]),
            Some(plain),
            "plain faces are never evicted"
        );
        assert!(lock(&r.pinned).contains_key(&plain));
    }

    #[cfg(feature = "system-fonts")]
    #[test]
    fn faces_of_one_file_share_its_bytes_until_the_last_is_dropped() {
        let path = temp_dir().join(format!("aurea-shared-{}.ttf", id()));
        write(&path, EMBEDDED.as_slice()).expect("write font");
        let source = FaceSource::File(path.clone(), 0);
        let r = FontFileRasterizer::new();
        let plain = r.load(source.clone(), &[]).expect("plain face");
        let varied = r
            .load(source.clone(), &[variation(700.0)])
            .expect("varied face");
        let bytes_at = |face: FaceId| r.face_data(face).expect("data").bytes().as_ptr();
        assert_eq!(bytes_at(plain), bytes_at(varied));
        let weak = lock(&r.file_bytes).get(&path).cloned().expect("tracked");
        assert!(weak.upgrade().is_some());
        drop(r);
        let _ = remove_file(&path);
        assert!(
            weak.upgrade().is_none(),
            "dropping the faces frees the file"
        );
    }

    #[test]
    fn trim_drops_variation_faces_and_they_are_built_again() {
        let r = FontFileRasterizer::new();
        let face = r
            .load(FaceSource::Embedded, &[variation(300.0)])
            .expect("varied face");
        let h = r.get(face).expect("face").glyph_index('H');
        let before = r.rasterize_gray(face, h, 24.0).expect("coverage");
        let tally = lock(&r.varied).tally.clone();
        let source = Arc::downgrade(&r.varied);
        let weak: Weak<Mutex<dyn Measured + Send>> = source;
        let reg = Registry {
            caches: vec![Registered {
                kind: CacheKind::Face,
                tally,
                cache: weak,
            }],
            retired: [(0, 0); KINDS],
        };
        trim_registry(&reg);
        assert_eq!(lock(&r.varied).len(), 0);
        let again = r.rasterize_gray(face, h, 24.0).expect("coverage");
        assert_eq!(again.coverage, before.coverage);
    }

    #[test]
    fn loads_a_font_or_fallback() {
        let r = FontFileRasterizer::new();
        let font = Font::new("__no_such_font__", 14.0);
        let face = r
            .face((&font).into())
            .expect("should fall back to a system font");
        assert!(r.face_metrics(face, 14.0).0 > 0.0);
        assert!(r.has_glyph(face, 'A'));
    }

    #[test]
    fn a_fallback_that_fails_to_load_is_read_once() {
        let root = temp_dir().join(format!("aurea-broken-fallback-{}", id()));
        create_dir_all(&root).expect("create font dir");
        let path = root.join("broken.ttf");
        write(&path, b"nope").expect("write");
        let r = FontFileRasterizer::new();
        assert_eq!(r.load_file(&path), None);
        write(&path, EMBEDDED.as_slice()).expect("write font");
        let again = r.load_file(&path);
        let fresh = FontFileRasterizer::new().load_file(&path);
        let _ = remove_file(&path);
        let _ = remove_dir_all(&root);
        assert_eq!(
            again, None,
            "the failure is kept, the file is not read again"
        );
        assert!(fresh.is_some(), "a new rasterizer reads the now valid file");
    }

    #[cfg(feature = "system-fonts")]
    #[test]
    fn a_missing_glyph_comes_from_a_fallback() {
        let r = FontFileRasterizer::new();
        let font = Font::new("__no_such_font__", 14.0);
        let primary = r.face((&font).into()).expect("some font");
        let covered = r.fallback_covering('가').is_some();
        let drawn = r.face_for_char((&font).into(), primary, '가');
        assert_eq!(r.has_glyph(drawn, '가'), covered);
        assert_eq!(r.face_for_char((&font).into(), primary, ' '), primary);
    }

    #[test]
    fn a_varied_face_is_drawn_from_its_outlines() {
        let r = FontFileRasterizer::new();
        let at = |weight: f32| {
            let font = Font::new("Bahnschrift", 24.0).with_variation("wght", weight);
            let face = r.face((&font).into()).expect("some face");
            let f = r.get(face).expect("loaded");
            (face, !f.coords().is_empty(), f.glyph_index('H'))
        };
        let ((light, varied, h), (heavy, _, _)) = (at(300.0), at(700.0));
        #[cfg(all(windows, feature = "system-fonts"))]
        assert!(varied, "Bahnschrift varies");
        if !varied {
            return;
        }
        assert_ne!(light, heavy);
        let ink = |face| {
            let g = r.rasterize_gray(face, h, 24.0).expect("gray");
            assert_eq!(g.coverage.len(), (g.width * g.height) as usize);
            g.coverage.iter().map(|&c| u32::from(c)).sum::<u32>()
        };
        assert!(ink(heavy) > ink(light) * 5 / 4);
        let sub = r.rasterize_subpixel(heavy, h, 24.0).expect("subpixel");
        assert_eq!(sub.coverage.len(), (sub.width * sub.height * 3) as usize);
        assert!(sub.coverage.iter().any(|&c| c > 200));
    }

    // Cambria and Cambria Math are two faces of one file, Arial Black a file of its own beside
    // arial.ttf, and Segoe UI Emoji beside segoeui.ttf.
    #[cfg(all(windows, feature = "system-fonts"))]
    #[test]
    fn the_exact_face_of_an_installed_font_is_found() {
        use skrifa::MetadataProvider;
        use skrifa::raw::types::NameId;

        let r = FontFileRasterizer::new();
        let family_of = |family: &str| {
            let font = Font::new(family, 14.0);
            let face = r.load_installed((&font).into())?;
            let data = r.get(face).expect("data");
            let parsed = skrifa::FontRef::from_index(data.bytes(), data.index()).expect("font");
            let name = parsed
                .localized_strings(NameId::FAMILY_NAME)
                .english_or_first()?;
            Some(name.chars().collect::<String>())
        };
        for family in ["Cambria Math", "Cambria", "Arial Black", "Segoe UI Emoji"] {
            assert_eq!(family_of(family).as_deref(), Some(family));
        }
        assert_eq!(family_of("__no_such_font__"), None);
    }

    #[cfg(all(windows, feature = "system-fonts"))]
    #[test]
    fn a_weight_picks_its_face_within_a_family() {
        use skrifa::raw::TableProvider;

        let r = FontFileRasterizer::new();
        let weight_of = |weight| {
            let font = Font::new("Segoe UI", 14.0).with_weight(weight);
            let face = r.load_installed((&font).into()).expect("segoe ui");
            let data = r.get(face).expect("data");
            let parsed = skrifa::FontRef::from_index(data.bytes(), data.index()).expect("font");
            parsed.os2().expect("weight table").us_weight_class()
        };
        assert!(weight_of(FontWeight::Bold) > weight_of(FontWeight::Normal));
    }

    #[cfg(all(target_os = "macos", feature = "system-fonts"))]
    #[test]
    fn arabic_hebrew_and_thai_fall_back_to_a_face_that_draws_them() {
        let r = FontFileRasterizer::new();
        for ch in ['\u{645}', '\u{5E2}', '\u{E01}'] {
            assert!(
                r.fallback_covering(ch).is_some(),
                "no fallback draws {ch:?}"
            );
        }
    }
}
