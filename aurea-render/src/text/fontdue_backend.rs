//! fontdue glyph rasterizer — cross-platform fallback backend.
//!
//! Memory design: we never parse all system fonts.  fontdb's
//! `load_system_fonts` reads every font file on disk to extract metadata —
//! on macOS that is 500+ files, easily 200-300 MB of page-cache pressure.
//! Instead we list file names under the standard font directories once, match
//! the family by name, and load only the files we actually draw with.

use crate::numeric::{f32_to_i32_clamped, f32_to_u8_clamped};
use crate::text::LruCache;
use crate::text::atlas::{GlyphBitmap, GlyphKey};
use crate::text::platform::{FontRef, PlatformTextRasterizer, SubpixelGlyph};
use crate::types::{FontStyle, FontWeight, TextMetrics};
use aurea_foundation::{AureaError, AureaResult, lock};
use fontdue::{Font, FontSettings};
use std::env::var;
#[cfg(target_os = "windows")]
use std::env::var_os;
use std::fs::{read, read_dir};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

// ── Font key ─────────────────────────────────────────────────────────────────

/// A `u64` fingerprint of the normalized family + weight/style, so
/// `resolve_font` never allocates a `String` on a cache hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FontKey {
    family_hash: u64,
    weight: FontWeight,
    style: FontStyle,
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
        }
    }
}

// ── Font directory / file search ──────────────────────────────────────────────

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

/// Families tried in order when the requested one is missing, or has no glyph
/// for a character. These are normalised file stems, so one name finds the
/// font wherever the system keeps it. Sans first, then CJK, then monospace.
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
    #[cfg(all(unix, not(target_os = "macos")))]
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

/// Normalise a name for fuzzy comparison: lowercase, strip spaces/hyphens.
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

/// A font file and its normalised stem.
type FontFile = (String, PathBuf);

/// Every font file under `dirs`. Font directories nest
/// (`/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`), so this walks all of
/// them. Symlinked directories are skipped, which also keeps loops out.
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
    // Directory order is arbitrary; sorting keeps the pick the same each run.
    files.sort_by(|a, b| a.1.cmp(&b.1));
    files
}

/// Stem suffixes that name a face, most usual first. A bold or italic face
/// that is not installed falls back to the regular one.
fn face_suffixes(weight: FontWeight, style: FontStyle) -> &'static [&'static str] {
    match (weight, style) {
        (FontWeight::Normal, FontStyle::Normal) => &["", "regular", "book", "roman"],
        (FontWeight::Bold, FontStyle::Normal) => &["bold", "", "regular"],
        (FontWeight::Normal, FontStyle::Italic) => &["italic", "oblique", "", "regular"],
        (FontWeight::Bold, FontStyle::Italic) => {
            &["bolditalic", "boldoblique", "bold", "", "regular"]
        }
    }
}

/// The file that best matches `family` in the requested face. Exact stems
/// win; otherwise the shortest stem that starts with the family, then the
/// shortest that contains it.
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

/// Load a fontdue Font from a file path (collection index 0).
fn load_font_file(path: &Path) -> Option<Font> {
    let data = read(path).ok()?;
    Font::from_bytes(
        data,
        FontSettings {
            collection_index: 0,
            ..FontSettings::default()
        },
    )
    .ok()
}

// ── Rasterizer ────────────────────────────────────────────────────────────────

pub struct FontDbTextRasterizer {
    dirs: Vec<PathBuf>,
    /// Font files under `dirs`, scanned on first use.
    files: OnceLock<Vec<FontFile>>,
    /// One slot per `fallback_stems()` entry, loaded only when a glyph is
    /// missing from everything before it: a CJK font is tens of MB parsed,
    /// and Latin text should never pay for it.
    fallbacks: Vec<OnceLock<Option<Arc<Font>>>>,
    /// LRU cap: 32 entries — typical UIs use fewer than 10 font variants.
    font_cache: Mutex<LruCache<FontKey, Arc<Font>>>,
    /// LRU cap: 512 entries — one per (font, char) pair; covers full ASCII +
    /// common Unicode ranges without unbounded growth on text-heavy views.
    subpixel_cache: Mutex<LruCache<GlyphKey, Arc<SubpixelGlyph>>>,
}

impl FontDbTextRasterizer {
    pub fn new() -> Self {
        Self {
            dirs: font_search_dirs(),
            files: OnceLock::new(),
            fallbacks: fallback_stems().iter().map(|_| OnceLock::new()).collect(),
            font_cache: Mutex::new(LruCache::new(32)),
            subpixel_cache: Mutex::new(LruCache::new(512)),
        }
    }

    fn resolve_font(&self, font: FontRef) -> AureaResult<Arc<Font>> {
        let key = FontKey::from_font(font);

        if let Some(hit) = lock(&self.font_cache).get(&key).cloned() {
            return Ok(hit);
        }

        let loaded = self.load_for_key(font)?;
        lock(&self.font_cache).insert(key, loaded.clone());
        Ok(loaded)
    }

    fn files(&self) -> &[FontFile] {
        self.files.get_or_init(|| scan_font_files(&self.dirs))
    }

    fn fallback(&self, slot: usize) -> Option<Arc<Font>> {
        let stem = fallback_stems().get(slot)?;
        self.fallbacks
            .get(slot)?
            .get_or_init(|| {
                let (_, path) = self.files().iter().find(|(s, _)| s == stem)?;
                load_font_file(path).map(Arc::new)
            })
            .clone()
    }

    /// The font that draws `ch`: the requested one when it has the glyph,
    /// otherwise the first fallback that does.
    fn font_for_char(&self, primary: &Arc<Font>, ch: char) -> Arc<Font> {
        if ch.is_whitespace() || ch.is_control() || primary.has_glyph(ch) {
            return primary.clone();
        }
        (0..self.fallbacks.len())
            .filter_map(|slot| self.fallback(slot))
            .find(|font| font.has_glyph(ch))
            .unwrap_or_else(|| primary.clone())
    }

    fn load_for_key(&self, font: FontRef) -> AureaResult<Arc<Font>> {
        // 1. The requested family, in the requested face when installed.
        if let Some(path) = find_font_file(font.family, font.weight, font.style, self.files())
            && let Some(f) = load_font_file(path)
        {
            return Ok(Arc::new(f));
        }

        // 2. The first platform fallback that is installed.
        if let Some(f) = (0..self.fallbacks.len()).find_map(|slot| self.fallback(slot)) {
            return Ok(f);
        }

        // 3. Embedded Tuffy (public domain) — guaranteed last resort.
        static EMBEDDED: &[u8] = include_bytes!("../../fonts/Tuffy.ttf");
        if let Ok(f) = Font::from_bytes(
            EMBEDDED,
            FontSettings {
                collection_index: 0,
                ..FontSettings::default()
            },
        ) {
            return Ok(Arc::new(f));
        }

        Err(AureaError::RenderingFailed)
    }
}

impl Default for FontDbTextRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformTextRasterizer for FontDbTextRasterizer {
    fn rasterize_glyph(&self, font: FontRef, char_code: u32) -> AureaResult<GlyphBitmap> {
        let ch = char::from_u32(char_code).unwrap_or('\u{FFFD}');
        let fnt = self.font_for_char(&self.resolve_font(font)?, ch);
        let (m, bmp) = fnt.rasterize(ch, font.size);

        let width = u32::try_from(m.width).expect("glyph width fits in u32");
        let height = u32::try_from(m.height).expect("glyph height fits in u32");
        let mut data = vec![0u8; (width * height * 4) as usize];
        for (i, alpha) in bmp.iter().copied().enumerate() {
            let base = i * 4;
            if base + 3 < data.len() {
                data[base] = 255;
                data[base + 1] = 255;
                data[base + 2] = 255;
                data[base + 3] = alpha;
            }
        }

        Ok(GlyphBitmap {
            width,
            height,
            data,
            bearing_x: m.xmin as f32,
            bearing_y: m.height as f32 + m.ymin as f32,
            advance: m.advance_width,
        })
    }

    fn rasterize_subpixel(&self, font: FontRef, char_code: u32) -> AureaResult<Arc<SubpixelGlyph>> {
        let key = GlyphKey::new(font, char_code);
        // LruCache::get takes &mut self to update the recency timestamp.
        if let Some(hit) = lock(&self.subpixel_cache).get(&key).cloned() {
            return Ok(hit);
        }

        let ch = char::from_u32(char_code).unwrap_or('\u{FFFD}');
        let fnt = self.font_for_char(&self.resolve_font(font)?, ch);

        // 3× supersample → RGB subpixel coverage.
        let (m, bmp) = fnt.rasterize(ch, font.size * 3.0);
        let w3 = i32::try_from(m.width).expect("glyph width fits in i32");
        let h3 = i32::try_from(m.height).expect("glyph height fits in i32");

        let glyph = if w3 <= 0 || h3 <= 0 {
            SubpixelGlyph {
                width: 0,
                height: 0,
                left: 0,
                top: 0,
                advance: m.advance_width / 3.0,
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
            // 5-tap FreeType-default LCD filter.
            const FILT: [f32; 5] = [
                8.0 / 256.0,
                77.0 / 256.0,
                86.0 / 256.0,
                77.0 / 256.0,
                8.0 / 256.0,
            ];
            let mut coverage = vec![0u8; dev_w * dev_h * 3];
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

            SubpixelGlyph {
                width: u32::try_from(dev_w).expect("glyph width fits in u32"),
                height: u32::try_from(dev_h).expect("glyph height fits in u32"),
                left: f32_to_i32_clamped((m.xmin as f32 / 3.0).round()),
                top: -f32_to_i32_clamped(((h3 + m.ymin) as f32 / 3.0).round()),
                advance: m.advance_width / 3.0,
                coverage,
            }
        };

        let g = Arc::new(glyph);
        lock(&self.subpixel_cache).insert(key, g.clone());
        Ok(g)
    }

    fn measure_text(&self, text: &str, font: FontRef) -> AureaResult<TextMetrics> {
        let fnt = self.resolve_font(font)?;
        let advance: f32 = text
            .chars()
            .map(|c| {
                self.font_for_char(&fnt, c)
                    .metrics(c, font.size)
                    .advance_width
            })
            .sum();

        let (ascent, descent) = fnt
            .horizontal_line_metrics(font.size)
            .map(|lm| (lm.ascent, lm.descent.abs()))
            .unwrap_or((font.size * 0.8, font.size * 0.2));

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
    use std::env::temp_dir;
    use std::fs::{create_dir_all, remove_dir_all, write};
    use std::process::id;
    use std::slice::from_ref;

    #[test]
    fn loads_a_font_or_fallback() {
        let r = FontDbTextRasterizer::new();
        let font = Font::new("__no_such_font__", 14.0);
        let m = r
            .measure_text("A", (&font).into())
            .expect("should fall back to a system font");
        assert!(m.ascent > 0.0);
    }

    /// Hangul is missing from the usual Latin UI fonts. When any installed
    /// fallback has it, that is the font that draws it.
    #[test]
    fn a_missing_glyph_comes_from_a_fallback() {
        let r = FontDbTextRasterizer::new();
        let font = Font::new("__no_such_font__", 14.0);
        let primary = r.resolve_font((&font).into()).expect("some font");
        let covered = (0..r.fallbacks.len())
            .filter_map(|slot| r.fallback(slot))
            .any(|f| f.has_glyph('가'));
        assert_eq!(r.font_for_char(&primary, '가').has_glyph('가'), covered);
        assert!(Arc::ptr_eq(&r.font_for_char(&primary, ' '), &primary));
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
    fn picks_the_family_not_a_longer_one() {
        let list = files(&["dejavusansmono", "dejavusansbold", "dejavusans"]);
        let normal = (FontWeight::Normal, FontStyle::Normal);
        assert_eq!(pick("DejaVu Sans", normal.0, normal.1, &list), "dejavusans.ttf");
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
