//! Platform-specific text rasterization
//!
//! Uses native text APIs to rasterize glyphs into bitmaps.
//! Platform-specific implementations for macOS, Windows, Linux, iOS, Android.

use super::atlas::{GlyphAtlas, GlyphBitmap, GlyphKey};
use super::super::types::{Color, Font, Point};
use crate::AureaResult;

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

/// Default platform text rasterizer (placeholder)
pub struct DefaultTextRasterizer;

impl PlatformTextRasterizer for DefaultTextRasterizer {
    fn rasterize_glyph(&self, _font: &Font, _char_code: u32) -> AureaResult<GlyphBitmap> {
        // Placeholder - returns empty bitmap
        Ok(GlyphBitmap {
            width: 0,
            height: 0,
            data: Vec::new(),
            bearing_x: 0.0,
            bearing_y: 0.0,
            advance: 0.0,
        })
    }

    fn measure_text(
        &self,
        _text: &str,
        _font: &Font,
    ) -> AureaResult<super::super::types::TextMetrics> {
        Ok(super::super::types::TextMetrics {
            width: 0.0,
            height: 0.0,
            ascent: 0.0,
            descent: 0.0,
            advance: 0.0,
        })
    }
}

/// Get the platform-specific text rasterizer
pub fn get_platform_rasterizer() -> Box<dyn PlatformTextRasterizer> {
    // TODO: Implement platform-specific rasterizers
    // For now, return default for all platforms
    // Platform-specific implementations will be added later:
    // - macOS/iOS: CoreText
    // - Windows: DirectWrite
    // - Linux: Pango/HarfBuzz
    // - Android: Paint/Typeface
    Box::new(DefaultTextRasterizer)
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
        let mut y = position.y;

        for ch in text.chars() {
            let char_code = ch as u32;
            let key = GlyphKey::new(font, char_code);

            // Get glyph from cache or rasterize
            let glyph = match self.atlas.get(&key) {
                Some(bitmap) => bitmap,
                None => {
                    let bitmap = self.rasterizer.rasterize_glyph(font, char_code)?;
                    // Clone for putting in cache (atlas stores Arc internally)
                    let bitmap_clone = bitmap.clone();
                    self.atlas.put(key, bitmap_clone)?;
                    // Get it back from cache (now it's wrapped in Arc)
                    self.atlas.get(&key).unwrap()
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
