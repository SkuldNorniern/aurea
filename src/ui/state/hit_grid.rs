//! Hits bucketed into grid cells, so the topmost hit under the pointer is found without a scan.

use std::mem;

use crate::render::{Point, Rect};

/// Hits bucketed into a grid of cells, so finding the one under the pointer looks at a cell's few
/// hits instead of all of them. Each list keeps draw order; the last match wins, as in a scan.
#[derive(Debug, Default)]
pub(super) struct HitGrid {
    origin: Point,
    cell: (f32, f32),
    /// The bounds' size; points on its far edges are inside, as `Rect::contains` has them.
    extent: (f32, f32),
    pub(super) cols: usize,
    rows: usize,
    /// Where each cell's hit indices start in `items`; one more entry than there are cells.
    starts: Vec<u32>,
    items: Vec<u32>,
    /// Hits spanning too many cells to list in each, or not finite; always checked.
    big: Vec<u32>,
    /// Where the next hit goes in each cell, while building.
    next: Vec<u32>,
}

impl HitGrid {
    /// Below this many hits a scan is as quick.
    const MIN_HITS: usize = 64;
    const MAX_SIDE: usize = 32;
    const MAX_CELLS_PER_HIT: usize = 16;

    pub(super) fn build(&mut self, rects: impl Iterator<Item = Rect> + Clone) {
        self.cols = 0;
        self.rows = 0;
        let count = rects.clone().count();
        if count < Self::MIN_HITS {
            return;
        }
        let Some(bounds) = rects.clone().filter(|r| listable(*r)).reduce(|a, b| {
            let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
            let (x1, y1) = (a.right().max(b.right()), a.bottom().max(b.bottom()));
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        }) else {
            return;
        };
        let side = |len: f32| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = (len / 48.0).ceil() as usize;
            n.clamp(1, Self::MAX_SIDE)
        };
        let (cols, rows) = (side(bounds.width), side(bounds.height));
        #[allow(clippy::cast_precision_loss)]
        let cell = (bounds.width / cols as f32, bounds.height / rows as f32);
        if !(cell.0 > 0.0 && cell.1 > 0.0) {
            return;
        }
        (self.origin, self.cell, self.cols, self.rows) =
            (Point::new(bounds.x, bounds.y), cell, cols, rows);
        self.extent = (bounds.width, bounds.height);
        self.starts.clear();
        self.starts.resize(cols * rows + 1, 0);
        self.items.clear();
        self.big.clear();
        // Two passes: count per cell, then place, keeping each cell's hits in draw order.
        let layout = (self.origin, self.cell, cols, rows);
        let mut counts = mem::take(&mut self.starts);
        self.each_cell(rects.clone(), layout, true, |_, c| counts[c + 1] += 1);
        for c in 1..counts.len() {
            counts[c] += counts[c - 1];
        }
        let mut items = mem::take(&mut self.items);
        items.resize(counts[cols * rows] as usize, 0);
        let mut next = mem::take(&mut self.next);
        next.clear();
        next.extend_from_slice(&counts[..cols * rows]);
        self.each_cell(rects, layout, false, |i, c| {
            items[next[c] as usize] = i;
            next[c] += 1;
        });
        self.starts = counts;
        self.items = items;
        self.next = next;
    }

    /// Calls `visit` with each hit's index and every cell it is listed in, in draw order; hits
    /// spanning too many cells, or not finite, go to `big` instead, on the `first` pass.
    fn each_cell(
        &mut self,
        rects: impl Iterator<Item = Rect>,
        layout: (Point, (f32, f32), usize, usize),
        first: bool,
        mut visit: impl FnMut(u32, usize),
    ) {
        let cols = layout.2;
        for (i, rect) in rects.enumerate() {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            if rect.is_empty() {
                continue;
            }
            let span = listable(rect).then(|| Self::span(layout, rect));
            let Some((c0, r0, c1, r1)) =
                span.filter(|(c0, r0, c1, r1)| (c1 - c0) * (r1 - r0) <= Self::MAX_CELLS_PER_HIT)
            else {
                if first {
                    self.big.push(i);
                }
                continue;
            };
            for row in r0..r1 {
                for col in c0..c1 {
                    visit(i, row * cols + col);
                }
            }
        }
    }

    /// The cells `rect` touches, edges included, as column and row ranges.
    fn span(
        (origin, cell, cols, rows): (Point, (f32, f32), usize, usize),
        rect: Rect,
    ) -> (usize, usize, usize, usize) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = |v: f32, origin: f32, size: f32, n: usize| {
            (((v - origin) / size).floor().max(0.0) as usize).min(n - 1)
        };
        let c0 = index(rect.x, origin.x, cell.0, cols);
        let r0 = index(rect.y, origin.y, cell.1, rows);
        let c1 = index(rect.right(), origin.x, cell.0, cols) + 1;
        let r1 = index(rect.bottom(), origin.y, cell.1, rows) + 1;
        (c0, r0, c1, r1)
    }

    /// The hits in the cell under `at` and the big ones, or `None` when there is no grid and every
    /// hit has to be scanned.
    pub(super) fn candidates(&self, at: Point) -> Option<(&[u32], &[u32])> {
        if self.cols == 0 {
            return None;
        }
        let (dx, dy) = (at.x - self.origin.x, at.y - self.origin.y);
        #[allow(clippy::cast_precision_loss)]
        let outside = !(0.0..=self.extent.0).contains(&dx) || !(0.0..=self.extent.1).contains(&dy);
        if outside {
            return Some((&[], &self.big));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (col, row) = (
            ((dx / self.cell.0) as usize).min(self.cols - 1),
            ((dy / self.cell.1) as usize).min(self.rows - 1),
        );
        let c = row * self.cols + col;
        let cell = &self.items[self.starts[c] as usize..self.starts[c + 1] as usize];
        Some((cell, &self.big))
    }
}

/// Whether `rect` can be listed in grid cells: finite and not empty.
fn listable(rect: Rect) -> bool {
    !rect.is_empty()
        && [rect.x, rect.y, rect.width, rect.height]
            .iter()
            .all(|v| v.is_finite())
}
