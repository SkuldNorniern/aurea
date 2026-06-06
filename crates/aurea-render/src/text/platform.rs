//! Text rasterization orchestration.
//!
//! Defines the [`PlatformTextRasterizer`] backend seam, the [`SubpixelGlyph`]
//! exchange type, and [`TextRenderer`], which shapes a run of per-glyph subpixel
//! coverage into a single [`GlyphMask`]. Concrete backends live in sibling
//! modules:
//! - `directwrite_backend` — hinted ClearType via DirectWrite (Windows only).
//! - `fontdue_backend` — cross-platform fallback (no hinting).
//!
//! [`get_platform_rasterizer`] picks the best available backend per platform.

use super::super::types::{Color, Font, GlyphMask, Point};
use super::atlas::{GlyphAtlas, GlyphBitmap, GlyphKey};
use aurea_core::AureaResult;
use std::sync::Arc;

/// A single glyph rasterized to device-resolution RGB subpixel coverage.
///
/// Coordinates are in device pixels. The mask is colourless: `coverage` holds
/// three bytes per pixel (R, G, B subpixel stripes); the text colour is applied
/// at composite time. Backends are expected to return *hinted* coverage where
/// the platform supports it.
#[derive(Clone)]
pub struct SubpixelGlyph {
    /// Bitmap width in device pixels.
    pub width: u32,
    /// Bitmap height in device pixels.
    pub height: u32,
    /// X offset from the pen origin to the bitmap's left edge.
    pub left: i32,
    /// Y offset from the baseline to the bitmap's top edge (down = positive).
    pub top: i32,
    /// Horizontal advance in device pixels.
    pub advance: f32,
    /// Subpixel coverage, `width * height * 3` bytes in R, G, B order.
    pub coverage: Vec<u8>,
}

/// Platform text rasterizer trait — the backend seam.
pub trait PlatformTextRasterizer: Send + Sync {
    /// Rasterize a single grayscale glyph (legacy / generic path).
    fn rasterize_glyph(&self, font: &Font, char_code: u32) -> AureaResult<GlyphBitmap>;

    /// Rasterize a single glyph to hinted RGB subpixel coverage (cached).
    fn rasterize_subpixel(&self, font: &Font, char_code: u32) -> AureaResult<Arc<SubpixelGlyph>>;

    /// Measure text dimensions.
    fn measure_text(
        &self,
        text: &str,
        font: &Font,
    ) -> AureaResult<super::super::types::TextMetrics>;
}

/// Get the best available platform text rasterizer.
///
/// Windows uses DirectWrite (hinted ClearType); everything else uses fontdue.
pub fn get_platform_rasterizer() -> Box<dyn PlatformTextRasterizer> {
    #[cfg(windows)]
    {
        match super::directwrite_backend::DirectWriteRasterizer::new() {
            Ok(dw) => return Box::new(dw),
            Err(_) => {
                // Fall through to the fontdue backend if DirectWrite init fails.
            }
        }
    }
    Box::new(super::fontdue_backend::FontDbTextRasterizer::new())
}

/// Text renderer: owns a backend + a glyph atlas, and shapes runs.
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

    /// Shape a text run into a subpixel (LCD) coverage mask.
    ///
    /// Returns the mask plus the run's ascent and padding so the caller can
    /// position it: the mask's top-left sits at `(point.x - pad, point.y -
    /// ascent - pad)`. Per-glyph coverage comes pre-hinted from the backend;
    /// this just lays glyphs out along the pen and max-combines overlaps.
    pub fn render_text_subpixel(
        &self,
        text: &str,
        font: &Font,
    ) -> AureaResult<(GlyphMask, f32, f32)> {
        let tm = self.rasterizer.measure_text(text, font)?;
        let pad = 3.0f32;
        let ascent = tm.ascent.max(0.0);
        let dev_w = (tm.width + pad * 2.0).ceil().max(1.0) as u32;
        let dev_h = (tm.height + pad * 2.0).ceil().max(1.0) as u32;
        let mut coverage = vec![0u8; (dev_w * dev_h * 3) as usize];

        let baseline = (ascent + pad).round() as i32;
        let mut pen = pad;

        for ch in text.chars() {
            let g = self.rasterizer.rasterize_subpixel(font, ch as u32)?;
            if g.width > 0 && g.height > 0 {
                let gx = pen.round() as i32 + g.left;
                let gy = baseline + g.top;
                let gw = g.width as i32;
                for row in 0..g.height as i32 {
                    let dy = gy + row;
                    if dy < 0 || dy >= dev_h as i32 {
                        continue;
                    }
                    for col in 0..gw {
                        let dx = gx + col;
                        if dx < 0 || dx >= dev_w as i32 {
                            continue;
                        }
                        let si = ((row * gw + col) * 3) as usize;
                        let di = ((dy * dev_w as i32 + dx) * 3) as usize;
                        // Max-combine so slight glyph overlaps don't double-darken.
                        for c in 0..3 {
                            if g.coverage[si + c] > coverage[di + c] {
                                coverage[di + c] = g.coverage[si + c];
                            }
                        }
                    }
                }
            }
            pen += g.advance;
        }

        Ok((
            GlyphMask {
                width: dev_w,
                height: dev_h,
                coverage,
            },
            ascent,
            pad,
        ))
    }

    /// Render grayscale text into an RGBA buffer (legacy generic path).
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

            let glyph = match self.atlas.get(&key) {
                Some(bitmap) => bitmap,
                None => {
                    let bitmap = self.rasterizer.rasterize_glyph(font, char_code)?;
                    self.atlas.put(key, bitmap.clone())?;
                    self.atlas
                        .get(&key)
                        .expect("glyph present after atlas put; we just inserted this key")
                }
            };

            self.blit_glyph(&glyph, x, y, color, buffer, buffer_width, buffer_height)?;
            x += glyph.advance;
        }

        Ok(())
    }

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
        let start_x = (x + glyph.bearing_x).round() as i32;
        let start_y = (y - glyph.bearing_y).round() as i32;

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
                        let out_a = glyph.data[glyph_idx * 4 + 3];
                        if out_a == 0 {
                            continue;
                        }
                        let buffer_idx =
                            (buffer_y as usize) * (buffer_width as usize) + (buffer_x as usize);
                        if buffer_idx < buffer.len() {
                            buffer[buffer_idx] = ((out_a as u32) << 24)
                                | ((color.r as u32) << 16)
                                | ((color.g as u32) << 8)
                                | (color.b as u32);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Measure text dimensions.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Font;

    #[test]
    fn measure_text_returns_positive_for_non_empty() {
        let rasterizer = get_platform_rasterizer();
        let font = Font::new("", 24.0);
        let metrics = rasterizer
            .measure_text("Hello", &font)
            .expect("measure_text should succeed with system fonts");
        assert!(metrics.width > 0.0, "width should be positive");
        assert!(metrics.ascent > 0.0, "ascent should be positive");
        assert!(metrics.advance > 0.0, "advance should be positive");
    }

    #[test]
    fn subpixel_mask_has_rgb_stride() {
        let rasterizer = get_platform_rasterizer();
        let font = Font::new("", 24.0);
        let g = rasterizer
            .rasterize_subpixel(&font, 'A' as u32)
            .expect("rasterize_subpixel A should succeed");
        assert_eq!(g.coverage.len(), (g.width * g.height * 3) as usize);
    }
}
