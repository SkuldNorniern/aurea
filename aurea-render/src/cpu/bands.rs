//! Drawing a frame as runs of rows on the pool's threads: rows apart touch pixels apart, and each
//! run draws every item in order, so the pixels match one thread's.

use std::iter;

use aurea_foundation::{AureaError, AureaResult};

use super::clip::ClipBox;
use super::counters::{CommandKind, Tally};
use super::damage::{should_render_item, uncovered_runs};
use super::pool;
use super::raster::band::Band;
use super::raster::fill::PathScratch;
use super::rasterizer::{CpuRasterizer, ImageScratch, color_to_u32};
use crate::scene::command::DrawCommand;
use crate::scene::display_list::DisplayItem;
use crate::types::Rect;

/// Damage smaller than this many pixels a band draws on one thread: spawning would cost more.
pub(super) const BAND_MIN_PIXELS: usize = 1 << 17;
/// Most threads a frame draws on.
pub(super) const MAX_BANDS: usize = 8;

/// What a band past the first draws with.
#[derive(Default)]
pub(super) struct BandScratch {
    path: PathScratch,
    row: ImageScratch,
}

/// What every band of a frame reads.
#[derive(Clone, Copy)]
pub(super) struct FramePlan<'f> {
    pub(super) items: &'f [DisplayItem],
    pub(super) occluded: &'f [bool],
    pub(super) dirty: &'f [bool],
    pub(super) under_clear: &'f [bool],
    pub(super) last_clear: Option<usize>,
    pub(super) dims: (u32, u32, u32, u32),
    pub(super) clip: ClipBox,
}

impl FramePlan<'_> {
    /// Draws the items in order into `buf`, inside `self.clip`.
    pub(super) fn draw(
        &self,
        buf: &mut Band<'_>,
        scratch_path: &mut PathScratch,
        scratch_row: &mut ImageScratch,
        tally: &mut Tally<'_>,
    ) -> AureaResult<()> {
        let (tiles_x, tiles_y, bw, bh) = self.dims;
        for (i, item) in self.items.iter().enumerate() {
            if let DrawCommand::Clear(color) = &item.command {
                let started = tally.start();
                let (color, clip) = (color_to_u32(*color), self.clip.intersect_opt(item.clip));
                let runs = self.last_clear == Some(i) || !self.occluded[i];
                if self.last_clear == Some(i) {
                    uncovered_runs(self.under_clear, self.dims, |run| {
                        CpuRasterizer::clear_rect(&run, color, buf, bw, clip);
                    });
                } else if !self.occluded[i] {
                    let whole = Rect::new(0.0, 0.0, bw as f32, bh as f32);
                    CpuRasterizer::clear_rect(&whole, color, buf, bw, clip);
                }
                tally.clear(runs, started);
                continue;
            }
            if !should_render_item(item, self.occluded, i, self.dirty, tiles_x, tiles_y) {
                tally.skip(self.occluded[i]);
                continue;
            }
            let started = tally.start();
            CpuRasterizer::render_clipped_item(
                item,
                buf,
                scratch_path,
                scratch_row,
                bw,
                self.clip.intersect_opt(item.clip),
            )?;
            tally.ran(CommandKind::of(&item.command), started);
        }
        Ok(())
    }
}

/// Draws `plan` as `bands` runs of rows, the first on this thread and the rest on the pool's: rows
/// apart touch pixels apart, and each band draws every item in order.
pub(super) fn draw_bands(
    plan: &FramePlan<'_>,
    bands: usize,
    frame: &mut [u32],
    (path, row): (&mut PathScratch, &mut ImageScratch),
    extra: &mut [BandScratch],
) -> AureaResult<()> {
    let (y0, y1) = (plan.clip.y0, plan.clip.y1);
    let width = plan.dims.2 as usize;
    let rows = (y1 - y0).div_ceil(u32::try_from(bands).unwrap_or(u32::MAX));
    let damaged = &mut frame[y0 as usize * width..y1 as usize * width];
    let scratches = iter::once((path, row)).chain(
        extra
            .iter_mut()
            .map(|scratch| (&mut scratch.path, &mut scratch.row)),
    );
    let mut results = [const { Ok(()) }; MAX_BANDS];
    let mut tasks: Vec<_> = damaged
        .chunks_mut(rows as usize * width)
        .zip(scratches)
        .zip(results.iter_mut())
        .enumerate()
        .map(|(k, ((chunk, (path, row)), result))| {
            let top = y0 + u32::try_from(k).unwrap_or(u32::MAX) * rows;
            let height = u32::try_from(chunk.len() / width).unwrap_or(u32::MAX);
            let plan = FramePlan {
                clip: ClipBox {
                    y0: top,
                    y1: top + height,
                    ..plan.clip
                },
                ..*plan
            };
            let start = top as usize * width;
            move || {
                let mut band = Band::new(&mut *chunk, start);
                *result = plan.draw(&mut band, &mut *path, &mut *row, &mut Tally(None));
            }
        })
        .collect();
    let finished = pool::run(&mut tasks);
    drop(tasks);
    if !finished {
        return Err(AureaError::RenderingFailed);
    }
    results.into_iter().collect()
}
