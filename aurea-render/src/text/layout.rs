//! Laying text out in lines: hard breaks, wrapping at Unicode line break
//! opportunities, alignment, and the carets, hit tests and selection
//! highlights an editor needs.

use std::ops::Range;
use std::sync::Arc;

use unicode_bidi::{Direction, get_base_direction};
use unicode_linebreak::{BreakOpportunity, linebreaks};

use crate::geometry::{Point, Rect};
use crate::text::platform::{FontRef, TextRenderer};
use crate::text::shape::ShapedText;
use aurea_foundation::{AureaError, AureaResult};

/// Where lines may break inside a run of letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordBreak {
    /// Wherever Unicode allows: between words, and between CJK characters
    /// and Hangul syllables.
    #[default]
    Normal,
    /// Only where there is a space or punctuation, so a Korean (or CJK)
    /// word stays whole, as Korean is usually set. A word too long for a
    /// line still breaks inside.
    KeepAll,
}

/// Where lines sit across the layout's width (down a column, for vertical
/// text, where `Left` is the top and `Right` the bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Where the paragraph starts reading: left, or right for right-to-left
    /// text.
    #[default]
    Start,
    /// Where it ends reading.
    End,
    Left,
    Center,
    Right,
}

/// How [`TextLayout`] lays text out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutOptions {
    /// Wrap lines longer than this (columns taller than this, for vertical
    /// text); `None` breaks only at newlines. Must be finite and nonnegative.
    pub max_width: Option<f32>,
    pub align: TextAlign,
    /// Line spacing as a positive, finite multiple of the font's ascent plus descent.
    pub line_height: f32,
    pub word_break: WordBreak,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            max_width: None,
            align: TextAlign::Start,
            line_height: 1.0,
            word_break: WordBreak::Normal,
        }
    }
}

/// One laid-out line, or column of vertical text.
#[derive(Debug, Clone)]
pub struct LayoutLine {
    /// The bytes of the text it shows, without the newline ending it.
    pub range: Range<usize>,
    /// Its shaping, clusters counted from the line's start.
    pub shaped: Arc<ShapedText>,
    /// Where it starts, from the layout's left edge: a column's left edge.
    pub x: f32,
    /// Its top, and its baseline, from the layout's top: where a column's
    /// text starts, and the same.
    pub top: f32,
    pub baseline: f32,
    /// How tall the line is: how wide, for a column.
    pub height: f32,
    /// Whether its paragraph reads right to left.
    pub rtl: bool,
}

/// Text laid out in lines, or in columns from the right for vertical text.
/// Positions are from the layout's top-left.
#[derive(Debug, Clone)]
pub struct TextLayout {
    pub text: Arc<str>,
    pub lines: Vec<LayoutLine>,
    /// The widest line, or the wrapping width when one is set; for vertical
    /// text, all the columns together.
    pub width: f32,
    pub height: f32,
    /// Set in columns, top to bottom, the first on the right.
    pub vertical: bool,
}

impl TextLayout {
    /// The line holding byte `index`: the first whose range reaches it.
    fn line_of(&self, index: usize) -> Option<&LayoutLine> {
        self.lines
            .iter()
            .find(|l| index <= l.range.end)
            .or(self.lines.last())
    }

    /// The caret before byte `index`, as a zero-width rect a line tall (a
    /// zero-height one a column wide, for vertical text).
    pub fn caret(&self, index: usize) -> Rect {
        let Some(line) = self.line_of(index) else {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        };
        let at = index.clamp(line.range.start, line.range.end) - line.range.start;
        let along = line.shaped.caret_x(at);
        if self.vertical {
            Rect::new(line.x, line.top + along, line.height, 0.0)
        } else {
            Rect::new(line.x + along, line.top, 0.0, line.height)
        }
    }

    /// The byte index whose caret is nearest `point`.
    pub fn hit(&self, point: Point) -> usize {
        let line = if self.vertical {
            // Columns run right to left.
            self.lines
                .iter()
                .find(|l| point.x >= l.x)
                .or(self.lines.last())
        } else {
            self.lines
                .iter()
                .find(|l| point.y < l.top + l.height)
                .or(self.lines.last())
        };
        let Some(line) = line else {
            return 0;
        };
        let along = if self.vertical {
            point.y - line.top
        } else {
            point.x - line.x
        };
        line.range.start + line.shaped.hit(along)
    }

    /// Rects covering the text in `range`, one or more per line it spans:
    /// right-to-left runs inside a line can split it.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Rect> {
        let mut rects: Vec<Rect> = Vec::new();
        for line in &self.lines {
            let (a, b) = (
                range.start.max(line.range.start),
                range.end.min(line.range.end),
            );
            if a >= b {
                continue;
            }
            let spans = line
                .shaped
                .selection_spans(a - line.range.start..b - line.range.start);
            rects.extend(spans.into_iter().map(|(l, r)| {
                if self.vertical {
                    Rect::new(line.x, line.top + l, line.height, r - l)
                } else {
                    Rect::new(line.x + l, line.top, r - l, line.height)
                }
            }));
        }
        rects
    }
}

/// The width of `range` of a shaped paragraph by its clusters, leaving out
/// trailing whitespace, which hangs past a line's end.
fn span_width(text: &str, clusters: &[(usize, f32)], range: Range<usize>) -> f32 {
    let end = range.start + text[range.clone()].trim_end().len();
    clusters
        .iter()
        .filter(|(c, _)| (range.start..end).contains(c))
        .map(|(_, w)| w)
        .sum()
}

/// Each cluster's advance, in reading order.
fn cluster_widths(shaped: &ShapedText) -> Vec<(usize, f32)> {
    let mut clusters: Vec<(usize, f32)> = Vec::new();
    for g in &shaped.glyphs {
        match clusters.iter_mut().find(|(c, _)| *c == g.cluster) {
            Some((_, w)) => *w += g.advance,
            None => clusters.push((g.cluster, g.advance)),
        }
    }
    clusters.sort_by_key(|(c, _)| *c);
    clusters
}

/// Where a word too wide for a line alone breaks: after the most clusters
/// from `start` that fit, and at least one.
fn break_inside(paragraph: &str, clusters: &[(usize, f32)], start: usize, max_width: f32) -> usize {
    let after = clusters.iter().map(|(c, _)| *c).filter(|&c| c > start);
    after
        .clone()
        .take_while(|&c| span_width(paragraph, clusters, start..c) <= max_width)
        .last()
        .or_else(|| after.clone().next())
        .unwrap_or(paragraph.len())
}

/// Whether a line may break at byte `at` of `text` under `word_break`:
/// keeping words whole drops the breaks between two letters or digits.
fn may_break(text: &str, at: usize, word_break: WordBreak) -> bool {
    if word_break == WordBreak::Normal || at == text.len() {
        return true;
    }
    let before = text[..at].chars().next_back();
    let after = text[at..].chars().next();
    !matches!((before, after), (Some(b), Some(a)) if b.is_alphanumeric() && a.is_alphanumeric())
}

/// Where `paragraph` breaks to fit `max_width`, as line ranges: at Unicode
/// line break opportunities, or inside a word too long for a line alone.
fn wrap(
    paragraph: &str,
    shaped: &ShapedText,
    max_width: f32,
    word_break: WordBreak,
) -> Vec<Range<usize>> {
    let clusters = cluster_widths(shaped);
    let mut lines = Vec::new();
    let mut start = 0;
    let mut fits = None;
    let mut breaks = linebreaks(paragraph)
        .filter(|&(i, op)| op == BreakOpportunity::Allowed || i == paragraph.len())
        .map(|(i, _)| i)
        .filter(|&i| may_break(paragraph, i, word_break))
        .peekable();
    while let Some(&at) = breaks.peek() {
        // A forced break can cross an opportunity inside a shaped cluster.
        // Consume old opportunities so every iteration advances or emits a line.
        if at <= start {
            breaks.next();
            continue;
        }
        if span_width(paragraph, &clusters, start..at) <= max_width {
            fits = Some(at);
            breaks.next();
            continue;
        }
        let end = match fits.take() {
            Some(end) if end > start => end,
            _ => break_inside(paragraph, &clusters, start, max_width),
        };
        lines.push(start..end);
        start = end;
    }
    if start < paragraph.len() || lines.is_empty() {
        lines.push(start..paragraph.len());
    }
    lines
}

/// The paragraphs of `text`: they end at hard breaks, whose newlines are
/// in no line.
fn paragraphs(text: &str) -> Vec<Range<usize>> {
    const ENDS: [char; 4] = ['\n', '\r', '\u{2028}', '\u{2029}'];
    let mut paragraphs = Vec::new();
    let mut start = 0;
    for (at, op) in linebreaks(text) {
        if op == BreakOpportunity::Mandatory {
            let body = text[start..at].trim_end_matches(ENDS);
            paragraphs.push(start..start + body.len());
            start = at;
        }
    }
    // After a final newline comes an empty line, where the caret can go.
    if text.is_empty() || text.ends_with(ENDS) {
        paragraphs.push(text.len()..text.len());
    }
    paragraphs
}

/// Places `lines` across the layout by `options.align`, returning its
/// width.
fn align(lines: &mut [LayoutLine], text: &str, options: LayoutOptions) -> f32 {
    let content = |l: &LayoutLine| {
        let body = &text[l.range.clone()];
        let trailing = body.len() - body.trim_end().len();
        let hang: f32 = l
            .shaped
            .glyphs
            .iter()
            .filter(|g| g.cluster >= body.len() - trailing)
            .map(|g| g.advance)
            .sum();
        (l.shaped.width - hang, hang)
    };
    let width = options
        .max_width
        .unwrap_or_else(|| lines.iter().map(|l| content(l).0).fold(0.0, f32::max));
    for line in lines.iter_mut() {
        let (w, hang) = content(line);
        let right = match (options.align, line.rtl) {
            (TextAlign::Left, _) | (TextAlign::Start, false) | (TextAlign::End, true) => false,
            (TextAlign::Center, _) => {
                line.x = (width - w) / 2.0;
                continue;
            }
            _ => true,
        };
        // Trailing spaces hang past the end: on the left of a right-to-left
        // line.
        let left = if right { width - w } else { 0.0 };
        line.x = if line.rtl { left - hang } else { left };
    }

    width
}

/// Lays `text` out in `font`, shaping with `renderer`.
pub(crate) fn layout_text(
    text: &str,
    font: FontRef,
    options: LayoutOptions,
    renderer: &TextRenderer,
) -> AureaResult<TextLayout> {
    // Invalid widths can prevent wrapping from advancing at the paragraph's end.
    // Validate before shaping or entering the wrapping loop.
    if options.max_width.is_some_and(|w| !w.is_finite() || w < 0.0)
        || !options.line_height.is_finite()
        || options.line_height <= 0.0
        || !font.size.is_finite()
        || font.size < 0.0
    {
        return Err(AureaError::RenderingFailed);
    }
    let mut lines = Vec::new();
    let mut top = 0.0;
    for para in paragraphs(text) {
        let body = &text[para.clone()];
        let rtl = get_base_direction(body) == Direction::Rtl;
        let ranges = match options.max_width {
            Some(max) if !body.is_empty() => {
                let shaped = renderer.shape_directed(body, font, Some(rtl))?;
                wrap(body, &shaped, max, options.word_break)
            }
            _ => Vec::from([Range {
                start: 0,
                end: body.len(),
            }]),
        };
        for range in ranges {
            let shaped = renderer.shape_directed(&body[range.clone()], font, Some(rtl))?;
            let natural = shaped.ascent + shaped.descent;
            let height = natural * options.line_height;
            let baseline = top + shaped.ascent + (height - natural) / 2.0;
            lines.push(LayoutLine {
                range: para.start + range.start..para.start + range.end,
                shaped,
                x: 0.0,
                top,
                baseline,
                height,
                rtl,
            });
            top += height;
        }
    }

    let length = align(&mut lines, text, options);
    if !font.vertical {
        return Ok(TextLayout {
            text: text.into(),
            lines,
            width: length,
            height: top,
            vertical: false,
        });
    }
    // Laid out as lines, turned into columns: what was across a line is
    // down a column, and lines stack right to left.
    for line in &mut lines {
        let (along, across) = (line.x, line.top);
        line.x = top - across - line.height;
        line.top = along;
        line.baseline = along;
    }
    Ok(TextLayout {
        text: text.into(),
        lines,
        width: top,
        height: length,
        vertical: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Font;

    fn lay(text: &str, options: LayoutOptions) -> TextLayout {
        let r = TextRenderer::new();
        let font = Font::new("", 16.0);
        layout_text(text, (&font).into(), options, &r).expect("layout")
    }

    fn lines(l: &TextLayout) -> Vec<&str> {
        l.lines
            .iter()
            .map(|line| &l.text[line.range.clone()])
            .collect()
    }

    #[test]
    fn invalid_layout_dimensions_are_rejected() {
        let renderer = TextRenderer::new();
        let font = Font::new("", 16.0);
        for width in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let options = LayoutOptions {
                max_width: Some(width),
                ..LayoutOptions::default()
            };
            assert!(layout_text("word", (&font).into(), options, &renderer).is_err());
        }
        for height in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let options = LayoutOptions {
                line_height: height,
                ..LayoutOptions::default()
            };
            assert!(layout_text("word", (&font).into(), options, &renderer).is_err());
        }
        for size in [-1.0, f32::NAN, f32::INFINITY] {
            let font = Font::new("", size);
            assert!(
                layout_text("word", (&font).into(), LayoutOptions::default(), &renderer).is_err()
            );
        }
    }

    #[test]
    fn wrapping_skips_opportunities_crossed_by_a_cluster() {
        let text = "aa aa";
        let layout = lay(text, LayoutOptions::default());
        let mut shaped = (*layout.lines[0].shaped).clone();
        // Model a font that shapes across the space's line break opportunity.
        for glyph in &mut shaped.glyphs {
            if glyph.cluster < 4 {
                glyph.cluster = 0;
            }
        }
        let ranges = wrap(text, &shaped, 0.0, WordBreak::Normal);
        assert_eq!(ranges, [0..4, 4..5]);
    }

    #[test]
    fn zero_width_wrapping_makes_progress() {
        let layout = lay(
            "abc",
            LayoutOptions {
                max_width: Some(0.0),
                ..LayoutOptions::default()
            },
        );
        assert_eq!(lines(&layout).concat(), "abc");
        assert!(layout.lines.iter().all(|line| !line.range.is_empty()));
    }

    #[test]
    fn newlines_end_lines() {
        let l = lay("one\ntwo\r\n\nthree", LayoutOptions::default());
        assert_eq!(lines(&l), ["one", "two", "", "three"]);
        assert_eq!(lines(&lay("a\n", LayoutOptions::default())), ["a", ""]);
        assert!(l.lines.windows(2).all(|w| w[1].top > w[0].top));
    }

    #[test]
    fn long_lines_wrap_between_words() {
        let one = lay("word", LayoutOptions::default()).width;
        let wrapped = lay(
            "word word word word",
            LayoutOptions {
                max_width: Some(one * 2.6),
                ..LayoutOptions::default()
            },
        );
        assert_eq!(lines(&wrapped), ["word word ", "word word"]);
    }

    #[test]
    fn a_word_too_long_breaks_inside() {
        let l = lay(
            "abcdefghij",
            LayoutOptions {
                max_width: Some(30.0),
                ..LayoutOptions::default()
            },
        );
        assert!(l.lines.len() > 1);
        assert_eq!(lines(&l).concat(), "abcdefghij");
        assert!(l.lines.iter().all(|line| line.shaped.width <= 30.5));
    }

    #[test]
    fn alignment_places_lines() {
        let opts = |align| LayoutOptions {
            max_width: Some(200.0),
            align,
            ..LayoutOptions::default()
        };
        let left = lay("hi", opts(TextAlign::Left));
        let centre = lay("hi", opts(TextAlign::Center));
        let right = lay("hi", opts(TextAlign::Right));
        let w = left.lines[0].shaped.width;
        assert_eq!(left.lines[0].x, 0.0);
        assert!((centre.lines[0].x - (200.0 - w) / 2.0).abs() < 0.01);
        assert!((right.lines[0].x - (200.0 - w)).abs() < 0.01);
        // Hebrew starts on the right.
        let hebrew = lay("\u{05D0}\u{05D1}", opts(TextAlign::Start));
        assert!(hebrew.lines[0].x > 100.0);
    }

    /// Keeping words whole breaks Korean at its spaces, not between
    /// syllables.
    #[test]
    fn keep_all_keeps_korean_words_whole() {
        let text = "줄바꿈도 됩니다 한국어 문장";
        let word = lay("줄바꿈도", LayoutOptions::default()).width;
        let opts = |word_break| LayoutOptions {
            max_width: Some(word * 1.5),
            word_break,
            ..LayoutOptions::default()
        };
        let normal = lay(text, opts(WordBreak::Normal));
        let whole = lay(text, opts(WordBreak::KeepAll));
        let ends_in_word = |l: &TextLayout| {
            l.lines
                .iter()
                .take(l.lines.len() - 1)
                .any(|line| !l.text[line.range.clone()].ends_with(' '))
        };
        assert!(ends_in_word(&normal), "{:?}", lines(&normal));
        assert!(!ends_in_word(&whole), "{:?}", lines(&whole));
        assert_eq!(lines(&whole).concat(), text);
    }

    /// Vertical text is set in columns, the first on the right, with carets
    /// across them and hits found by column.
    #[test]
    fn vertical_text_lays_out_in_columns_from_the_right() {
        let r = TextRenderer::new();
        let font = Font::new("", 16.0).with_vertical(true);
        let l = layout_text(
            "縦書き\n二行目",
            (&font).into(),
            LayoutOptions::default(),
            &r,
        )
        .expect("layout");
        assert!(l.vertical);
        assert_eq!(l.lines.len(), 2);
        let (first, second) = (&l.lines[0], &l.lines[1]);
        assert!(first.x > second.x, "the first column is on the right");
        assert!(l.height > l.width / 2.0);
        let caret = l.caret(3);
        assert_eq!((caret.x, caret.height), (first.x, 0.0));
        assert!(caret.y > 0.0 && caret.width > 0.0);
        let index = second.range.start + 3;
        let below = l.caret(index);
        assert_eq!(l.hit(Point::new(second.x + 1.0, below.y + 0.1)), index);
        let sel = l.selection_rects(0..l.text.len());
        assert_eq!(sel.len(), 2);
        assert!(sel.iter().all(|r| r.height > r.width));
    }

    /// A column too tall wraps into the next, to the left.
    #[test]
    fn vertical_columns_wrap() {
        let r = TextRenderer::new();
        let font = Font::new("", 16.0).with_vertical(true);
        let options = LayoutOptions {
            max_width: Some(40.0),
            ..LayoutOptions::default()
        };
        let l = layout_text("縦書きの文字", (&font).into(), options, &r).expect("layout");
        assert!(l.lines.len() > 1);
        assert!(l.lines.windows(2).all(|w| w[1].x < w[0].x));
        assert!(l.lines.iter().all(|line| line.shaped.width <= 40.5));
    }

    #[test]
    fn carets_hits_and_selections_cross_lines() {
        let l = lay("ab\ncd", LayoutOptions::default());
        let second = l.caret(3);
        assert_eq!(second.x, 0.0);
        assert!(second.y > 0.0);
        assert_eq!(l.hit(Point::new(0.0, second.y + 1.0)), 3);
        assert_eq!(l.hit(Point::new(1000.0, 1.0)), 2);
        let sel = l.selection_rects(1..4);
        assert_eq!(sel.len(), 2, "one rect on each line: {sel:?}");
    }
}
