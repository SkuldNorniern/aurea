//! Text rasterization using system fonts
//!
//! Uses fontdb/fontdue to rasterize glyphs into bitmaps from installed fonts.
//! Platform-native rasterizers can be added later if needed.

use super::super::types::{Color, Font, FontStyle, FontWeight, Point, TextMetrics};
use super::atlas::{GlyphAtlas, GlyphBitmap, GlyphKey};
use aurea_core::{AureaError, AureaResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Platform text rasterizer trait
pub trait PlatformTextRasterizer: Send + Sync {
    /// Rasterize a single glyph
    fn rasterize_glyph(&self, font: &Font, char_code: u32) -> AureaResult<GlyphBitmap>;

    /// Measure text dimensions
    fn measure_text(
        &self,
        text: &str,
        font: &Font,
    ) -> AureaResult<super::super::types::TextMetrics>;
}

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

struct FontDbTextRasterizer {
    db: fontdb::Database,
    cache: Mutex<HashMap<FontKey, Arc<fontdue::Font>>>,
}

impl FontDbTextRasterizer {
    fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Self {
            db,
            cache: Mutex::new(HashMap::new()),
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

        let fallback_families = [fontdb::Family::SansSerif];
        let fallback_query = fontdb::Query {
            families: &fallback_families,
            weight,
            style,
            stretch: fontdb::Stretch::Normal,
        };

        let face_id = self
            .db
            .query(&query)
            .or_else(|| self.db.query(&fallback_query));
        let face_id = face_id.ok_or(AureaError::RenderingFailed)?;

        let data = self
            .db
            .with_face_data(face_id, |bytes, _| bytes.to_vec())
            .ok_or(AureaError::RenderingFailed)?;

        let fontdue = fontdue::Font::from_bytes(data, fontdue::FontSettings::default())
            .map_err(|_| AureaError::RenderingFailed)?;

        let font_arc = Arc::new(fontdue);
        aurea_core::lock(&self.cache).insert(key, font_arc.clone());
        Ok(font_arc)
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
            bearing_y: -metrics.ymin as f32,
            advance: metrics.advance_width,
        })
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

/// Get the platform text rasterizer (system fonts via fontdb/fontdue).
pub fn get_platform_rasterizer() -> Box<dyn PlatformTextRasterizer> {
    Box::new(FontDbTextRasterizer::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Font;

    #[test]
    fn measure_text_returns_positive_for_non_empty() {
        let rasterizer = get_platform_rasterizer();
        let font = Font::new("", 24.0);
        // SAFETY: System fonts always load; fontdb/fontdue never fail for basic ASCII.
        let metrics = rasterizer
            .measure_text("Hello", &font)
            .expect("measure_text should succeed with system fonts");
        assert!(metrics.width > 0.0, "width should be positive");
        assert!(metrics.ascent > 0.0, "ascent should be positive");
        assert!(metrics.advance > 0.0, "advance should be positive");
    }

    #[test]
    fn measure_text_empty_string_zero_width() {
        let rasterizer = get_platform_rasterizer();
        let font = Font::new("", 24.0);
        // SAFETY: Empty string always yields zero-width metrics.
        let metrics = rasterizer
            .measure_text("", &font)
            .expect("measure_text empty should succeed");
        assert!(metrics.width == 0.0 && metrics.advance == 0.0);
    }

    #[test]
    fn rasterize_glyph_returns_bitmap() {
        let rasterizer = get_platform_rasterizer();
        let font = Font::new("", 24.0);
        // SAFETY: fontdue rasterizes ASCII 'A' from system fonts.
        let bitmap = rasterizer
            .rasterize_glyph(&font, 'A' as u32)
            .expect("rasterize_glyph A should succeed");
        assert!(bitmap.width > 0 && bitmap.height > 0);
        assert_eq!(
            bitmap.data.len(),
            (bitmap.width * bitmap.height) as usize * 4
        );
    }
}

/// Text renderer that uses platform APIs and glyph atlas
pub struct TextRenderer {
    rasterizer: Box<dyn PlatformTextRasterizer>,
    atlas: GlyphAtlas,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            rasterizer: get_platform_rasterizer(),
            atlas: GlyphAtlas::new(4), // 4MB cache budget
        }
    }

    /// Render text to a buffer at a position
    pub fn render_text(
        &self,
        text: &str,
        position: Point,
        font: &Font,
        color: Color,
        buffer: &mut [u32],
        buffer_width: u32,
        buffer_height: u32,
    ) -> AureaResult<()> {
        let mut x = position.x;
        let y = position.y;

        for ch in text.chars() {
            let char_code = ch as u32;
            let key = GlyphKey::new(font, char_code);

            // Get glyph from cache or rasterize
            let glyph = match self.atlas.get(&key) {
                Some(bitmap) => bitmap,
                None => {
                    let bitmap = self.rasterizer.rasterize_glyph(font, char_code)?;
                    let bitmap_clone = bitmap.clone();
                    self.atlas.put(key, bitmap_clone)?;
                    // SAFETY: We just inserted key via put(); get() must return Some.
                    self.atlas
                        .get(&key)
                        .expect("glyph present after atlas put; we just inserted this key")
                }
            };

            // Blit glyph to buffer
            self.blit_glyph(&glyph, x, y, color, buffer, buffer_width, buffer_height)?;

            x += glyph.advance;
        }

        Ok(())
    }

    /// Blit a glyph bitmap to the buffer
    fn blit_glyph(
        &self,
        glyph: &GlyphBitmap,
        x: f32,
        y: f32,
        color: Color,
        buffer: &mut [u32],
        buffer_width: u32,
        buffer_height: u32,
    ) -> AureaResult<()> {
        let start_x = (x + glyph.bearing_x) as i32;
        let start_y = (y - glyph.bearing_y) as i32;

        for gy in 0..glyph.height {
            for gx in 0..glyph.width {
                let buffer_x = start_x + gx as i32;
                let buffer_y = start_y + gy as i32;

                if buffer_x >= 0
                    && buffer_x < buffer_width as i32
                    && buffer_y >= 0
                    && buffer_y < buffer_height as i32
                {
                    let glyph_idx = (gy * glyph.width + gx) as usize;
                    if glyph_idx * 4 + 3 < glyph.data.len() {
                        let alpha = glyph.data[glyph_idx * 4 + 3] as f32 / 255.0;

                        // Blend glyph alpha with text color
                        let blended = Color::rgba(
                            ((color.r as f32 * alpha).min(255.0)) as u8,
                            ((color.g as f32 * alpha).min(255.0)) as u8,
                            ((color.b as f32 * alpha).min(255.0)) as u8,
                            ((color.a as f32 * alpha).min(255.0)) as u8,
                        );

                        let buffer_idx =
                            (buffer_y as usize) * (buffer_width as usize) + (buffer_x as usize);
                        if buffer_idx < buffer.len() {
                            buffer[buffer_idx] = ((blended.a as u32) << 24)
                                | ((blended.r as u32) << 16)
                                | ((blended.g as u32) << 8)
                                | (blended.b as u32);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Measure text dimensions
    pub fn measure_text(
        &self,
        text: &str,
        font: &Font,
    ) -> AureaResult<super::super::types::TextMetrics> {
        self.rasterizer.measure_text(text, font)
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
