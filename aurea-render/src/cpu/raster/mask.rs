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
use crate::types::{Paint, Path, Point, Rect};
use aurea_foundation::AureaResult;
use std::ops::Range;
use std::sync::Arc;

/// Buffers a clipped item reuses, and the coverage they last held.
#[derive(Default)]
pub(crate) struct ClipScratch {
    /// Backdrop only where clip coverage is less than full.
    saved: Vec<u32>,
    /// Antialiased clip coverage per pixel of the box.
    coverage: Vec<u8>,
    /// One clip path's coverage, as words.
    words: Vec<u32>,
    /// Non-full-coverage runs in box-relative row-major order.
    edges: Vec<Range<usize>>,
    has_coverage: bool,
    /// The clip paths and box `coverage` was built for.
    key: Option<(Arc<[Path]>, ClipBox)>,
}

impl ClipScratch {
    fn save_backdrop(&mut self, buf: &Band<'_>, bw: u32, boxed: ClipBox) {
        let width = (boxed.x1 - boxed.x0) as usize;
        // Fully covered interiors need neither a saved backdrop nor another composite.
        // Runs may cross mask rows, whose width differs from the framebuffer stride.
        self.saved.clear();
        let saved_len = self.edges.iter().map(|r| r.end - r.start).sum();
        if let Some(smaller) = smaller_buffer(self.saved.capacity(), saved_len) {
            self.saved = smaller;
        }
        self.saved.reserve(saved_len);
        for run in &self.edges {
            let mut at = run.start;
            while at < run.end {
                let x = at % width;
                let count = (run.end - at).min(width - x);
                let dst = (boxed.y0 as usize + at / width) * bw as usize + boxed.x0 as usize + x;
                self.saved.extend_from_slice(&buf[dst..dst + count]);
                at += count;
            }
        }
    }

    fn restore_edges(&self, buf: &mut Band<'_>, bw: u32, boxed: ClipBox) {
        let width = (boxed.x1 - boxed.x0) as usize;
        let mut saved_at = 0;
        for run in &self.edges {
            let mut at = run.start;
            while at < run.end {
                let x = at % width;
                let count = (run.end - at).min(width - x);
                let dst = (boxed.y0 as usize + at / width) * bw as usize + boxed.x0 as usize + x;
                for (px, &coverage) in buf[dst..dst + count]
                    .iter_mut()
                    .zip(&self.coverage[at..at + count])
                {
                    let (old, cov) = (self.saved[saved_at], u32::from(coverage));
                    saved_at += 1;
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
                at += count;
            }
        }
    }

    fn find_edges(&mut self) {
        self.has_coverage = self.coverage.iter().any(|&c| c != 0);
        self.edges.clear();
        // Fragmented masks fall back to a full-box run: metadata must stay bounded.
        let limit = (self.coverage.len() / 16).clamp(1, 4096);
        let mut at = 0;
        while at < self.coverage.len() {
            if self.coverage[at] == 255 {
                at += 1;
                continue;
            }
            let start = at;
            while at < self.coverage.len() && self.coverage[at] != 255 {
                at += 1;
            }
            if self.edges.len() == limit {
                self.edges.clear();
                self.edges.push(0..self.coverage.len());
                break;
            }
            self.edges.push(start..at);
        }
    }
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
    let Some(paths) = item.clip_paths.as_deref() else {
        return item::render(item, buf, scratch_path, scratch_row, bw, clip);
    };
    if clip.is_empty() {
        return Ok(());
    }
    // Only the pixels the item can reach change, so only those are saved and restored.
    let boxed = item_clip_box(item, clip);
    if boxed.is_empty() {
        return Ok(());
    }
    let (w, h) = (boxed.x1 - boxed.x0, boxed.y1 - boxed.y0);
    let width = w as usize;
    let len = width * h as usize;
    let s = &mut scratch_row.clip;
    let cached = match (&s.key, item.clip_paths.as_ref()) {
        (Some((key, key_box)), Some(now)) => Arc::ptr_eq(key, now) && *key_box == boxed,
        _ => false,
    };
    if !cached {
        let origin = Point::new(boxed.x0 as f32, boxed.y0 as f32);
        s.key = None;
        s.coverage.clear();
        s.coverage.resize(len, 255);
        for path in paths {
            s.words.clear();
            s.words.resize(len, 0);
            coverage::draw_alpha(
                path,
                &Paint::new(),
                origin,
                (w, h),
                &mut s.words,
                scratch_path,
            )?;
            for (c, px) in s.coverage.iter_mut().zip(&s.words) {
                let m = (*px >> 24) as u8;
                *c = f32_to_u8_clamped((f32::from(*c) * f32::from(m) / 255.0).round());
            }
        }
        s.find_edges();
        s.key = item.clip_paths.clone().map(|p| (p, boxed));
    }
    if !s.has_coverage {
        return Ok(());
    }
    s.save_backdrop(buf, bw, boxed);
    item::render(item, buf, scratch_path, scratch_row, bw, clip)?;
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
                    assert_eq!(actual, expected, "cached pass {round}, {mode:?}");
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
        assert!(scratch.clip.coverage.len() > 1_000_000);
        scratch.clip.coverage = (0..1024)
            .map(|i| if i % 2 == 0 { 0 } else { 255 })
            .collect();
        scratch.clip.find_edges();
        assert_eq!(
            scratch.clip.edges.len(),
            1,
            "fragmentation uses one bounded run"
        );
        assert_eq!(scratch.clip.edges[0], 0..1024);
        scratch.clip.coverage.fill(0);
        scratch.clip.find_edges();
        assert!(!scratch.clip.has_coverage);
        scratch.clip.coverage.fill(255);
        scratch.clip.find_edges();
        assert!(
            scratch.clip.edges.is_empty(),
            "fully covered clip saves no backdrop"
        );
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
            let s = &mut scratch_row.clip;
            s.saved.clear();
            for y in boxed.y0..boxed.y1 {
                let row = y as usize * bw as usize;
                s.saved.extend_from_slice(&buf[row + x0..row + x1]);
            }
            item::render(item, buf, scratch_path, scratch_row, bw, clip)?;

            let s = &mut scratch_row.clip;
            let cached = match (&s.key, item.clip_paths.as_ref()) {
                (Some((key, key_box)), Some(now)) => Arc::ptr_eq(key, now) && *key_box == boxed,
                _ => false,
            };
            if !cached {
                let origin = Point::new(boxed.x0 as f32, boxed.y0 as f32);
                s.key = None;
                s.coverage.clear();
                s.coverage.resize(s.saved.len(), 255);
                for path in paths {
                    s.words.clear();
                    s.words.resize(s.saved.len(), 0);
                    coverage::draw_alpha(
                        path,
                        &Paint::new(),
                        origin,
                        (w, h),
                        &mut s.words,
                        scratch_path,
                    )?;
                    for (c, px) in s.coverage.iter_mut().zip(&s.words) {
                        let m = (*px >> 24) as u8;
                        *c = f32_to_u8_clamped((f32::from(*c) * f32::from(m) / 255.0).round());
                    }
                }
                s.key = item.clip_paths.clone().map(|p| (p, boxed));
            }
            let mut at = 0;
            for y in boxed.y0..boxed.y1 {
                let row = y as usize * bw as usize;
                for px in &mut buf[row + x0..row + x1] {
                    let (old, cov) = (s.saved[at], u32::from(s.coverage[at]));
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
