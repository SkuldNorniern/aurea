//! fontdue/fontdb glyph rasterizer — the cross-platform fallback backend.
//!
//! Produces grayscale glyph bitmaps and, for subpixel AA, supersamples each
//! glyph at 3x and collapses it back to RGB subpixel coverage. fontdue does no
//! hinting, so small text is softer than a platform-native (e.g. DirectWrite)
//! backend; this exists so every platform renders *something* reasonable.

use super::super::types::{Font, FontStyle, FontWeight, TextMetrics};
use super::atlas::{GlyphBitmap, GlyphKey};
use super::platform::{PlatformTextRasterizer, SubpixelGlyph};
use aurea_core::{AureaError, AureaResult};
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    family: String,
    weight: FontWeight,
    style: FontStyle,
}

impl FontKey {
    fn from_font(font: &Font) -> Self {
        Self {
            family: font.family.clone(),
            weight: font.weight,
            style: font.style,
        }
    }
}

pub struct FontDbTextRasterizer {
    db: fontdb::Database,
    cache: Mutex<HashMap<FontKey, Arc<fontdue::Font>>>,
    subpixel_cache: Mutex<HashMap<GlyphKey, Arc<SubpixelGlyph>>>,
}

impl FontDbTextRasterizer {
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        set_platform_generic_families(&mut db);
        Self {
            db,
            cache: Mutex::new(HashMap::new()),
            subpixel_cache: Mutex::new(HashMap::new()),
        }
    }

    fn resolve_font(&self, font: &Font) -> AureaResult<Arc<fontdue::Font>> {
        let key = FontKey::from_font(font);

        if let Some(cached) = aurea_core::lock(&self.cache).get(&key).cloned() {
            return Ok(cached);
        }

        let family_name = font.family.trim();
        let families = if family_name.is_empty() {
            [fontdb::Family::SansSerif]
        } else {
            [fontdb::Family::Name(family_name)]
        };

        let weight = match font.weight {
            FontWeight::Bold => fontdb::Weight(700),
            FontWeight::Normal => fontdb::Weight(400),
        };

        let style = match font.style {
            FontStyle::Italic => fontdb::Style::Italic,
            FontStyle::Normal => fontdb::Style::Normal,
        };

        let query = fontdb::Query {
            families: &families,
            weight,
            style,
            stretch: fontdb::Stretch::Normal,
        };

        let fallback_families = [fontdb::Family::Monospace, fontdb::Family::SansSerif];
        let fallback_query = fontdb::Query {
            families: &fallback_families,
            weight,
            style,
            stretch: fontdb::Stretch::Normal,
        };

        let font_arc = self
            .db
            .query(&query)
            .and_then(|face_id| self.load_face(face_id).ok().map(Arc::new))
            .or_else(|| {
                self.db
                    .query(&fallback_query)
                    .and_then(|face_id| self.load_face(face_id).ok().map(Arc::new))
            })
            .or_else(|| self.first_loadable_face().map(Arc::new))
            .ok_or(AureaError::RenderingFailed)?;

        aurea_core::lock(&self.cache).insert(key, font_arc.clone());
        Ok(font_arc)
    }

    fn load_face(&self, face_id: fontdb::ID) -> AureaResult<fontdue::Font> {
        let face = self.db.face(face_id).ok_or(AureaError::RenderingFailed)?;
        let collection_index = face.index;
        let data = match &face.source {
            fontdb::Source::Binary(data) => data.as_ref().as_ref().to_vec(),
            fontdb::Source::File(path) => fs::read(path).map_err(|_| AureaError::RenderingFailed)?,
            fontdb::Source::SharedFile(_, data) => data.as_ref().as_ref().to_vec(),
        };

        fontdue::Font::from_bytes(
            data,
            fontdue::FontSettings {
                collection_index,
                ..fontdue::FontSettings::default()
            },
        )
        .map_err(|_| AureaError::RenderingFailed)
    }

    fn first_loadable_face(&self) -> Option<fontdue::Font> {
        self.db
            .faces()
            .find_map(|face| self.load_face(face.id).ok())
    }
}

#[cfg(target_os = "linux")]
fn set_platform_generic_families(db: &mut fontdb::Database) {
    if let Some(name) = first_family_matching(db, |face| face.monospaced) {
        db.set_monospace_family(name);
    }
    if let Some(name) = preferred_sans_family(db) {
        db.set_sans_serif_family(name);
    }
}

#[cfg(not(target_os = "linux"))]
fn set_platform_generic_families(_db: &mut fontdb::Database) {}

#[cfg(target_os = "linux")]
fn first_family_matching(
    db: &fontdb::Database,
    predicate: impl Fn(&fontdb::FaceInfo) -> bool,
) -> Option<String> {
    db.faces()
        .find(|face| predicate(face))
        .and_then(|face| face.families.first().map(|(name, _)| name.clone()))
}

#[cfg(target_os = "linux")]
fn preferred_sans_family(db: &fontdb::Database) -> Option<String> {
    first_family_matching(db, |face| !face.monospaced)
        .or_else(|| first_family_matching(db, |_| true))
}

impl Default for FontDbTextRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_requested_family_falls_back_to_system_font() {
        let rasterizer = FontDbTextRasterizer::new();
        let font = Font::new("__aurea_missing_font_family__", 24.0);
        let metrics = rasterizer
            .measure_text("Hello", &font)
            .expect("missing requested font should fall back to a system font");

        assert!(metrics.width > 0.0);
        assert!(metrics.ascent > 0.0);
    }
}

impl PlatformTextRasterizer for FontDbTextRasterizer {
    fn rasterize_glyph(&self, font: &Font, char_code: u32) -> AureaResult<GlyphBitmap> {
        let fontdue = self.resolve_font(font)?;
        let ch = char::from_u32(char_code).unwrap_or('\u{FFFD}');
        let (metrics, bitmap) = fontdue.rasterize(ch, font.size);

        let width = metrics.width as u32;
        let height = metrics.height as u32;
        let mut data = vec![0u8; (width * height * 4) as usize];
        for (i, alpha) in bitmap.iter().copied().enumerate() {
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
            bearing_x: metrics.xmin as f32,
            bearing_y: metrics.height as f32 + metrics.ymin as f32,
            advance: metrics.advance_width,
        })
    }

    fn rasterize_subpixel(&self, font: &Font, char_code: u32) -> AureaResult<Arc<SubpixelGlyph>> {
        let key = GlyphKey::new(font, char_code);
        if let Some(cached) = aurea_core::lock(&self.subpixel_cache).get(&key).cloned() {
            return Ok(cached);
        }

        let fontdue = self.resolve_font(font)?;
        let ch = char::from_u32(char_code).unwrap_or('\u{FFFD}');
        // 3x supersample so each source column becomes one RGB subpixel stripe.
        let (m, bmp) = fontdue.rasterize(ch, font.size * 3.0);
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
            // Accumulate linear coverage per subpixel column (dev_w*3) x dev_h.
            let sub_w = dev_w * 3;
            let mut acc = vec![0f32; sub_w * dev_h];
            for sy in 0..h3 {
                let dev_row = (sy / 3) as usize;
                let g_row = (sy * w3) as usize;
                for sx in 0..w3 {
                    let a = bmp[g_row + sx as usize] as f32 / 255.0;
                    acc[dev_row * sub_w + sx as usize] += a / 3.0; // vertical box average
                }
            }
            for v in acc.iter_mut() {
                if *v > 1.0 {
                    *v = 1.0;
                }
            }
            // Light 5-tap LCD filter (FreeType default) to curb colour fringing.
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
                    let mut s = 0.0f32;
                    for (k, weight) in FILT.iter().enumerate() {
                        let xi = x as isize + k as isize - 2;
                        if xi >= 0 && (xi as usize) < sub_w {
                            s += acc[row + xi as usize] * weight;
                        }
                    }
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

        let glyph = Arc::new(glyph);
        aurea_core::lock(&self.subpixel_cache).insert(key, glyph.clone());
        Ok(glyph)
    }

    fn measure_text(&self, text: &str, font: &Font) -> AureaResult<TextMetrics> {
        let fontdue = self.resolve_font(font)?;
        let mut advance = 0.0f32;

        for ch in text.chars() {
            let metrics = fontdue.metrics(ch, font.size);
            advance += metrics.advance_width;
        }

        let (ascent, descent) = match fontdue.horizontal_line_metrics(font.size) {
            Some(line) => (line.ascent, line.descent.abs()),
            None => (font.size * 0.8, font.size * 0.2),
        };

        let height = (ascent + descent).max(0.0);

        Ok(TextMetrics {
            width: advance,
            height,
            ascent,
            descent,
            advance,
        })
    }
}
