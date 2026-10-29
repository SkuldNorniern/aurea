//! fontdue glyph rasterizer — cross-platform fallback backend.

use crate::numeric::{f32_to_i32_clamped, f32_to_u8_clamped};
use crate::text::glyph::{FaceData, FaceId, GlyphCoverage, GlyphKey};
use crate::text::outline::outline_coverage;
use crate::text::platform::{FontRef, PlatformTextRasterizer, SubpixelGlyph, variation_hash};
use crate::text::{CacheKind, GLYPH_BYTES, LruCache, shared};
use crate::types::{FontStyle, FontVariation, FontWeight};
use aurea_foundation::{AureaError, AureaResult, lock};
use fontdue::{Font, FontSettings};
use std::collections::HashMap;
#[cfg(not(target_os = "android"))]
use std::env::var;
#[cfg(target_os = "windows")]
use std::env::var_os;
use std::fs::{read, read_dir};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FontKey {
    family_hash: u64,
    weight: FontWeight,
    style: FontStyle,
    variations: u64,
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
        }
    }
}

#[cfg(target_os = "android")]
fn font_search_dirs() -> Vec<PathBuf> {
    vec![PathBuf::from("/system/fonts")]
}

#[cfg(not(target_os = "android"))]
fn font_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = var("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(root) = var_os("SYSTEMROOT") {
            dirs.push(PathBuf::from(root).join("Fonts"));
        } else {
            dirs.push(PathBuf::from("C:\\Windows\\Fonts"));
        }
        if let Ok(profile) = var("USERPROFILE") {
            let home = PathBuf::from(profile);
            dirs.push(home.join("AppData\\Local\\Microsoft\\Windows\\Fonts"));
            dirs.push(home.join("AppData\\Roaming\\Microsoft\\Windows\\Fonts"));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = var("HOME") {
            let h = PathBuf::from(home);
            dirs.push(h.join(".fonts"));
            dirs.push(h.join(".local/share/fonts"));
        }
    }

    dirs
}

fn fallback_stems() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        &[
            "sfns",
            "helveticaneue",
            "helvetica",
            "applesdgothicneo",
            "pingfang",
            "hiraginosansgb",
            "menlo",
        ]
    }
    #[cfg(target_os = "windows")]
    {
        &["segoeui", "arial", "malgun", "msyh", "yugothr", "consola"]
    }
    #[cfg(target_os = "android")]
    {
        &[
            "robotostaticregular",
            "robotoregular",
            "droidsans",
            "notosanscjkregular",
            "droidsansmono",
        ]
    }
    #[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
    {
        &[
            "dejavusans",
            "liberationsansregular",
            "notosansregular",
            "ubunturegular",
            "notosanscjkregular",
            "notosanscjkkrregular",
            "nanumgothic",
            "droidsansfallbackfull",
            "dejavusansmono",
            "liberationmonoregular",
        ]
    }
}

fn normalise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn file_stem(name: &str) -> &str {
    name.trim_end_matches(".ttf")
        .trim_end_matches(".TTF")
        .trim_end_matches(".otf")
        .trim_end_matches(".OTF")
        .trim_end_matches(".ttc")
        .trim_end_matches(".TTC")
}

fn is_font_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc")
}

type FontFile = (String, PathBuf);

fn scan_font_files(dirs: &[PathBuf]) -> Vec<FontFile> {
    let mut files = Vec::new();
    let mut pending = dirs.to_vec();
    while let Some(dir) = pending.pop() {
        let Ok(entries) = read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if is_font_file(&name) {
                files.push((normalise(file_stem(&name)), entry.path()));
            }
        }
    }
    files.sort_by(|a, b| a.1.cmp(&b.1));
    files
}

fn face_suffixes(weight: FontWeight, style: FontStyle) -> &'static [&'static str] {
    match (weight.number(), style) {
        (1..=149, FontStyle::Normal) => &["thin", "hairline", "light", "", "regular"],
        (150..=249, FontStyle::Normal) => &["extralight", "ultralight", "light", "", "regular"],
        (250..=349, FontStyle::Normal) => &["light", "semilight", "", "regular"],
        (450..=549, FontStyle::Normal) => &["medium", "", "regular", "bold"],
        (550..=649, FontStyle::Normal) => &["semibold", "demibold", "bold", "", "regular"],
        (650..=749, FontStyle::Normal) => &["bold", "", "regular"],
        (750..=849, FontStyle::Normal) => &["extrabold", "ultrabold", "bold", "", "regular"],
        (850..=1000, FontStyle::Normal) => &["black", "heavy", "extrabold", "bold", "", "regular"],
        (_, FontStyle::Normal) => &["", "regular", "book", "roman"],
        (1..=149, FontStyle::Italic) => &[
            "thinitalic",
            "hairlineitalic",
            "lightitalic",
            "italic",
            "",
            "regular",
        ],
        (150..=249, FontStyle::Italic) => &[
            "extralightitalic",
            "ultralightitalic",
            "lightitalic",
            "italic",
            "",
            "regular",
        ],
        (250..=349, FontStyle::Italic) => &["lightitalic", "lightoblique", "italic", "", "regular"],
        (450..=549, FontStyle::Italic) => {
            &["mediumitalic", "mediumoblique", "italic", "", "regular"]
        }
        (550..=649, FontStyle::Italic) => &[
            "semibolditalic",
            "demibolditalic",
            "bolditalic",
            "bold",
            "",
            "regular",
        ],
        (650..=749, FontStyle::Italic) => &["bolditalic", "boldoblique", "bold", "", "regular"],
        (750..=849, FontStyle::Italic) => &[
            "extrabolditalic",
            "ultrabolditalic",
            "bolditalic",
            "bold",
            "",
            "regular",
        ],
        (850..=1000, FontStyle::Italic) => &[
            "blackitalic",
            "heavyitalic",
            "extrabolditalic",
            "bolditalic",
            "bold",
            "",
            "regular",
        ],
        (_, FontStyle::Italic) => &["italic", "oblique", "", "regular"],
    }
}

fn find_font_file<'a>(
    family: &str,
    weight: FontWeight,
    style: FontStyle,
    files: &'a [FontFile],
) -> Option<&'a Path> {
    let want = normalise(family);
    if want.is_empty() {
        return None;
    }

    for suffix in face_suffixes(weight, style) {
        let name = format!("{want}{suffix}");
        if let Some((_, path)) = files.iter().find(|(stem, _)| *stem == name) {
            return Some(path);
        }
    }

    files
        .iter()
        .filter_map(|(stem, path)| {
            let score = if stem.starts_with(&want) || want.starts_with(stem.as_str()) {
                2
            } else if stem.contains(&want) {
                1
            } else {
                return None;
            };
            Some((score, stem.len(), path))
        })
        .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
        .map(|(_, _, path)| path.as_path())
}

struct Face {
    font: Arc<Font>,
    data: Arc<FaceData>,
    varied: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum FaceSource {
    File(PathBuf),
    Embedded,
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

pub struct FontDbTextRasterizer {
    dirs: Vec<PathBuf>,
    files: OnceLock<Vec<FontFile>>,
    ids: Mutex<Ids>,
    /// Faces without variations. Few, and fallbacks rely on them, so they are never evicted.
    pinned: Mutex<HashMap<FaceId, Arc<Face>>>,
    /// Faces with variations, the least recently used built again when next needed.
    varied: Arc<Mutex<LruCache<FaceId, Arc<Face>>>>,
    /// Font file bytes by file. Weak, so the last face using a file frees it.
    file_bytes: Mutex<HashMap<FaceSource, Weak<Vec<u8>>>>,
    fallbacks: Vec<OnceLock<Option<FaceId>>>,
    font_cache: Mutex<LruCache<FontKey, FaceId>>,
    subpixel_cache: Arc<Mutex<LruCache<GlyphKey, Arc<SubpixelGlyph>>>>,
}

static EMBEDDED: LazyLock<Arc<Vec<u8>>> =
    LazyLock::new(|| Arc::new(include_bytes!("../../fonts/Tuffy.ttf").to_vec()));

impl FontDbTextRasterizer {
    pub fn new() -> Self {
        Self {
            dirs: font_search_dirs(),
            files: OnceLock::new(),
            ids: Mutex::new(Ids::default()),
            pinned: Mutex::new(HashMap::new()),
            varied: shared(CacheKind::Face, LruCache::new(VARIED_FACES)),
            file_bytes: Mutex::new(HashMap::new()),
            fallbacks: fallback_stems().iter().map(|_| OnceLock::new()).collect(),
            font_cache: Mutex::new(LruCache::new(32)),
            subpixel_cache: shared(CacheKind::Glyph, LruCache::with_budget(512, GLYPH_BYTES)),
        }
    }

    fn files(&self) -> &[FontFile] {
        self.files.get_or_init(|| scan_font_files(&self.dirs))
    }

    fn get(&self, face: FaceId) -> AureaResult<Arc<Face>> {
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

    /// The bytes of `source`, shared with every face of it still alive.
    fn bytes(&self, source: &FaceSource) -> Option<Arc<Vec<u8>>> {
        match source {
            FaceSource::Embedded => Some(EMBEDDED.clone()),
            FaceSource::File(path) => {
                let mut shared = lock(&self.file_bytes);
                if let Some(bytes) = shared.get(source).and_then(Weak::upgrade) {
                    return Some(bytes);
                }
                let bytes = Arc::new(read(path).ok()?);
                shared.retain(|_, weak| weak.strong_count() > 0);
                shared.insert(source.clone(), Arc::downgrade(&bytes));
                Some(bytes)
            }
        }
    }

    fn build(&self, source: &FaceSource, variations: &[FontVariation]) -> Option<Face> {
        let bytes = self.bytes(source)?;
        let font = Font::from_bytes(
            &bytes[..],
            FontSettings {
                collection_index: 0,
                ..FontSettings::default()
            },
        )
        .ok()?;
        let data = Arc::new(FaceData::new(bytes, 0, variations)?);
        let varied = !data.coords().is_empty();
        Some(Face {
            font: Arc::new(font),
            data,
            varied,
        })
    }

    /// The id of `source` at `variations`, built now if it is new. A face with variations may be
    /// evicted later and is then built again under the same id.
    fn load(&self, source: FaceSource, variations: &[FontVariation]) -> Option<FaceId> {
        let key = (source, variation_hash(variations));
        let mut ids = lock(&self.ids);
        if let Some(&id) = ids.by_key.get(&key) {
            return Some(id);
        }
        let face = Arc::new(self.build(&key.0, variations)?);
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

    fn fallback(&self, slot: usize) -> Option<FaceId> {
        let stem = fallback_stems().get(slot)?;
        *self.fallbacks.get(slot)?.get_or_init(|| {
            let (_, path) = self.files().iter().find(|(s, _)| s == stem)?;
            self.load(FaceSource::File(path.clone()), &[])
        })
    }

    fn has_glyph(&self, face: FaceId, ch: char) -> bool {
        self.get(face)
            .is_ok_and(|f| f.font.lookup_glyph_index(ch) != 0)
    }

    fn load_for_key(&self, font: FontRef) -> AureaResult<FaceId> {
        if let Some(path) = find_font_file(font.family, font.weight, font.style, self.files())
            && let Some(id) = self.load(FaceSource::File(path.to_path_buf()), font.variations)
        {
            return Ok(id);
        }
        (0..self.fallbacks.len())
            .find_map(|slot| self.fallback(slot))
            .or_else(|| self.load(FaceSource::Embedded, &[]))
            .ok_or(AureaError::RenderingFailed)
    }
}

impl Default for FontDbTextRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTextRasterizer for FontDbTextRasterizer {
    fn face(&self, font: FontRef) -> AureaResult<FaceId> {
        let key = FontKey::from_font(font);
        if let Some(&hit) = lock(&self.font_cache).get(&key) {
            return Ok(hit);
        }
        let id = self.load_for_key(font)?;
        lock(&self.font_cache).insert(key, id);
        Ok(id)
    }

    fn face_for_char(&self, _font: FontRef, primary: FaceId, ch: char) -> FaceId {
        if ch.is_whitespace() || ch.is_control() || self.has_glyph(primary, ch) {
            return primary;
        }
        (0..self.fallbacks.len())
            .filter_map(|slot| self.fallback(slot))
            .find(|&face| self.has_glyph(face, ch))
            .unwrap_or(primary)
    }

    fn face_data(&self, face: FaceId) -> AureaResult<Arc<FaceData>> {
        Ok(self.get(face)?.data.clone())
    }

    fn face_metrics(&self, face: FaceId, size: f32) -> (f32, f32) {
        self.get(face)
            .ok()
            .and_then(|f| f.font.horizontal_line_metrics(size))
            .map_or((size * 0.8, size * 0.2), |lm| (lm.ascent, lm.descent.abs()))
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

        let f = self.get(face)?;
        let glyph = if f.varied {
            outline_subpixel(&f.data, glyph, size)
        } else {
            fontdue_subpixel(&f.font, glyph, size)
        };
        let bytes = glyph.coverage.len();
        let g = Arc::new(glyph);
        lock(&self.subpixel_cache).insert_sized(key, g.clone(), bytes);
        Ok(g)
    }

    fn rasterize_gray(&self, face: FaceId, glyph: u16, size: f32) -> AureaResult<GlyphCoverage> {
        let f = self.get(face)?;
        if f.varied {
            return Ok(outline_coverage(&f.data, glyph, size, 1.0));
        }
        let (m, coverage) = f.font.rasterize_indexed(glyph, size);
        let height = i32::try_from(m.height).expect("glyph height fits in i32");
        Ok(GlyphCoverage {
            width: u32::try_from(m.width).expect("glyph width fits in u32"),
            height: height.unsigned_abs(),
            left: m.xmin,
            top: -(height + m.ymin),
            coverage,
        })
    }
}

fn fontdue_subpixel(font: &Font, glyph: u16, size: f32) -> SubpixelGlyph {
    let (m, bmp) = font.rasterize_indexed(glyph, size * 3.0);
    let w3 = i32::try_from(m.width).expect("glyph width fits in i32");
    let h3 = i32::try_from(m.height).expect("glyph height fits in i32");

    if w3 <= 0 || h3 <= 0 {
        SubpixelGlyph {
            width: 0,
            height: 0,
            left: 0,
            top: 0,
            coverage: Vec::new(),
        }
    } else {
        let dev_w = ((w3 + 2) / 3).max(1).unsigned_abs() as usize;
        let dev_h = ((h3 + 2) / 3).max(1).unsigned_abs() as usize;
        let sub_w = dev_w * 3;
        let mut acc = vec![0f32; sub_w * dev_h];
        for sy in 0..h3 {
            let g_row = (sy * w3).unsigned_abs() as usize;
            let dev_row = (sy / 3).unsigned_abs() as usize;
            for sx in 0..w3 {
                acc[dev_row * sub_w + sx.unsigned_abs() as usize] +=
                    f32::from(bmp[g_row + sx.unsigned_abs() as usize]) / (255.0 * 3.0);
            }
        }
        for v in acc.iter_mut() {
            *v = v.min(1.0);
        }
        let coverage = lcd_filter(&acc, sub_w, dev_h);

        SubpixelGlyph {
            width: u32::try_from(dev_w).expect("glyph width fits in u32"),
            height: u32::try_from(dev_h).expect("glyph height fits in u32"),
            left: f32_to_i32_clamped((m.xmin as f32 / 3.0).round()),
            top: -f32_to_i32_clamped(((h3 + m.ymin) as f32 / 3.0).round()),
            coverage,
        }
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
    use crate::types::Font;
    use std::env::temp_dir;
    use std::fs::{create_dir_all, remove_dir_all, remove_file, write};
    use std::process::id;
    use std::slice::from_ref;

    use crate::text::{KINDS, Measured, Registered, Registry, trim_registry};

    fn variation(value: f32) -> FontVariation {
        FontVariation {
            tag: *b"wght",
            value,
        }
    }

    #[test]
    fn variation_faces_stay_bounded_and_come_back_under_their_id() {
        let r = FontDbTextRasterizer::new();
        let plain = r.load(FaceSource::Embedded, &[]).expect("plain face");
        let first = r
            .load(FaceSource::Embedded, &[variation(100.0)])
            .expect("varied face");
        let h = r.get(first).expect("face").font.lookup_glyph_index('H');
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

    #[test]
    fn faces_of_one_file_share_its_bytes_until_the_last_is_dropped() {
        let path = temp_dir().join(format!("aurea-shared-{}.ttf", id()));
        write(&path, EMBEDDED.as_slice()).expect("write font");
        let source = FaceSource::File(path.clone());
        let r = FontDbTextRasterizer::new();
        let plain = r.load(source.clone(), &[]).expect("plain face");
        let varied = r
            .load(source.clone(), &[variation(700.0)])
            .expect("varied face");
        let bytes_at = |face: FaceId| r.face_data(face).expect("data").bytes().as_ptr();
        assert_eq!(bytes_at(plain), bytes_at(varied));
        let weak = lock(&r.file_bytes).get(&source).cloned().expect("tracked");
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
        let r = FontDbTextRasterizer::new();
        let face = r
            .load(FaceSource::Embedded, &[variation(300.0)])
            .expect("varied face");
        let h = r.get(face).expect("face").font.lookup_glyph_index('H');
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
        let r = FontDbTextRasterizer::new();
        let font = Font::new("__no_such_font__", 14.0);
        let face = r
            .face((&font).into())
            .expect("should fall back to a system font");
        assert!(r.face_metrics(face, 14.0).0 > 0.0);
        assert!(r.has_glyph(face, 'A'));
    }

    #[test]
    fn a_missing_glyph_comes_from_a_fallback() {
        let r = FontDbTextRasterizer::new();
        let font = Font::new("__no_such_font__", 14.0);
        let primary = r.face((&font).into()).expect("some font");
        let covered = (0..r.fallbacks.len())
            .filter_map(|slot| r.fallback(slot))
            .any(|f| r.has_glyph(f, '가'));
        let drawn = r.face_for_char((&font).into(), primary, '가');
        assert_eq!(r.has_glyph(drawn, '가'), covered);
        assert_eq!(r.face_for_char((&font).into(), primary, ' '), primary);
    }

    #[test]
    fn a_varied_face_is_drawn_from_its_outlines() {
        let r = FontDbTextRasterizer::new();
        let at = |weight: f32| {
            let font = Font::new("Bahnschrift", 24.0).with_variation("wght", weight);
            let face = r.face((&font).into()).expect("some face");
            let f = r.get(face).expect("loaded");
            (face, f.varied, f.font.lookup_glyph_index('H'))
        };
        let ((light, varied, h), (heavy, _, _)) = (at(300.0), at(700.0));
        #[cfg(windows)]
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

    fn files(stems: &[&str]) -> Vec<FontFile> {
        stems
            .iter()
            .map(|s| ((*s).to_owned(), PathBuf::from(format!("{s}.ttf"))))
            .collect()
    }

    fn pick(family: &str, weight: FontWeight, style: FontStyle, list: &[FontFile]) -> String {
        find_font_file(family, weight, style, list)
            .and_then(Path::to_str)
            .unwrap_or("")
            .to_owned()
    }

    #[test]
    fn picks_numeric_face_weights_without_collapsing_them_to_bold() {
        let list = files(&[
            "notosanslight",
            "notosansregular",
            "notosansmedium",
            "notosanssemibold",
            "notosansbold",
            "notosansblackitalic",
        ]);
        for (weight, expected) in [
            (300, "notosanslight.ttf"),
            (400, "notosansregular.ttf"),
            (500, "notosansmedium.ttf"),
            (600, "notosanssemibold.ttf"),
            (700, "notosansbold.ttf"),
        ] {
            assert_eq!(
                pick(
                    "Noto Sans",
                    FontWeight::from_number(weight),
                    FontStyle::Normal,
                    &list
                ),
                expected
            );
        }
        assert_eq!(
            pick(
                "Noto Sans",
                FontWeight::from_number(900),
                FontStyle::Italic,
                &list
            ),
            "notosansblackitalic.ttf"
        );
        assert_eq!(
            pick(
                "Noto Sans",
                FontWeight::from_number(200),
                FontStyle::Normal,
                &list
            ),
            "notosanslight.ttf"
        );
    }

    #[test]
    fn picks_the_family_not_a_longer_one() {
        let list = files(&["dejavusansmono", "dejavusansbold", "dejavusans"]);
        let normal = (FontWeight::Normal, FontStyle::Normal);
        assert_eq!(
            pick("DejaVu Sans", normal.0, normal.1, &list),
            "dejavusans.ttf"
        );
        assert_eq!(pick("DejaVu", normal.0, normal.1, &list), "dejavusans.ttf");
    }

    #[test]
    fn picks_the_bold_face_and_falls_back_to_regular() {
        let list = files(&["notosansregular", "notosansbold", "dejavusans"]);
        assert_eq!(
            pick("Noto Sans", FontWeight::Bold, FontStyle::Normal, &list),
            "notosansbold.ttf"
        );
        assert_eq!(
            pick("Noto Sans", FontWeight::Normal, FontStyle::Normal, &list),
            "notosansregular.ttf"
        );
        assert_eq!(
            pick("DejaVu Sans", FontWeight::Bold, FontStyle::Italic, &list),
            "dejavusans.ttf"
        );
    }

    #[test]
    fn scans_nested_font_directories() {
        let root = temp_dir().join(format!("aurea-fonts-{}", id()));
        let nested = root.join("truetype").join("dejavu");
        create_dir_all(&nested).expect("create font dirs");
        write(nested.join("DejaVuSans.ttf"), b"").expect("write font");
        write(nested.join("README"), b"").expect("write readme");

        let found = scan_font_files(from_ref(&root));
        let _ = remove_dir_all(&root);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "dejavusans");
    }
}
