//! fontdue glyph rasterizer — cross-platform fallback backend.
//!
//! Memory design: we never scan all system fonts.  fontdb's
//! `load_system_fonts` reads every font file on disk to extract metadata —
//! on macOS that is 500+ files, easily 200-300 MB of page-cache pressure.
//! Instead we do a cheap filename-based search in the standard font directories
//! and load only the single file we actually need.

use super::super::types::{FontStyle, FontWeight, TextMetrics};
use super::LruCache;
use super::atlas::{GlyphBitmap, GlyphKey};
use super::platform::{FontRef, PlatformTextRasterizer, SubpixelGlyph};
use aurea_foundation::{AureaError, AureaResult};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(root) = std::env::var_os("SYSTEMROOT") {
            dirs.push(PathBuf::from(root).join("Fonts"));
        } else {
            dirs.push(PathBuf::from("C:\\Windows\\Fonts"));
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let home = PathBuf::from(profile);
            dirs.push(home.join("AppData\\Local\\Microsoft\\Windows\\Fonts"));
            dirs.push(home.join("AppData\\Roaming\\Microsoft\\Windows\\Fonts"));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            let h = PathBuf::from(home);
            dirs.push(h.join(".fonts"));
            dirs.push(h.join(".local/share/fonts"));
        }
    }

    dirs
}

/// Platform fallback font paths tried in order when the requested family is
/// not found by filename search.
fn fallback_paths() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        &[
            "/System/Library/Fonts/SFNSMono.ttf",
            "/System/Library/Fonts/Menlo.ttc",
            "/System/Library/Fonts/Monaco.ttf",
            "/System/Library/Fonts/Courier.ttc",
            "/System/Library/Fonts/Supplemental/Courier New.ttf",
            "/System/Library/Fonts/Supplemental/Andale Mono.ttf",
        ]
    }
    #[cfg(target_os = "windows")]
    {
        &[
            "C:\\Windows\\Fonts\\consola.ttf",
            "C:\\Windows\\Fonts\\cour.ttf",
            "C:\\Windows\\Fonts\\lucon.ttf",
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
            "/usr/share/fonts/truetype/freefont/FreeMono.ttf",
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
    let name = name
        .trim_end_matches(".ttf")
        .trim_end_matches(".TTF")
        .trim_end_matches(".otf")
        .trim_end_matches(".OTF")
        .trim_end_matches(".ttc")
        .trim_end_matches(".TTC");
    name
}

/// Walk `dirs` (non-recursively) and return the path of the file whose stem
/// best matches `family`.  Returns `None` if nothing matches.
fn find_by_filename(family: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let want = normalise(family);
    if want.is_empty() {
        return None;
    }

    let mut best: Option<(usize, PathBuf)> = None; // (match_score, path)

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let fname_str = fname.to_string_lossy();
            // Only consider font files.
            let lower = fname_str.to_lowercase();
            if !lower.ends_with(".ttf") && !lower.ends_with(".otf") && !lower.ends_with(".ttc") {
                continue;
            }
            let stem_norm = normalise(file_stem(&fname_str));

            // Score: exact > starts-with > contains.
            let score = if stem_norm == want {
                3
            } else if stem_norm.starts_with(&want) || want.starts_with(&stem_norm) {
                2
            } else if stem_norm.contains(&want) {
                1
            } else {
                continue;
            };

            if best.as_ref().map_or(true, |(s, _)| score > *s) {
                best = Some((score, entry.path()));
            }
        }
    }

    best.map(|(_, p)| p)
}

/// Load a fontdue Font from a file path (collection index 0).
fn load_font_file(path: &Path) -> Option<fontdue::Font> {
    let data = std::fs::read(path).ok()?;
    fontdue::Font::from_bytes(
        data,
        fontdue::FontSettings {
            collection_index: 0,
            ..fontdue::FontSettings::default()
        },
    )
    .ok()
}

// ── Rasterizer ────────────────────────────────────────────────────────────────

pub struct FontDbTextRasterizer {
    dirs: Vec<PathBuf>,
    /// LRU cap: 32 entries — typical UIs use fewer than 10 font variants.
    font_cache: Mutex<LruCache<FontKey, Arc<fontdue::Font>>>,
    /// LRU cap: 512 entries — one per (font, char) pair; covers full ASCII +
    /// common Unicode ranges without unbounded growth on text-heavy views.
    subpixel_cache: Mutex<LruCache<GlyphKey, Arc<SubpixelGlyph>>>,
}

impl FontDbTextRasterizer {
    pub fn new() -> Self {
        Self {
            dirs: font_search_dirs(),
            font_cache: Mutex::new(LruCache::new(32)),
            subpixel_cache: Mutex::new(LruCache::new(512)),
        }
    }

    fn resolve_font(&self, font: FontRef) -> AureaResult<Arc<fontdue::Font>> {
        let key = FontKey::from_font(font);

        if let Some(hit) = aurea_foundation::lock(&self.font_cache).get(&key).cloned() {
            return Ok(hit);
        }

        let loaded = self.load_for_key(font)?;
        aurea_foundation::lock(&self.font_cache).insert(key, loaded.clone());
        Ok(loaded)
    }

    fn load_for_key(&self, font: FontRef) -> AureaResult<Arc<fontdue::Font>> {
        // 1. Filename search for the requested family. `find_by_filename`
        // normalizes the family internally, so no allocation is needed here.
        if !font.family.is_empty() {
            if let Some(path) = find_by_filename(font.family, &self.dirs) {
                if let Some(f) = load_font_file(&path) {
                    return Ok(Arc::new(f));
                }
            }
        }

        // 2. Platform fallbacks in order.
        for &path in fallback_paths() {
            if let Some(f) = load_font_file(Path::new(path)) {
                return Ok(Arc::new(f));
            }
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
        let fnt = self.resolve_font(font)?;
        let ch = char::from_u32(char_code).unwrap_or('\u{FFFD}');
        let (m, bmp) = fnt.rasterize(ch, font.size);

        let width = m.width as u32;
        let height = m.height as u32;
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
        if let Some(hit) = aurea_foundation::lock(&self.subpixel_cache)
            .get(&key)
            .cloned()
        {
            return Ok(hit);
        }

        let fnt = self.resolve_font(font)?;
        let ch = char::from_u32(char_code).unwrap_or('\u{FFFD}');

        // 3× supersample → RGB subpixel coverage.
        let (m, bmp) = fnt.rasterize(ch, font.size * 3.0);
        let w3 = m.width as i32;
        let h3 = m.height as i32;

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
            let dev_w = ((w3 + 2) / 3).max(1) as usize;
            let dev_h = ((h3 + 2) / 3).max(1) as usize;
            let sub_w = dev_w * 3;
            let mut acc = vec![0f32; sub_w * dev_h];
            for sy in 0..h3 {
                let g_row = (sy * w3) as usize;
                let dev_row = (sy / 3) as usize;
                for sx in 0..w3 {
                    acc[dev_row * sub_w + sx as usize] +=
                        bmp[g_row + sx as usize] as f32 / (255.0 * 3.0);
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
                            if xi >= 0 && (xi as usize) < sub_w {
                                acc[row + xi as usize] * w
                            } else {
                                0.0
                            }
                        })
                        .sum();
                    coverage[y * sub_w + x] = (s * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }

            SubpixelGlyph {
                width: dev_w as u32,
                height: dev_h as u32,
                left: (m.xmin as f32 / 3.0).round() as i32,
                top: -(((h3 + m.ymin) as f32) / 3.0).round() as i32,
                advance: m.advance_width / 3.0,
                coverage,
            }
        };

        let g = Arc::new(glyph);
        aurea_foundation::lock(&self.subpixel_cache).insert(key, g.clone());
        Ok(g)
    }

    fn measure_text(&self, text: &str, font: FontRef) -> AureaResult<TextMetrics> {
        let fnt = self.resolve_font(font)?;
        let advance: f32 = text
            .chars()
            .map(|c| fnt.metrics(c, font.size).advance_width)
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

    #[test]
    fn loads_a_font_or_fallback() {
        let r = FontDbTextRasterizer::new();
        let font = Font::new("__no_such_font__", 14.0);
        let m = r
            .measure_text("A", (&font).into())
            .expect("should fall back to a system font");
        assert!(m.ascent > 0.0);
    }
}
