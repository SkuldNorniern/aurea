//! Shaping: text to positioned glyphs.
//!
//! A line is split into bidi runs (in visual order), each run into pieces
//! drawn by one face (the requested one, or a fallback for the characters it
//! lacks), and each piece shaped by harfrust, so ligatures, combining marks,
//! Hangul and complex scripts come out as the font designs them.

use std::ops::Range;

use harfrust::{Buffer, Direction, Feature, ShapeOptions, ShaperFont, Tag, shape};
use unicode_bidi::{BidiClass, BidiInfo, Level, bidi_class};

use crate::text::glyph::FaceId;
use crate::text::platform::{FontRef, PlatformTextRasterizer};
use aurea_foundation::{AureaError, AureaResult};

/// One positioned glyph of [`ShapedText`], in pixels at the text's size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    /// The face that draws it.
    pub face: FaceId,
    /// The glyph id in that face.
    pub id: u16,
    /// Byte offset in the text of the first character the glyph draws.
    /// Several glyphs share one cluster (a base and its marks), and one
    /// glyph can draw several characters (a ligature).
    pub cluster: usize,
    /// How far the pen moves after it: right, or down in vertical text.
    pub advance: f32,
    /// Where it sits from the pen, right and down positive.
    pub offset_x: f32,
    pub offset_y: f32,
    /// Whether it is in a right-to-left run.
    pub rtl: bool,
}

/// A line of text as glyphs in visual order, left to right (top to bottom
/// when vertical), with its metrics. Positions start at the pen's origin:
/// on the baseline, or at the top of the column's centre line.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedText {
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance, along the line.
    pub width: f32,
    /// The requested face's ascent and descent, which set the line height.
    pub ascent: f32,
    pub descent: f32,
    /// The tallest reach of any face drawn, so no glyph is cut off.
    pub ink_ascent: f32,
    pub ink_descent: f32,
    /// Size of the text in pixels.
    pub size: f32,
    /// Length of the text in bytes.
    pub len: usize,
    /// Set top to bottom.
    pub vertical: bool,
}

impl ShapedText {
    /// Each glyph's pen position, in visual order.
    pub fn pen_positions(&self) -> impl Iterator<Item = (f32, &ShapedGlyph)> {
        self.glyphs.iter().scan(0.0, |pen, g| {
            let at = *pen;
            *pen += g.advance;
            Some((at, g))
        })
    }

    /// Where the caret goes before the character at byte `index` (or at the
    /// end for `index == len`), from the line's left edge. Inside a
    /// ligature the glyph's width is split evenly between its characters.
    pub fn caret_x(&self, index: usize) -> f32 {
        // The cluster holding `index`: the glyph run with the greatest
        // cluster not after it.
        let Some(cluster) = self
            .glyphs
            .iter()
            .map(|g| g.cluster)
            .filter(|&c| c <= index)
            .max()
        else {
            return if self.glyphs.first().is_some_and(|g| g.rtl) {
                self.width
            } else {
                0.0
            };
        };
        let next = self.next_cluster(cluster);
        let (left, right, rtl) = self.cluster_span(cluster);
        let fraction = if next > cluster {
            (index.min(next) - cluster) as f32 / (next - cluster) as f32
        } else {
            0.0
        };
        if rtl {
            right - (right - left) * fraction
        } else {
            left + (right - left) * fraction
        }
    }

    /// The byte index whose caret is nearest `x`, from the line's left
    /// edge: the start of the cluster under `x`, or of the one after it
    /// when `x` is past the cluster's middle (in its reading direction).
    pub fn hit(&self, x: f32) -> usize {
        for (at, g) in self.pen_positions() {
            if x < at + g.advance {
                let (left, right, rtl) = self.cluster_span(g.cluster);
                let before = (x < (left + right) / 2.0) != rtl;
                return if before {
                    g.cluster
                } else {
                    self.next_cluster(g.cluster)
                };
            }
        }
        match self.glyphs.last() {
            // Past the right end of a right-to-left line is before its
            // first character, which the rightmost glyph draws.
            Some(g) if g.rtl => g.cluster,
            Some(_) => self.len,
            None => 0,
        }
    }

    /// Where the text in byte `range` shows, as left and right edges from
    /// the line's start: more than one span when right-to-left runs split
    /// it.
    pub fn selection_spans(&self, range: Range<usize>) -> Vec<(f32, f32)> {
        let mut spans: Vec<(f32, f32)> = Vec::new();
        for (pen, g) in self.pen_positions() {
            if !range.contains(&g.cluster) {
                continue;
            }
            let (l, r) = (pen, pen + g.advance);
            match spans.last_mut() {
                Some(last) if (last.1 - l).abs() < 0.01 => last.1 = r,
                _ => spans.push((l, r)),
            }
        }
        spans
    }

    /// The first cluster after `cluster`, or the text's end.
    fn next_cluster(&self, cluster: usize) -> usize {
        self.glyphs
            .iter()
            .map(|g| g.cluster)
            .filter(|&c| c > cluster)
            .min()
            .unwrap_or(self.len)
    }

    /// The left and right edges of the glyphs in `cluster`, and whether it
    /// reads right to left.
    fn cluster_span(&self, cluster: usize) -> (f32, f32, bool) {
        let mut span: Option<(f32, f32, bool)> = None;
        for (at, g) in self.pen_positions().filter(|(_, g)| g.cluster == cluster) {
            let (l, r) = (at, at + g.advance);
            span = Some(match span {
                Some((sl, sr, rtl)) => (sl.min(l), sr.max(r), rtl),
                None => (l, r, g.rtl),
            });
        }
        span.unwrap_or((0.0, 0.0, false))
    }
}

/// Characters that belong with the one before them and so are drawn by its
/// face: marks, joiners and variation selectors. Splitting there would break
/// a cluster across two shaping calls.
fn joins_previous(ch: char) -> bool {
    matches!(ch, '\u{200C}' | '\u{200D}' | '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
        || bidi_class(ch) == BidiClass::NSM
}

/// Pieces of `range` and the face that draws each, in logical order.
fn face_pieces(
    text: &str,
    range: Range<usize>,
    font: FontRef,
    primary: FaceId,
    raster: &dyn PlatformTextRasterizer,
) -> Vec<(Range<usize>, FaceId)> {
    let mut pieces: Vec<(Range<usize>, FaceId)> = Vec::new();
    for (i, ch) in text[range.clone()].char_indices() {
        let at = range.start + i;
        let end = at + ch.len_utf8();
        let face = match pieces.last() {
            Some((_, face)) if joins_previous(ch) || ch.is_whitespace() => *face,
            _ => raster.face_for_char(font, primary, ch),
        };
        match pieces.last_mut() {
            Some((r, f)) if *f == face => r.end = end,
            _ => pieces.push((at..end, face)),
        }
    }
    pieces
}

/// Shape `text` in `font` as one line, in the direction its first strong
/// character reads.
pub fn shape_text(
    text: &str,
    font: FontRef,
    raster: &dyn PlatformTextRasterizer,
) -> AureaResult<ShapedText> {
    shape_line(text, font, None, raster)
}

/// Shape `text` in `font` as one line of a paragraph whose base direction
/// is `rtl`, or found from the text when `None`.
pub(crate) fn shape_line(
    text: &str,
    font: FontRef,
    rtl: Option<bool>,
    raster: &dyn PlatformTextRasterizer,
) -> AureaResult<ShapedText> {
    let primary = raster.face(font)?;
    let (ascent, descent) = raster.face_metrics(primary, font.size);
    let mut shaped = ShapedText {
        glyphs: Vec::new(),
        width: 0.0,
        ascent,
        descent,
        ink_ascent: ascent,
        ink_descent: descent,
        size: font.size,
        len: text.len(),
        vertical: font.vertical,
    };
    let features: Vec<Feature> = font
        .features
        .iter()
        .map(|f| Feature::new(Tag::new(&f.tag), f.value, ..))
        .collect();
    let shaping = Shaping {
        size: font.size,
        features: &features,
        raster,
    };
    if font.vertical {
        // A column reads top to bottom whatever the script.
        for (range, face) in face_pieces(text, 0..text.len(), font, primary, raster) {
            shaping.piece(text, range, face, Direction::TopToBottom, &mut shaped)?;
        }
        shaped.width = shaped.glyphs.iter().map(|g| g.advance).sum();
        return Ok(shaped);
    }
    let level = rtl.map(|rtl| if rtl { Level::rtl() } else { Level::ltr() });
    let bidi = BidiInfo::new(text, level);
    for paragraph in &bidi.paragraphs {
        let (levels, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
        for run in runs {
            let rtl = levels[run.start].is_rtl();
            let mut pieces = face_pieces(text, run, font, primary, raster);
            // A right-to-left run reads its pieces from the right.
            if rtl {
                pieces.reverse();
            }
            let direction = if rtl {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            };
            for (range, face) in pieces {
                shaping.piece(text, range, face, direction, &mut shaped)?;
            }
        }
    }
    shaped.width = shaped.glyphs.iter().map(|g| g.advance).sum();
    Ok(shaped)
}

/// What every piece of one line is shaped with.
struct Shaping<'a> {
    size: f32,
    features: &'a [Feature],
    raster: &'a dyn PlatformTextRasterizer,
}

impl Shaping<'_> {
    fn piece(
        &self,
        text: &str,
        range: Range<usize>,
        face: FaceId,
        direction: Direction,
        shaped: &mut ShapedText,
    ) -> AureaResult<()> {
        let data = self.raster.face_data(face)?;
        let (a, d) = self.raster.face_metrics(face, self.size);
        shaped.ink_ascent = shaped.ink_ascent.max(a);
        shaped.ink_descent = shaped.ink_descent.max(d);
        let mut buffer = Buffer::new();
        buffer.push_str(&text[range.clone()]);
        buffer.set_direction(direction);
        buffer.guess_segment_properties();
        let shaper = ShaperFont::new(data.shaping_font());
        shape(
            &shaper,
            &mut buffer,
            ShapeOptions::new().features(self.features),
        )
        .map_err(|_| AureaError::RenderingFailed)?;
        let scale = self.size / data.units_per_em();
        let vertical = direction == Direction::TopToBottom;
        let deltas = if vertical {
            Vec::new()
        } else {
            let ids: Vec<u32> = buffer.glyph_infos().iter().map(|i| i.glyph_id).collect();
            data.advance_deltas(&ids)
        };
        for (i, (info, pos)) in buffer
            .glyph_infos()
            .iter()
            .zip(buffer.glyph_positions())
            .enumerate()
        {
            // Fonts measure up; the canvas measures down.
            let advance = if vertical {
                -pos.y_advance as f32
            } else {
                pos.x_advance as f32 + deltas.get(i).copied().unwrap_or(0.0)
            };
            shaped.glyphs.push(ShapedGlyph {
                face,
                id: u16::try_from(info.glyph_id).unwrap_or(0),
                cluster: range.start + info.cluster as usize,
                advance: advance * scale,
                offset_x: pos.x_offset as f32 * scale,
                offset_y: -(pos.y_offset as f32) * scale,
                rtl: direction == Direction::RightToLeft,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(cluster: usize, advance: f32, rtl: bool) -> ShapedGlyph {
        ShapedGlyph {
            face: FaceId(0),
            id: 1,
            cluster,
            advance,
            offset_x: 0.0,
            offset_y: 0.0,
            rtl,
        }
    }

    fn line(glyphs: Vec<ShapedGlyph>, len: usize) -> ShapedText {
        let width = glyphs.iter().map(|g| g.advance).sum();
        ShapedText {
            glyphs,
            width,
            ascent: 10.0,
            descent: 3.0,
            ink_ascent: 10.0,
            ink_descent: 3.0,
            size: 12.0,
            len,
            vertical: false,
        }
    }

    #[test]
    fn carets_and_hits_follow_clusters() {
        // "abc" as three glyphs of 10px.
        let t = line(
            vec![
                glyph(0, 10.0, false),
                glyph(1, 10.0, false),
                glyph(2, 10.0, false),
            ],
            3,
        );
        assert_eq!([0, 1, 2, 3].map(|i| t.caret_x(i)), [0.0, 10.0, 20.0, 30.0]);
        assert_eq!([2.0, 6.0, 25.0, 40.0].map(|x| t.hit(x)), [0, 1, 3, 3]);
    }

    #[test]
    fn a_ligature_splits_its_width_between_its_characters() {
        // "ffi" as one ligature glyph drawing bytes 0..3, then "x".
        let t = line(vec![glyph(0, 30.0, false), glyph(3, 10.0, false)], 4);
        assert_eq!([0, 1, 2, 3].map(|i| t.caret_x(i)), [0.0, 10.0, 20.0, 30.0]);
    }

    #[test]
    fn right_to_left_runs_read_from_the_right() {
        // Two RTL characters, shown in visual order: byte 2 on the left.
        let t = line(vec![glyph(2, 10.0, true), glyph(0, 10.0, true)], 4);
        assert_eq!(t.caret_x(0), 20.0);
        assert_eq!(t.caret_x(2), 10.0);
        // Left of the left glyph's middle is after it, in reading order.
        assert_eq!(t.hit(2.0), 4);
        assert_eq!(t.hit(18.0), 0);
    }

    #[test]
    fn marks_and_joiners_stay_with_their_base() {
        assert!(joins_previous('\u{0301}'));
        assert!(joins_previous('\u{200D}'));
        assert!(joins_previous('\u{FE0F}'));
        assert!(!joins_previous('a'));
        assert!(!joins_previous('가'));
    }
}
