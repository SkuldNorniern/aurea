//! Clip paths: an antialiased mask, applied only where it is less than full.

use crate::cpu::clip::ClipBox;
use crate::cpu::damage::is_known_bounds;
use crate::cpu::raster::band::Band;
use crate::cpu::raster::fill::PathScratch;
use crate::cpu::raster::image::ImageScratch;
use crate::cpu::raster::{coverage, item};
use crate::memory::smaller_buffer;
use crate::numeric::f32_to_u8_clamped;
use crate::scene::display_list::DisplayItem;
use crate::types::{Paint, Path, PathCommand, Point, Rect};
use aurea_foundation::AureaResult;
use std::ops::Range;
use std::sync::Arc;

/// Most runs a row keeps; a row with more becomes one run from its first to its last.
const MAX_ROW_RUNS: usize = 8;
/// Padding around a clip path's bounds, enough that snapping to the pixel grid keeps its edge.
const PATH_PAD: f32 = 2.0;

/// Columns of a mask row where coverage is less than full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Run {
    x0: u32,
    x1: u32,
    /// Coverage is zero across the whole run.
    empty: bool,
}

/// A clip's coverage over a region of the frame, kept across items and frames so a frame of many
/// items under one clip scan converts it once.
struct Mask {
    paths: Arc<[Path]>,
    region: ClipBox,
    coverage: Vec<u8>,
    /// Where each row's runs start in `runs`, and one past the last row's.
    rows: Vec<usize>,
    runs: Vec<Run>,
}

impl Mask {
    fn holds(&self, paths: &Arc<[Path]>, boxed: ClipBox) -> bool {
        let inside = self.region.x0 <= boxed.x0
            && self.region.y0 <= boxed.y0
            && self.region.x1 >= boxed.x1
            && self.region.y1 >= boxed.y1;
        inside && (Arc::ptr_eq(&self.paths, paths) || *self.paths == **paths)
    }

    fn width(&self) -> usize {
        (self.region.x1 - self.region.x0) as usize
    }

    fn find_runs(&mut self) {
        self.rows.clear();
        self.runs.clear();
        let width = self.width().max(1);
        for row in self.coverage.chunks_exact(width) {
            self.rows.push(self.runs.len());
            let first = self.runs.len();
            let mut at = 0;
            while at < row.len() {
                if row[at] == 255 {
                    at += 1;
                    continue;
                }
                let start = at;
                let mut empty = true;
                while at < row.len() && row[at] != 255 {
                    empty &= row[at] == 0;
                    at += 1;
                }
                self.runs.push(Run {
                    x0: self.region.x0 + u32::try_from(start).unwrap_or(u32::MAX),
                    x1: self.region.x0 + u32::try_from(at).unwrap_or(u32::MAX),
                    empty,
                });
            }
            if self.runs.len() - first > MAX_ROW_RUNS {
                let (x0, x1) = (self.runs[first].x0, self.runs[self.runs.len() - 1].x1);
                self.runs.truncate(first);
                self.runs.push(Run {
                    x0,
                    x1,
                    empty: false,
                });
            }
        }
        self.rows.push(self.runs.len());
    }

    fn row_runs(&self, y: u32) -> &[Run] {
        let row = (y - self.region.y0) as usize;
        &self.runs[self.rows[row]..self.rows[row + 1]]
    }

    /// Whether nothing inside `boxed` shows, which the first row with coverage usually settles.
    fn hides(&self, boxed: ClipBox) -> bool {
        (boxed.y0..boxed.y1).all(|y| {
            self.row_runs(y)
                .iter()
                .any(|run| run.empty && run.x0 <= boxed.x0 && run.x1 >= boxed.x1)
        })
    }

    /// The columns inside `boxed` of each row's runs, with the row's first pixel in the frame.
    fn spans(&self, boxed: ClipBox, bw: u32) -> impl Iterator<Item = (u32, Range<usize>)> {
        (boxed.y0..boxed.y1).flat_map(move |y| {
            let row = y as usize * bw as usize;
            self.row_runs(y).iter().filter_map(move |run| {
                let (x0, x1) = (run.x0.max(boxed.x0), run.x1.min(boxed.x1));
                (x0 < x1).then(|| (y, row + x0 as usize..row + x1 as usize))
            })
        })
    }

    /// The coverage of frame pixels `span` in row `y`.
    fn coverage_of(&self, y: u32, span: &Range<usize>, bw: u32) -> &[u8] {
        let row = y as usize * bw as usize;
        let at = (y - self.region.y0) as usize * self.width();
        let x = span.start - row - self.region.x0 as usize;
        &self.coverage[at + x..at + x + span.len()]
    }
}

/// Buffers a clipped item reuses.
#[derive(Default)]
pub(crate) struct ClipScratch {
    /// Backdrop only where clip coverage is less than full.
    saved: Vec<u32>,
    /// One clip path's coverage, as words.
    words: Vec<u32>,
    mask: Option<Mask>,
}

impl ClipScratch {
    /// Makes `self.mask` hold `paths` over `region`, reusing its buffers.
    fn build(
        &mut self,
        paths: &Arc<[Path]>,
        region: ClipBox,
        scratch_path: &mut PathScratch,
    ) -> AureaResult<()> {
        let (w, h) = (region.x1 - region.x0, region.y1 - region.y0);
        let len = w as usize * h as usize;
        let origin = Point::new(region.x0 as f32, region.y0 as f32);
        let mut mask = self.mask.take().unwrap_or_else(|| Mask {
            paths: Arc::clone(paths),
            region,
            coverage: Vec::new(),
            rows: Vec::new(),
            runs: Vec::new(),
        });
        mask.paths = Arc::clone(paths);
        mask.region = region;
        if let Some(smaller) = smaller_buffer(mask.coverage.capacity(), len) {
            mask.coverage = smaller;
        }
        mask.coverage.clear();
        mask.coverage.resize(len, 255);
        for (i, path) in paths.iter().enumerate() {
            self.words.clear();
            self.words.resize(len, 0);
            coverage::draw_alpha(
                path,
                &Paint::new(),
                origin,
                (w, h),
                &mut self.words,
                scratch_path,
            )?;
            for (c, px) in mask.coverage.iter_mut().zip(&self.words) {
                let m = (*px >> 24) as u8;
                *c = if i == 0 {
                    m
                } else {
                    f32_to_u8_clamped((f32::from(*c) * f32::from(m) / 255.0).round())
                };
            }
        }
        mask.find_runs();
        self.mask = Some(mask);
        Ok(())
    }

    /// Keeps the frame's pixels under the mask's runs in `boxed`, the only ones the mask can change.
    fn save_backdrop(&mut self, buf: &Band<'_>, bw: u32, boxed: ClipBox) {
        let Some(mask) = &self.mask else {
            return;
        };
        self.saved.clear();
        for (_, span) in mask.spans(boxed, bw) {
            self.saved.extend_from_slice(&buf[span]);
        }
    }

    fn restore_edges(&self, buf: &mut Band<'_>, bw: u32, boxed: ClipBox) {
        let Some(mask) = &self.mask else {
            return;
        };
        let mut saved = self.saved.iter();
        for (y, span) in mask.spans(boxed, bw) {
            let coverage = mask.coverage_of(y, &span, bw);
            for ((px, &coverage), &old) in buf[span].iter_mut().zip(coverage).zip(&mut saved) {
                let cov = u32::from(coverage);
                *px = match cov {
                    255 => *px,
                    0 => old,
                    _ => {
                        let mix = |shift: u32| {
                            let (o, n) = ((old >> shift) & 0xFF, (*px >> shift) & 0xFF);
                            ((o * (255 - cov) + n * cov + 127) / 255) << shift
                        };
                        mix(24) | mix(16) | mix(8) | mix(0)
                    }
                };
            }
        }
    }
}

/// What `paths` can cover inside `clip`: every path's bounds, since coverage multiplies.
fn paths_region(paths: &[Path], clip: ClipBox) -> ClipBox {
    paths.iter().fold(clip, |region, path| {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for command in &path.commands {
            let mut add = |p: &Point| {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            };
            match command {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => add(p),
                PathCommand::QuadTo(c, p) => {
                    add(c);
                    add(p);
                }
                PathCommand::CubicTo(a, b, p) => {
                    add(a);
                    add(b);
                    add(p);
                }
                PathCommand::Close => {}
            }
        }
        if x0 > x1 {
            return ClipBox {
                x1: region.x0,
                ..region
            };
        }
        region.intersect(Rect::new(
            x0 - PATH_PAD,
            y0 - PATH_PAD,
            x1 - x0 + 2.0 * PATH_PAD,
            y1 - y0 + 2.0 * PATH_PAD,
        ))
    })
}

/// Padding around an item's bounds for antialiasing, enough that snapping to the pixel grid
/// keeps every pixel the item can draw.
const ITEM_PAD: f32 = 3.0;

/// Renders `item`, then keeps only what falls inside its clip paths, antialiased.
pub(crate) fn render_clipped(
    item: &DisplayItem,
    buf: &mut Band<'_>,
    scratch_path: &mut PathScratch,
    scratch_row: &mut ImageScratch,
    bw: u32,
    clip: ClipBox,
) -> AureaResult<()> {
    let Some(paths) = item.clip_paths.as_ref() else {
        return item::render(item, buf, scratch_path, scratch_row, bw, clip);
    };
    let region = paths_region(paths, clip);
    // Only the pixels the item can reach change, so only those are saved and restored.
    let boxed = item_clip_box(item, region);
    if boxed.is_empty() {
        return Ok(());
    }
    let s = &mut scratch_row.clip;
    if !s.mask.as_ref().is_some_and(|mask| mask.holds(paths, boxed)) {
        s.build(paths, region, scratch_path)?;
    } else if let Some(mask) = &mut s.mask {
        // A new Arc of the same paths: the next item needn't compare them again.
        mask.paths = Arc::clone(paths);
    }
    if s.mask.as_ref().is_some_and(|mask| mask.hides(boxed)) {
        return Ok(());
    }
    s.save_backdrop(buf, bw, boxed);
    item::render(item, buf, scratch_path, scratch_row, bw, region)?;
    scratch_row.clip.restore_edges(buf, bw, boxed);
    Ok(())
}

/// The pixels `item` can draw in, inside `clip`.
fn item_clip_box(item: &DisplayItem, clip: ClipBox) -> ClipBox {
    let b = item.bounds;
    if !is_known_bounds(b) {
        return clip;
    }
    clip.intersect(Rect::new(
        b.x - ITEM_PAD,
        b.y - ITEM_PAD,
        b.width + 2.0 * ITEM_PAD,
        b.height + 2.0 * ITEM_PAD,
    ))
}

#[cfg(test)]
mod tests {
    use super::{ClipScratch, Mask};
    use crate::cpu::clip::ClipBox;
    use crate::cpu::raster::band::Band;
    use crate::cpu::raster::fill::PathScratch;
    use crate::cpu::raster::image::ImageScratch;
    use crate::cpu::raster::{coverage, item};
    use crate::cpu::rasterizer::CpuRasterizer;
    use crate::numeric::f32_to_u8_clamped;
    use crate::scene::command::DrawCommand;
    use crate::scene::display_list::{CacheKey, DisplayIndex, DisplayItem};
    use crate::types::{BlendMode, Color, Paint, Path, PathCommand, Point, Rect};
    use aurea_foundation::AureaResult;
    use std::sync::Arc;

    /// The largest difference of any channel of any pixel.
    fn worst_difference(a: &[u32], b: &[u32]) -> u8 {
        a.iter()
            .zip(b)
            .flat_map(|(a, b)| a.to_be_bytes().into_iter().zip(b.to_be_bytes()))
            .map(|(a, b)| a.abs_diff(b))
            .max()
            .unwrap_or(0)
    }

    fn item(size: u32, paths: Vec<Path>, mode: BlendMode) -> DisplayItem {
        let rect = Rect::new(7.0, 5.0, size as f32 - 14.0, size as f32 - 10.0);
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey(0),
            rect,
            false,
            mode,
            DrawCommand::DrawRect(rect, Paint::new().color(Color::rgba(80, 170, 210, 143))),
        )
        .with_clip_paths(Some(paths.into()))
        .with_opacity(0.7)
    }

    fn draw(
        item: &DisplayItem,
        pixels: &mut [u32],
        width: u32,
        scratch: &mut ImageScratch,
        reference: bool,
    ) {
        let clip = ClipBox::surface(width, width).intersect(Rect::new(
            3.0,
            2.0,
            width as f32 - 6.0,
            width as f32 - 4.0,
        ));
        let draw = if reference {
            CpuRasterizer::reference_clipped_item
        } else {
            super::render_clipped
        };
        draw(
            item,
            &mut Band::whole(pixels),
            &mut PathScratch::default(),
            scratch,
            width,
            clip,
        )
        .expect("clipped item");
    }

    #[test]
    fn edge_only_clips_match_full_box_for_nested_masks_and_blending() {
        const SIZE: u32 = 93;
        let rounded = Path::rounded_rect(Rect::new(9.3, 6.7, 72.4, 76.1), 13.5);
        let triangle = Path {
            commands: vec![
                PathCommand::MoveTo(Point::new(5.4, 4.8)),
                PathCommand::LineTo(Point::new(89.1, 13.2)),
                PathCommand::LineTo(Point::new(64.7, 88.8)),
                PathCommand::Close,
            ],
            ..Path::new()
        };
        for paths in [
            vec![],
            vec![Path::new()],
            vec![rounded.clone()],
            vec![triangle.clone()],
            vec![rounded.clone(), triangle],
        ] {
            for mode in [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen] {
                let item = item(SIZE, paths.clone(), mode);
                let mut fast = ImageScratch::default();
                let mut slow = ImageScratch::default();
                for round in 0..3 {
                    let mut expected: Vec<u32> = (0..SIZE * SIZE)
                        .map(|i| i.wrapping_mul(123456789).wrapping_add(round))
                        .collect();
                    let mut actual = expected.clone();
                    draw(&item, &mut expected, SIZE, &mut slow, true);
                    draw(&item, &mut actual, SIZE, &mut fast, false);
                    // The reference's mask starts elsewhere, so its float edges round differently.
                    let worst = worst_difference(&actual, &expected);
                    assert!(worst <= 1, "off by {worst}, pass {round}, {mode:?}");
                }
            }
        }
    }

    #[test]
    fn rounded_clip_keeps_only_edges_and_fragmented_masks_bound_metadata() {
        let item = item(
            1024,
            vec![Path::rounded_rect(
                Rect::new(7.0, 5.0, 1010.0, 1014.0),
                24.0,
            )],
            BlendMode::Normal,
        );
        let mut scratch = ImageScratch::default();
        draw(&item, &mut vec![0; 1024 * 1024], 1024, &mut scratch, false);
        assert!(scratch.clip.saved.len() < 20000, "interior is not copied");
    }

    fn mask_of(row: Vec<u8>) -> Mask {
        let width = u32::try_from(row.len()).expect("a short row");
        let mut mask = Mask {
            paths: Arc::from(Vec::new()),
            region: ClipBox::surface(width, 1),
            coverage: row,
            rows: Vec::new(),
            runs: Vec::new(),
        };
        mask.find_runs();
        mask
    }

    #[test]
    fn a_mask_keeps_only_runs_below_full_and_bounds_them() {
        let box_all = ClipBox::surface(64, 1);
        let fragments = mask_of((0..64).map(|i| if i % 2 == 0 { 0 } else { 255 }).collect());
        assert_eq!(fragments.runs.len(), 1, "too many runs become one");
        assert_eq!((fragments.runs[0].x0, fragments.runs[0].x1), (0, 63));
        assert!(!fragments.hides(box_all));
        assert!(mask_of(vec![0; 64]).hides(box_all));
        assert!(!mask_of(vec![255; 64]).hides(box_all));
        assert!(
            mask_of(vec![255; 64]).runs.is_empty(),
            "no backdrop to save"
        );
    }

    #[test]
    fn a_mask_is_kept_for_equal_paths_and_any_box_inside_it() {
        let shape = || Path::rounded_rect(Rect::new(4.0, 4.0, 40.0, 40.0), 8.0);
        let first: Arc<[Path]> = vec![shape()].into();
        let mut scratch = ClipScratch::default();
        let region = ClipBox::surface(64, 64);
        scratch
            .build(&first, region, &mut PathScratch::default())
            .expect("mask");
        let mask = scratch.mask.as_ref().expect("mask");
        let inside = ClipBox {
            x0: 8,
            y0: 8,
            x1: 20,
            y1: 20,
        };
        assert!(mask.holds(&first, inside));
        assert!(
            mask.holds(&vec![shape()].into(), inside),
            "next frame's Arc"
        );
        let other: Arc<[Path]> =
            vec![Path::rounded_rect(Rect::new(4.0, 4.0, 40.0, 41.0), 8.0)].into();
        assert!(!mask.holds(&other, inside));
    }

    #[test]
    fn items_under_one_clip_match_each_drawn_with_its_own_mask() {
        const SIZE: u32 = 96;
        let clip: Arc<[Path]> =
            vec![Path::rounded_rect(Rect::new(5.5, 6.5, 80.0, 78.0), 11.0)].into();
        let rect = |x: f32, y: f32| Rect::new(x, y, 14.0, 12.0);
        let items: Vec<DisplayItem> = (0..7)
            .flat_map(|row| (0..7).map(move |col| (row, col)))
            .map(|(row, col)| {
                let at = rect(col as f32 * 14.0, row as f32 * 13.0);
                DisplayItem::new(
                    DisplayIndex(0),
                    CacheKey(0),
                    at,
                    false,
                    BlendMode::Normal,
                    DrawCommand::DrawRect(at, Paint::new().color(Color::rgba(200, 90, 40, 180))),
                )
                .with_clip_paths(Some(Arc::clone(&clip)))
            })
            .collect();
        let start: Vec<u32> = (0..SIZE * SIZE)
            .map(|i| i.wrapping_mul(2654435761) | 0xFF00_0000)
            .collect();
        let (mut shared, mut alone) = (start.clone(), start);
        let mut scratch = ImageScratch::default();
        for item in &items {
            draw(item, &mut shared, SIZE, &mut scratch, false);
            draw(item, &mut alone, SIZE, &mut ImageScratch::default(), true);
        }
        let worst = worst_difference(&shared, &alone);
        assert!(worst <= 1, "off by {worst}");
    }

    impl CpuRasterizer {
        #[allow(clippy::too_many_arguments)]
        fn reference_clipped_item(
            item: &DisplayItem,
            buf: &mut Band<'_>,
            scratch_path: &mut PathScratch,
            scratch_row: &mut ImageScratch,
            bw: u32,
            clip: ClipBox,
        ) -> AureaResult<()> {
            let Some(paths) = item.clip_paths.as_deref() else {
                return item::render(item, buf, scratch_path, scratch_row, bw, clip);
            };
            if clip.is_empty() {
                return Ok(());
            }
            // Only the pixels the item can reach change, so only those are saved and restored.
            let boxed = super::item_clip_box(item, clip);
            if boxed.is_empty() {
                return Ok(());
            }
            let (x0, x1) = (boxed.x0 as usize, boxed.x1 as usize);
            let (w, h) = (boxed.x1 - boxed.x0, boxed.y1 - boxed.y0);
            let mut saved = Vec::new();
            for y in boxed.y0..boxed.y1 {
                let row = y as usize * bw as usize;
                saved.extend_from_slice(&buf[row + x0..row + x1]);
            }
            item::render(item, buf, scratch_path, scratch_row, bw, clip)?;

            let origin = Point::new(boxed.x0 as f32, boxed.y0 as f32);
            let mut coverage_of = vec![255_u8; saved.len()];
            for path in paths {
                let mut words = vec![0_u32; saved.len()];
                coverage::draw_alpha(
                    path,
                    &Paint::new(),
                    origin,
                    (w, h),
                    &mut words,
                    scratch_path,
                )?;
                for (c, px) in coverage_of.iter_mut().zip(&words) {
                    let m = (*px >> 24) as u8;
                    *c = f32_to_u8_clamped((f32::from(*c) * f32::from(m) / 255.0).round());
                }
            }
            let mut at = 0;
            for y in boxed.y0..boxed.y1 {
                let row = y as usize * bw as usize;
                for px in &mut buf[row + x0..row + x1] {
                    let (old, cov) = (saved[at], u32::from(coverage_of[at]));
                    at += 1;
                    *px = match cov {
                        255 => *px,
                        0 => old,
                        _ => {
                            let mix = |shift: u32| {
                                let (o, n) = ((old >> shift) & 0xFF, (*px >> shift) & 0xFF);
                                ((o * (255 - cov) + n * cov + 127) / 255) << shift
                            };
                            mix(24) | mix(16) | mix(8) | mix(0)
                        }
                    };
                }
            }
            Ok(())
        }
    }
}
