//! CPU rasterizer — flat framebuffer, no tile overhead.

use crate::cpu::bands::{BAND_MIN_PIXELS, BandScratch, FramePlan, MAX_BANDS, draw_bands};
use crate::cpu::clip::ClipBox;
use crate::cpu::counters::{FrameCounters, Tally};
use crate::cpu::damage::{
    FrameDamage, is_known_bounds, mark_occluded, resolve_frame_damage, union_rect,
};
use crate::cpu::raster::band::Band;
use crate::cpu::raster::fill::PathScratch;
use crate::cpu::raster::gradient;
use crate::cpu::raster::image::ImageScratch;
use crate::cpu::tiles::{
    mark_tile_range_dirty, tile_grid_dims, tile_range, union_dirty_tile_rects,
};
use crate::cpu::{pool, stroke};
use crate::memory::smaller_buffer;
use crate::recording::RecordingContext;
use crate::renderer::{DrawingContext, FrameOutcome, FrameOutput, Renderer};
use crate::scene::display_list::{CacheKey, DisplayItem, DisplayList};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::{Color, Rect};
use aurea_foundation::{AureaError, AureaResult};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;
use std::{mem, thread};

#[cfg(test)]
use crate::cpu::damage::*;
#[cfg(test)]
use crate::cpu::tiles::*;

/// Start and multiplier of the per-tile fold of item hashes in draw order.
const TILE_HASH_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
const TILE_HASH_MUL: u64 = 0xBF58_476D_1CE4_E5B9;

/// An item's cache key and bounds as one hash, for the tile fold.
fn item_hash(item: &DisplayItem) -> u64 {
    let mut h = DefaultHasher::new();
    item.cache_key.0.hash(&mut h);
    let b = item.bounds;
    for v in [b.x, b.y, b.width, b.height] {
        v.to_bits().hash(&mut h);
    }
    h.finish()
}

/// An offscreen frame as RGBA8 bytes, rows top to bottom, in physical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaPixels {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// `width * height * 4` bytes, straight (not premultiplied) alpha.
    pub rgba: Vec<u8>,
}

/// CPU software rasterizer drawing into a frame buffer.
pub struct CpuRasterizer {
    frame_buffer: Vec<u32>,
    width: u32,
    height: u32,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
    display_list: DisplayList,
    pending_damage: Option<Rect>,
    prev_items: Vec<(CacheKey, Rect)>,
    tile_hashes: Vec<u64>,
    scratch_path: PathScratch,
    scratch_row: ImageScratch,
    scratch_dirty: Vec<bool>,
    scratch_hashers: Vec<u64>,
    scratch_covered: Vec<bool>,
    scratch_occluded: Vec<bool>,
    scratch_under_clear: Vec<bool>,
    last_frame_damage: Option<Rect>,
    has_frame: bool,
    counting: bool,
    counters: Option<FrameCounters>,
    /// Scratch for the bands past the first, which draw on threads of their own.
    band_scratch: Vec<BandScratch>,
    threads: Option<usize>,
    band_min: usize,
}

impl CpuRasterizer {
    /// Creates a rasterizer for a `width` by `height` frame.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            frame_buffer: vec![0u32; (width * height) as usize],
            width,
            height,
            logical_width: width,
            logical_height: height,
            scale_factor: 1.0,
            display_list: DisplayList::new(),
            pending_damage: None,
            prev_items: Vec::new(),
            tile_hashes: Vec::new(),
            scratch_path: PathScratch::default(),
            scratch_row: ImageScratch::default(),
            scratch_dirty: Vec::new(),
            scratch_hashers: Vec::new(),
            scratch_covered: Vec::new(),
            scratch_occluded: Vec::new(),
            scratch_under_clear: Vec::new(),
            last_frame_damage: None,
            has_frame: false,
            counting: false,
            counters: None,
            band_scratch: Vec::new(),
            threads: None,
            band_min: BAND_MIN_PIXELS,
        }
    }

    /// Frees scratch the next frame regrows: fill, image and clip buffers, the band buffers, and
    /// this thread's gradient ramp and stroke buffers. The frame buffer and the state frame
    /// damage compares against stay. The band threads end too, with their thread-local buffers;
    /// the next frame that wants them starts them again.
    pub fn trim_memory(&mut self) {
        self.display_list.trim_memory();
        self.scratch_path = PathScratch::default();
        self.scratch_row = ImageScratch::default();
        self.scratch_dirty = Vec::new();
        self.scratch_hashers = Vec::new();
        self.scratch_covered = Vec::new();
        self.scratch_occluded = Vec::new();
        self.scratch_under_clear = Vec::new();
        self.band_scratch = Vec::new();
        gradient::trim_scratch();
        stroke::trim_scratch();
        pool::release();
    }

    /// Caps the threads a frame draws on; `None` uses the machine's cores, up to 8, and 1 draws
    /// on the caller's thread only. Each thread draws whole rows, items in their order, so the
    /// pixels are the same either way.
    pub fn set_threads(&mut self, threads: Option<usize>) {
        self.threads = threads.map(|n| n.max(1));
        if threads == Some(1) {
            self.band_scratch = Vec::new();
        }
    }

    /// How many row bands to draw `clip` in: one when counting, which times items one by one.
    fn band_count(&self, clip: ClipBox) -> usize {
        if self.counting || clip.is_empty() {
            return 1;
        }
        let threads = self.threads.unwrap_or_else(|| {
            thread::available_parallelism().map_or(1, |n| n.get().min(MAX_BANDS))
        });
        let (w, h) = ((clip.x1 - clip.x0) as usize, (clip.y1 - clip.y0) as usize);
        threads.min(w * h / self.band_min.max(1)).min(h).max(1)
    }

    /// Turns per-frame counters on or off. Turning them off drops the stored counters.
    pub fn set_counting(&mut self, on: bool) {
        self.counting = on;
        if !on {
            self.counters = None;
        }
    }

    /// The last `end_frame`'s counters; `None` while counting is off or before one ran.
    pub fn frame_counters(&self) -> Option<&FrameCounters> {
        self.counters.as_ref()
    }

    /// The last frame's pixels as [`FrameOutput`] describes them: `0xAARRGGBB` words, sRGB,
    /// straight alpha, rows top to bottom.
    pub fn pixels(&self) -> &[u32] {
        &self.frame_buffer
    }

    /// Draws `draw` on a transparent `width` by `height` logical canvas at `scale`, and returns
    /// the physical pixels as straight RGBA8.
    pub fn draw_offscreen(
        width: u32,
        height: u32,
        scale: f32,
        draw: impl FnOnce(&mut dyn DrawingContext) -> AureaResult<()>,
    ) -> AureaResult<RgbaPixels> {
        let mut r = Self::new(0, 0);
        r.configure(SurfaceInfo::new(width, height, scale))?;
        {
            let mut ctx = r.begin_frame()?;
            ctx.clear(Color::rgba(0, 0, 0, 0))?;
            draw(ctx.as_mut())?;
        }
        r.end_frame()?;
        let (width, height) = r.pixel_size();
        let mut rgba = vec![0u8; r.pixels().len() * 4];
        for (out, &word) in rgba.as_chunks_mut::<4>().0.iter_mut().zip(r.pixels()) {
            let [a, red, green, blue] = word.to_be_bytes();
            *out = [red, green, blue, a];
        }
        Ok(RgbaPixels {
            width,
            height,
            rgba,
        })
    }

    /// The frame buffer as a raw pointer, its length in bytes, and its size.
    pub fn get_buffer(&self) -> (*const u8, usize, u32, u32) {
        (
            self.frame_buffer.as_ptr() as *const u8,
            self.frame_buffer.len() * 4,
            self.width,
            self.height,
        )
    }

    /// The current display list.
    pub fn display_list(&self) -> &DisplayList {
        &self.display_list
    }

    /// The size it draws at, in logical pixels.
    pub fn logical_size(&self) -> (u32, u32) {
        (self.logical_width, self.logical_height)
    }

    /// The buffer's size, in device pixels.
    pub fn pixel_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Physical pixels per logical one.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub(crate) fn render_recorded(&mut self, list: &mut DisplayList) -> AureaResult<FrameOutcome> {
        mem::swap(&mut self.display_list, list);
        let outcome = self.end_frame();
        mem::swap(&mut self.display_list, list);
        outcome
    }

    /// What changed since the last frame. Items the two lists share at the start and the end are
    /// matched by cache key, so drawing a different number of items in the middle (a tile pyramid
    /// changing level) damages only the middle, not everything after it.
    fn diff_damage(&self) -> FrameDamage {
        let new_items = self.display_list.items();
        let old_items = &self.prev_items;
        let same = |n: &DisplayItem, o: &(CacheKey, Rect)| n.cache_key == o.0;
        let prefix = new_items
            .iter()
            .zip(old_items)
            .take_while(|(n, o)| same(n, o))
            .count();
        let suffix = new_items[prefix..]
            .iter()
            .rev()
            .zip(old_items[prefix..].iter().rev())
            .take_while(|(n, o)| same(n, o))
            .count();
        let new_mid = &new_items[prefix..new_items.len() - suffix];
        let old_mid = &old_items[prefix..old_items.len() - suffix];

        let mut acc: Option<Rect> = None;
        let mut add = |r: Rect| -> bool {
            if !is_known_bounds(r) {
                return false;
            }
            acc = Some(acc.map_or(r, |a| union_rect(a, r)));
            true
        };
        if new_mid.len() == old_mid.len() {
            for (n, o) in new_mid.iter().zip(old_mid) {
                if !(same(n, o) || (add(n.bounds) && add(o.1))) {
                    return FrameDamage::Full;
                }
            }
        } else {
            let bounds = new_mid
                .iter()
                .map(|n| n.bounds)
                .chain(old_mid.iter().map(|o| o.1));
            for r in bounds {
                if !add(r) {
                    return FrameDamage::Full;
                }
            }
        }

        match acc {
            Some(r) => FrameDamage::Region(r),
            None => FrameDamage::Unchanged,
        }
    }

    fn compute_dirty_tiles(
        &mut self,
        damage: Option<Rect>,
        forced: Option<Rect>,
        tiles_x: u32,
        tiles_y: u32,
    ) -> Vec<bool> {
        let tile_count = (tiles_x * tiles_y) as usize;
        if self.tile_hashes.len() != tile_count {
            self.tile_hashes = vec![0u64; tile_count];
        }

        let mut dirty = mem::take(&mut self.scratch_dirty);
        dirty.clear();
        dirty.resize(tile_count, false);

        let range = match damage {
            Some(rect) => tile_range(rect, tiles_x, tiles_y),
            None => (0, 0, tiles_x, tiles_y),
        };
        self.refine_tile_hashes(range, (tiles_x, tiles_y), &mut dirty);

        if damage.is_none() {
            dirty.fill(true);
        }
        if let Some(rect) = forced {
            mark_tile_range_dirty(rect, tiles_x, tiles_y, &mut dirty);
        }
        dirty
    }

    fn refine_tile_hashes(
        &mut self,
        (tx0, ty0, tx1, ty1): (u32, u32, u32, u32),
        (tiles_x, tiles_y): (u32, u32),
        dirty: &mut [bool],
    ) {
        if tx0 >= tx1 || ty0 >= ty1 {
            return;
        }
        let range_w = (tx1 - tx0) as usize;
        let mut tiles = mem::take(&mut self.scratch_hashers);
        tiles.clear();
        tiles.resize(range_w * (ty1 - ty0) as usize, TILE_HASH_SEED);

        for item in self.display_list.items() {
            if !is_known_bounds(item.bounds) {
                continue;
            }
            let (itx0, ity0, itx1, ity1) = tile_range(item.bounds, tiles_x, tiles_y);
            let lo_x = itx0.max(tx0);
            let hi_x = itx1.min(tx1);
            let lo_y = ity0.max(ty0);
            let hi_y = ity1.min(ty1);
            if lo_x >= hi_x || lo_y >= hi_y {
                continue;
            }
            // Hashed once, then folded into each tile it covers, in draw order.
            let item_hash = item_hash(item);
            for ty in lo_y..hi_y {
                let row = (ty - ty0) as usize * range_w;
                for tile in &mut tiles[row + (lo_x - tx0) as usize..row + (hi_x - tx0) as usize] {
                    *tile = (tile.rotate_left(23) ^ item_hash).wrapping_mul(TILE_HASH_MUL);
                }
            }
        }

        for ty in ty0..ty1 {
            for tx in tx0..tx1 {
                let new_hash = tiles[(ty - ty0) as usize * range_w + (tx - tx0) as usize];
                let idx = (ty * tiles_x + tx) as usize;
                if new_hash != self.tile_hashes[idx] {
                    dirty[idx] = true;
                }
                self.tile_hashes[idx] = new_hash;
            }
        }
        self.scratch_hashers = tiles;
    }

    fn capture_prev_items(&mut self) {
        self.prev_items.clear();
        self.prev_items.extend(
            self.display_list
                .items()
                .iter()
                .map(|item| (item.cache_key, item.bounds)),
        );
    }

    fn resize_buffer(&mut self, info: &SurfaceInfo) -> AureaResult<()> {
        let (lw, lh) = (info.width, info.height);
        let scale = info.scale_factor.max(1.0);
        let (rw, rh) = info.pixel_size();
        let new_len = (rw as usize)
            .checked_mul(rh as usize)
            .ok_or(AureaError::RenderingFailed)?;
        self.frame_buffer
            .try_reserve(new_len.saturating_sub(self.frame_buffer.len()))
            .map_err(|_| AureaError::RenderingFailed)?;
        if let Some(compact) = smaller_buffer(self.frame_buffer.capacity(), new_len) {
            self.frame_buffer = compact;
        }
        self.frame_buffer.clear();
        self.frame_buffer.resize(new_len, 0);
        self.has_frame = false;
        self.logical_width = lw;
        self.logical_height = lh;
        self.width = rw;
        self.height = rh;
        self.scale_factor = scale;
        self.display_list.clear();
        self.pending_damage = None;
        self.prev_items.clear();
        self.tile_hashes.clear();
        self.last_frame_damage = None;
        Ok(())
    }
}

impl Renderer for CpuRasterizer {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.configure(info)
    }

    fn trim_memory(&mut self) {
        Self::trim_memory(self);
    }

    fn configure(&mut self, info: SurfaceInfo) -> AureaResult<()> {
        self.resize_buffer(&info)
    }

    fn resize(&mut self, lw: u32, lh: u32) -> AureaResult<()> {
        self.resize_buffer(&SurfaceInfo::new(lw, lh, self.scale_factor))
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext + '_>> {
        self.display_list.clear();
        let mut ctx = RecordingContext::new(
            &mut self.display_list,
            self.logical_width,
            self.logical_height,
        );
        ctx.set_scale_factor(self.scale_factor);
        ctx.set_path_clips(true);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<FrameOutcome> {
        if !self.counting {
            return self.render_frame(None);
        }
        let started = Instant::now();
        let mut counters = FrameCounters::default();
        let outcome = self.render_frame(Some(&mut counters));
        counters.total = started.elapsed();
        self.counters = Some(counters);
        outcome
    }

    fn frame_output(&self) -> Option<FrameOutput<'_>> {
        if !self.has_frame {
            return None;
        }
        FrameOutput::new(&self.frame_buffer, self.width, self.height)
    }

    fn last_frame_damage(&self) -> Option<Rect> {
        self.last_frame_damage
    }

    fn cleanup(&mut self) {
        self.display_list.clear();
        self.pending_damage = None;
        self.prev_items.clear();
        self.tile_hashes.clear();
    }

    fn set_damage(&mut self, damage: Option<Rect>) {
        let scale = self.scale_factor;
        self.pending_damage = damage.map(|rect| {
            Rect::new(
                rect.x * scale,
                rect.y * scale,
                rect.width * scale,
                rect.height * scale,
            )
        });
    }

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }
}

impl CpuRasterizer {
    fn render_frame(&mut self, counters: Option<&mut FrameCounters>) -> AureaResult<FrameOutcome> {
        let mut tally = Tally(counters);
        tally.set_items(self.display_list.items().len());
        let (bw, bh) = (self.width, self.height);
        let pending = self.pending_damage.take();
        let diff = self.diff_damage();

        let Some(damage) = resolve_frame_damage(pending, diff, bw, bh) else {
            return Ok(FrameOutcome::Unchanged);
        };
        if damage.is_some_and(|r| {
            r.width <= 0.0
                || r.height <= 0.0
                || r.x >= bw as f32
                || r.y >= bh as f32
                || r.x + r.width <= 0.0
                || r.y + r.height <= 0.0
        }) {
            self.last_frame_damage = None;
            self.capture_prev_items();
            return Ok(FrameOutcome::Unchanged);
        }

        let (tiles_x, tiles_y) = tile_grid_dims(bw, bh);
        let dirty_tiles = self.compute_dirty_tiles(damage, pending, tiles_x, tiles_y);

        self.last_frame_damage = union_dirty_tile_rects(&dirty_tiles, tiles_x, tiles_y, bw, bh);
        if self.last_frame_damage.is_none() {
            self.scratch_dirty = dirty_tiles;
            self.capture_prev_items();
            return Ok(FrameOutcome::Unchanged);
        }

        let mut dirty_tiles = dirty_tiles;
        if let Some(region) = self.last_frame_damage {
            mark_tile_range_dirty(region, tiles_x, tiles_y, &mut dirty_tiles);
        }
        tally.set_dirty_tiles(&dirty_tiles);

        let surface_clip = match self.last_frame_damage {
            Some(region) => ClipBox::surface(bw, bh).intersect(region),
            None => ClipBox::surface(bw, bh),
        };
        let items = self.display_list.items();
        let mut occluded = mem::take(&mut self.scratch_occluded);
        let last_clear = mark_occluded(
            items,
            (tiles_x, tiles_y, bw, bh),
            &mut self.scratch_covered,
            &mut occluded,
            &mut self.scratch_under_clear,
        );
        let plan = FramePlan {
            items,
            occluded: &occluded,
            dirty: &dirty_tiles,
            under_clear: &self.scratch_under_clear,
            last_clear,
            dims: (tiles_x, tiles_y, bw, bh),
            clip: surface_clip,
        };
        let bands = self.band_count(surface_clip);
        let drawn = if bands <= 1 {
            plan.draw(
                &mut Band::whole(&mut self.frame_buffer),
                &mut self.scratch_path,
                &mut self.scratch_row,
                &mut tally,
            )
        } else {
            self.band_scratch
                .resize_with(bands - 1, BandScratch::default);
            let first = (&mut self.scratch_path, &mut self.scratch_row);
            draw_bands(
                &plan,
                bands,
                &mut self.frame_buffer,
                first,
                &mut self.band_scratch,
            )
        };
        drawn?;

        self.scratch_occluded = occluded;
        self.scratch_dirty = dirty_tiles;
        self.capture_prev_items();

        self.has_frame = true;
        Ok(FrameOutcome::Presented)
    }
}

#[cfg(test)]
mod diff_damage_tests {
    use super::*;
    use crate::cpu::damage::FrameDamage;
    use crate::cpu::raster::gradient;

    use crate::renderer::{FrameOutcome, Renderer};
    use crate::scene::command::DrawCommand;
    use crate::scene::display_list::{CacheKey, DisplayIndex, DisplayItem};
    use crate::surface::{Surface, SurfaceInfo};
    use crate::types::{
        BlendMode, Color, GradientStop, LinearGradient, Paint, Path, PathCommand, Point, Rect,
    };

    fn item(key: u64, bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(key),
            bounds,
            false,
            BlendMode::Normal,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    #[test]
    fn identical_list_reports_unchanged() {
        let mut r = CpuRasterizer::new(100, 100);
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        r.display_list.push(item(1, bounds));
        r.prev_items.push((CacheKey::from_hash(1), bounds));

        assert!(matches!(r.diff_damage(), FrameDamage::Unchanged));
    }

    #[test]
    fn a_middle_that_changes_length_damages_only_the_middle() {
        let mut r = CpuRasterizer::new(100, 100);
        let rect = |x: f32| Rect::new(x, 0.0, 10.0, 10.0);
        for (key, x) in [(1, 0.0), (2, 20.0), (3, 40.0), (9, 80.0)] {
            r.prev_items.push((CacheKey::from_hash(key), rect(x)));
        }
        for (key, x) in [(1, 0.0), (4, 30.0), (9, 80.0)] {
            r.display_list.push(item(key, rect(x)));
        }
        let FrameDamage::Region(damage) = r.diff_damage() else {
            panic!("expected a region");
        };
        assert_eq!(
            damage,
            Rect::new(20.0, 0.0, 30.0, 10.0),
            "the panel after stays clean"
        );
    }

    #[test]
    fn offscreen_changes_skip_presentation_and_update_previous_items() {
        let mut r = CpuRasterizer::new(32, 32);
        for step in 0..3 {
            {
                let mut ctx = r.begin_frame().expect("begin");
                ctx.clear(Color::rgb(10, 20, 30)).expect("clear");
                ctx.draw_rect(Rect::new(64.0 + step as f32, 4.0, 2.0, 2.0), &Paint::new())
                    .expect("offscreen rect");
            }
            let expected = if step == 0 {
                FrameOutcome::Presented
            } else {
                FrameOutcome::Unchanged
            };
            assert_eq!(r.end_frame().expect("end"), expected);
            assert!(r.pixels().iter().all(|&pixel| pixel == 0xff0a_141e));
            assert!(matches!(r.diff_damage(), FrameDamage::Unchanged));
        }
    }

    #[test]
    fn changed_item_unions_old_and_new_bounds() {
        let mut r = CpuRasterizer::new(100, 100);
        let old_bounds = Rect::new(0.0, 0.0, 8.0, 8.0);
        let new_bounds = Rect::new(20.0, 20.0, 8.0, 8.0);
        r.display_list.push(item(2, new_bounds));
        r.prev_items.push((CacheKey::from_hash(1), old_bounds));

        match r.diff_damage() {
            FrameDamage::Region(rect) => assert_eq!(rect, Rect::new(0.0, 0.0, 28.0, 28.0)),
            other => panic!("expected Region, got {other:?}"),
        }
    }

    #[test]
    fn unknown_bounds_change_forces_full_damage() {
        let mut r = CpuRasterizer::new(100, 100);
        let unknown = Rect::new(0.0, 0.0, 0.0, 0.0);
        r.display_list.push(item(2, unknown));
        r.prev_items.push((CacheKey::from_hash(1), unknown));

        assert!(matches!(r.diff_damage(), FrameDamage::Full));
    }

    #[test]
    fn appended_item_contributes_only_its_own_bounds() {
        let mut r = CpuRasterizer::new(100, 100);
        let shared_bounds = Rect::new(0.0, 0.0, 10.0, 10.0);
        let new_bounds = Rect::new(50.0, 50.0, 4.0, 4.0);
        r.display_list.push(item(1, shared_bounds));
        r.display_list.push(item(2, new_bounds));
        r.prev_items.push((CacheKey::from_hash(1), shared_bounds));

        match r.diff_damage() {
            FrameDamage::Region(rect) => assert_eq!(rect, new_bounds),
            other => panic!("expected Region, got {other:?}"),
        }
    }

    #[test]
    fn static_scene_skips_present_on_repeat_frame() {
        use crate::types::Paint;

        let mut r = CpuRasterizer::new(32, 32);
        let paint = Paint::new();

        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(10, 20, 30)).unwrap();
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &paint)
            .unwrap();
        drop(ctx);
        assert_eq!(r.end_frame().unwrap(), FrameOutcome::Presented);
        assert!(r.frame_output().is_some());

        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(10, 20, 30)).unwrap();
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &paint)
            .unwrap();
        drop(ctx);

        assert_eq!(r.end_frame().unwrap(), FrameOutcome::Unchanged);
        assert!(r.frame_output().is_some());
    }

    #[test]
    fn each_renderer_publishes_only_its_own_frame() {
        use crate::types::Paint;

        let mut first = CpuRasterizer::new(16, 16);
        {
            let mut ctx = first.begin_frame().unwrap();
            ctx.clear(Color::rgb(255, 0, 0)).unwrap();
            ctx.draw_rect(Rect::new(1.0, 1.0, 4.0, 4.0), &Paint::new())
                .unwrap();
        }
        first.end_frame().unwrap();

        let mut second = CpuRasterizer::new(32, 32);
        {
            let mut ctx = second.begin_frame().unwrap();
            ctx.clear(Color::rgb(0, 0, 255)).unwrap();
            ctx.draw_rect(Rect::new(2.0, 2.0, 8.0, 8.0), &Paint::new())
                .unwrap();
        }
        second.end_frame().unwrap();

        let a = first.frame_output().expect("first drew a frame");
        let b = second.frame_output().expect("second drew a frame");

        assert_eq!((a.width, a.height), (16, 16), "the first renderer's own");
        assert_eq!((b.width, b.height), (32, 32), "and the second's own");
        assert_ne!(a.pixels(), b.pixels(), "two buffers, not one shared slot");
    }

    #[test]
    fn a_resized_renderer_has_no_frame_to_publish() {
        let mut r = CpuRasterizer::new(16, 16);
        {
            let mut ctx = r.begin_frame().unwrap();
            ctx.clear(Color::rgb(255, 0, 0)).unwrap();
        }
        r.end_frame().unwrap();
        assert!(r.frame_output().is_some());

        r.resize(32, 32).unwrap();

        assert!(
            r.frame_output().is_none(),
            "an empty buffer of the new size is not a frame"
        );
    }

    #[test]
    fn reinitialization_reuses_storage_and_invalidates_old_frame_state() {
        let mut r = CpuRasterizer::new(32, 32);
        {
            r.begin_frame()
                .expect("begin")
                .clear(Color::rgb(255, 0, 0))
                .expect("clear");
        }
        r.end_frame().expect("end");
        r.set_damage(Some(Rect::new(4.0, 4.0, 1.0, 1.0)));
        let storage = r.frame_buffer.as_ptr();
        r.init(
            Surface::Cpu,
            SurfaceInfo {
                width: 16,
                height: 16,
                scale_factor: 1.0,
                pixels: None,
            },
        )
        .expect("reinit");
        assert_eq!(r.frame_buffer.as_ptr(), storage);
        assert!(r.frame_output().is_none());
        assert!(r.display_list.items().is_empty());
        assert!(r.pending_damage.is_none());
        assert!(r.pixels().iter().all(|&pixel| pixel == 0));
    }

    #[test]
    fn substantial_window_shrink_releases_oversized_framebuffer_capacity() {
        let mut r = CpuRasterizer::new(2048, 1024);
        let large_capacity = r.frame_buffer.capacity();
        r.resize(128, 128).expect("shrink");
        assert_eq!(r.pixels().len(), 128 * 128);
        assert!(r.frame_buffer.capacity() <= 2 * r.pixels().len());
        assert!(large_capacity - r.frame_buffer.capacity() >= (1 << 20));
        let small_storage = r.frame_buffer.as_ptr();
        r.resize(120, 120).expect("small resize");
        assert_eq!(
            r.frame_buffer.as_ptr(),
            small_storage,
            "modest changes retain useful slack"
        );
    }

    #[test]
    fn trim_memory_keeps_the_frame_and_the_next_frame_draws_as_before() {
        fn render(r: &mut CpuRasterizer, tint: u8) {
            {
                let mut ctx = r.begin_frame().expect("begin");
                ctx.draw_rect(
                    Rect::new(0.0, 0.0, 64.0, 48.0),
                    &Paint::new().color(Color::rgb(tint, 0, 0)),
                )
                .expect("rect");
                let stops = vec![
                    GradientStop {
                        offset: 0.0,
                        color: Color::rgb(0, 0, 0),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::rgb(255, 255, 255),
                    },
                ];
                let gradient = LinearGradient {
                    start: Point::new(4.0, 0.0),
                    end: Point::new(60.0, 0.0),
                    stops,
                };
                ctx.fill_linear_gradient(&gradient, Rect::new(4.0, 4.0, 56.0, 20.0))
                    .expect("gradient");
                let mut outline = Path::new();
                outline
                    .commands
                    .push(PathCommand::MoveTo(Point::new(4.0, 30.0)));
                outline
                    .commands
                    .push(PathCommand::LineTo(Point::new(60.0, 40.0)));
                ctx.draw_path(
                    &outline,
                    &Paint::new().color(Color::rgb(0, 80, 255)).stroke_width(3.0),
                )
                .expect("stroke");
            }
            r.end_frame().expect("end");
        }
        fn rasterizer() -> CpuRasterizer {
            let mut r = CpuRasterizer::new(64, 48);
            r.configure(SurfaceInfo::new(64, 48, 1.0))
                .expect("configure");
            r
        }

        let mut r = rasterizer();
        render(&mut r, 200);
        let shown = r.pixels().to_vec();
        assert!(gradient::RAMP_INDICES.with_borrow(|v| !v.is_empty()));
        r.trim_memory();
        assert_eq!(r.pixels(), &shown[..], "the frame buffer is kept");
        assert!(gradient::RAMP_INDICES.with_borrow(|v| v.capacity() == 0));
        assert!(r.scratch_dirty.capacity() == 0 && r.scratch_covered.capacity() == 0);

        render(&mut r, 90);
        let mut fresh = rasterizer();
        render(&mut fresh, 90);
        assert_eq!(r.pixels(), fresh.pixels(), "the next frame is unchanged");
    }
}

#[cfg(test)]
mod occlusion_tests {
    use super::*;
    use crate::cpu::damage::mark_occluded;

    use crate::recording::RecordingContext;
    use crate::renderer::{DrawingContext, Renderer};
    use crate::scene::command::DrawCommand;
    use crate::scene::display_list::{CacheKey, DisplayIndex, DisplayItem, DisplayList};
    use crate::types::{BlendMode, Color, Image, Paint, Point, Rect};

    const W: u32 = TILE_SIZE * 4;
    const H: u32 = TILE_SIZE * 2;
    const TILES_X: u32 = 4;
    const TILES_Y: u32 = 2;

    fn occluder_item(bounds: Rect, blend: BlendMode) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(1),
            bounds,
            true,
            blend,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    fn plain_item(bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(2),
            bounds,
            false,
            BlendMode::Normal,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    fn occluded(items: &[DisplayItem]) -> Vec<bool> {
        let (mut covered, mut occluded, mut under) = (Vec::new(), Vec::new(), Vec::new());
        mark_occluded(
            items,
            (TILES_X, TILES_Y, W, H),
            &mut covered,
            &mut occluded,
            &mut under,
        );
        occluded
    }

    fn image_item(image: &Image, src: Rect, bounds: Rect) -> DisplayItem {
        let mut list = DisplayList::new();
        let mut ctx = RecordingContext::new(&mut list, W, H);
        ctx.draw_image_region(image, src, bounds).expect("draw");
        drop(ctx);
        list.items()[0].clone()
    }

    #[test]
    fn opaque_images_hide_what_is_under_them() {
        let solid = Image::new(4, 4, vec![255; 64]);
        let mut see_through = vec![255; 64];
        see_through[63] = 254;
        let see_through = Image::new(4, 4, see_through);
        let all = Rect::new(0.0, 0.0, 4.0, 4.0);
        let hides = |image: &Image, src: Rect| {
            occluded(&[plain_item(first_tile()), image_item(image, src, whole())])[0]
        };
        assert!(hides(&solid, all));
        assert!(hides(&solid, Rect::new(1.0, 1.0, 2.0, 2.0)));
        assert!(!hides(&see_through, all));
        assert!(
            !hides(&solid, Rect::new(-1.0, 0.0, 4.0, 4.0)),
            "reaches past the image"
        );
    }

    #[test]
    fn the_last_clear_learns_what_later_draws_cover() {
        let left = Rect::new(0.0, 0.0, TILE_SIZE as f32, H as f32);
        let items = vec![
            plain_item(first_tile()),
            occluder_item(Rect::new(0.0, 0.0, f32::MAX, f32::MAX), BlendMode::Normal),
            occluder_item(left, BlendMode::Normal),
        ];
        let (mut covered, mut occluded, mut under) = (Vec::new(), Vec::new(), Vec::new());
        let dims = (TILES_X, TILES_Y, W, H);
        let last = mark_occluded(&items, dims, &mut covered, &mut occluded, &mut under);
        assert_eq!(last, Some(2), "the covering draw is itself a clear here");
        let mut runs = Vec::new();
        uncovered_runs(
            &[true, false, false, true, false, true, true, false],
            dims,
            |r| {
                runs.push((r.x, r.y, r.width));
            },
        );
        let t = TILE_SIZE as f32;
        assert_eq!(runs, [(t, 0.0, 2.0 * t), (0.0, t, t), (3.0 * t, t, t)]);
    }

    #[test]
    fn a_clear_under_an_opaque_image_still_shows_around_it() {
        let mut r = CpuRasterizer::new(W, H);
        let image = Image::new(1, 1, vec![10, 20, 30, 255]);
        let pixels = frame_of(&mut r, |ctx| {
            ctx.clear(Color::rgb(200, 0, 0)).unwrap();
            ctx.draw_image_rect(&image, Rect::new(0.0, 0.0, (W / 2) as f32, H as f32))
                .unwrap();
        });
        assert_eq!(pixels[0], 0xff0a_141e);
        assert_eq!(pixels[(W - 1) as usize], 0xffc8_0000);
        assert_eq!(pixels[(W * H - 1) as usize], 0xffc8_0000);
    }

    fn whole() -> Rect {
        Rect::new(0.0, 0.0, W as f32, H as f32)
    }
    fn first_tile() -> Rect {
        Rect::new(0.0, 0.0, TILE_SIZE as f32, TILE_SIZE as f32)
    }

    #[test]
    fn rect_contains_basic() {
        let outer = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(rect_contains(outer, Rect::new(1.0, 1.0, 5.0, 5.0)));
        assert!(rect_contains(outer, outer));
        assert!(!rect_contains(outer, Rect::new(5.0, 5.0, 10.0, 10.0)));
        assert!(!rect_contains(outer, Rect::new(-1.0, 0.0, 5.0, 5.0)));
    }

    #[test]
    fn a_later_opaque_draw_hides_what_is_under_it() {
        let items = vec![
            plain_item(first_tile()),
            occluder_item(whole(), BlendMode::Normal),
        ];
        assert!(occluded(&items)[0]);
    }

    #[test]
    fn an_earlier_draw_does_not_hide_a_later_one() {
        let items = vec![
            occluder_item(whole(), BlendMode::Normal),
            plain_item(first_tile()),
        ];
        assert!(!occluded(&items)[1]);
    }

    #[test]
    fn a_draw_that_covers_only_part_hides_nothing_outside_it() {
        let last_tile = Rect::new(
            (TILE_SIZE * 3) as f32,
            0.0,
            TILE_SIZE as f32,
            TILE_SIZE as f32,
        );
        let left = Rect::new(0.0, 0.0, TILE_SIZE as f32, H as f32);
        let items = vec![
            plain_item(last_tile),
            occluder_item(left, BlendMode::Normal),
        ];
        assert!(!occluded(&items)[0]);
    }

    #[test]
    fn a_blend_that_is_not_normal_hides_nothing() {
        let items = vec![
            plain_item(first_tile()),
            occluder_item(whole(), BlendMode::Multiply),
        ];
        assert!(!occluded(&items)[0]);
    }

    #[test]
    fn a_draw_that_is_not_opaque_hides_nothing() {
        let items = vec![plain_item(first_tile()), plain_item(whole())];
        assert!(!occluded(&items)[0]);
    }

    #[test]
    fn a_clipped_draw_hides_only_what_its_clip_reaches() {
        let last_tile = Rect::new(
            (TILE_SIZE * 3) as f32,
            0.0,
            TILE_SIZE as f32,
            TILE_SIZE as f32,
        );
        let mut occluder = occluder_item(whole(), BlendMode::Normal);
        occluder.clip = Some(Rect::new(0.0, 0.0, TILE_SIZE as f32, H as f32));
        let items = vec![plain_item(last_tile), occluder];

        assert!(
            !occluded(&items)[0],
            "the draw is clipped to the first tile column and cannot hide the last"
        );
    }

    #[test]
    fn a_clip_that_still_covers_the_item_hides_it() {
        let mut occluder = occluder_item(whole(), BlendMode::Normal);
        occluder.clip = Some(Rect::new(0.0, 0.0, (TILE_SIZE * 2) as f32, H as f32));
        let items = vec![plain_item(first_tile()), occluder];

        assert!(occluded(&items)[0]);
    }

    #[test]
    fn a_circle_covers_less_than_the_square_around_it() {
        let radius = TILE_SIZE as f32;
        let center = Point::new(radius, radius);
        let square = inscribed_square(center, radius);

        assert!(square.width < radius * 2.0, "smaller than the bounds");
        assert!(
            (center.x - square.x).hypot(center.y - square.y) <= radius + 0.001,
            "and its corner is still inside the circle"
        );
    }

    #[test]
    fn draws_combine_to_hide_what_neither_covers_alone() {
        let across = Rect::new(0.0, 0.0, (TILE_SIZE * 2) as f32, H as f32);
        let left = Rect::new(0.0, 0.0, TILE_SIZE as f32, H as f32);
        let right = Rect::new(TILE_SIZE as f32, 0.0, TILE_SIZE as f32, H as f32);
        let items = vec![
            plain_item(across),
            occluder_item(left, BlendMode::Normal),
            occluder_item(right, BlendMode::Normal),
        ];

        assert!(occluded(&items)[0]);
    }

    #[test]
    fn a_draw_ending_inside_a_tile_does_not_claim_it() {
        let half = Rect::new(0.0, 0.0, (TILE_SIZE / 2) as f32, H as f32);
        let items = vec![
            plain_item(first_tile()),
            occluder_item(half, BlendMode::Normal),
        ];

        assert!(!occluded(&items)[0], "half a tile is not a covered tile");
    }

    fn frame_of(r: &mut CpuRasterizer, scene: impl FnOnce(&mut dyn DrawingContext)) -> Vec<u32> {
        let mut ctx = r.begin_frame().unwrap();
        scene(&mut *ctx);
        drop(ctx);
        r.end_frame().unwrap();
        r.frame_buffer.clone()
    }

    #[test]
    fn repeated_frames_of_the_same_scene_are_stable() {
        let mut r = CpuRasterizer::new(240, 160);
        let scene = |caret_x: f32| {
            move |ctx: &mut dyn DrawingContext| {
                ctx.clear(Color::rgb(16, 18, 22)).unwrap();
                let panel = Paint::new().color(Color::rgb(30, 34, 40));
                ctx.draw_rect(Rect::new(0.0, 0.0, 240.0, 40.0), &panel)
                    .unwrap();
                let text = Paint::new().color(Color::rgb(200, 200, 210));
                for row in 0..6u8 {
                    let y = 50.0 + f32::from(row) * 14.0;
                    ctx.draw_rect(Rect::new(8.0, y, 180.0, 8.0), &text).unwrap();
                }
                let caret = Paint::new().color(Color::rgb(255, 220, 120));
                ctx.draw_rect(Rect::new(caret_x, 50.0, 2.0, 84.0), &caret)
                    .unwrap();
            }
        };
        frame_of(&mut r, scene(20.0));
        let settled = frame_of(&mut r, scene(20.0));
        assert_eq!(
            settled,
            frame_of(&mut r, scene(20.0)),
            "an unchanged scene changed"
        );
        frame_of(&mut r, scene(60.0));
        assert_eq!(
            settled,
            frame_of(&mut r, scene(20.0)),
            "the caret left a mark"
        );
    }

    #[test]
    fn content_returns_when_an_overlay_is_dismissed() {
        let mut r = CpuRasterizer::new(240, 160);
        let scene = |overlay: bool| {
            move |ctx: &mut dyn DrawingContext| {
                ctx.clear(Color::rgb(16, 18, 22)).unwrap();
                let text = Paint::new().color(Color::rgb(200, 200, 210));
                for row in 0..8u8 {
                    let y = 20.0 + f32::from(row) * 14.0;
                    ctx.draw_rect(Rect::new(8.0, y, 200.0, 8.0), &text).unwrap();
                }
                if overlay {
                    let panel = Paint::new().color(Color::rgb(40, 44, 52));
                    ctx.draw_rect(Rect::new(0.0, 0.0, 240.0, 160.0), &panel)
                        .unwrap();
                }
            }
        };
        frame_of(&mut r, scene(false));
        let without = frame_of(&mut r, scene(false));
        frame_of(&mut r, scene(true));
        assert_eq!(
            without,
            frame_of(&mut r, scene(false)),
            "covered content did not return"
        );
    }
}

#[cfg(test)]
mod tile_cache_tests {
    use super::*;
    use crate::cpu::raster::pixel;
    use crate::cpu::tiles::{tile_grid_dims, tile_range};
    use crate::renderer::Renderer;
    use crate::scene::command::DrawCommand;
    use crate::scene::display_list::{CacheKey, DisplayIndex, DisplayItem};
    use crate::types::{BlendMode, Color, Image, Paint, PaintStyle, Path, Point, Rect};

    fn tile() -> f32 {
        TILE_SIZE as f32
    }

    fn two_by_two() -> CpuRasterizer {
        CpuRasterizer::new(TILE_SIZE * 2, TILE_SIZE * 2)
    }

    fn whole() -> Rect {
        Rect::new(0.0, 0.0, tile() * 2.0, tile() * 2.0)
    }

    fn inside(tx: f32, ty: f32) -> Rect {
        Rect::new(tx * tile() + 4.0, ty * tile() + 4.0, 8.0, 8.0)
    }

    fn item(key: u64, bounds: Rect) -> DisplayItem {
        DisplayItem::new(
            DisplayIndex(0),
            CacheKey::from_hash(key),
            bounds,
            false,
            BlendMode::Normal,
            DrawCommand::Clear(Color::rgb(0, 0, 0)),
        )
    }

    #[test]
    fn tile_grid_dims_rounds_up() {
        assert_eq!(tile_grid_dims(TILE_SIZE, TILE_SIZE), (1, 1));
        assert_eq!(tile_grid_dims(TILE_SIZE + 1, TILE_SIZE), (2, 1));
        assert_eq!(tile_grid_dims(TILE_SIZE * 2, TILE_SIZE + 44), (2, 2));
    }

    #[test]
    fn tile_range_clamps_huge_bounds_to_grid() {
        let max = Rect::new(0.0, 0.0, f32::MAX, f32::MAX);
        assert_eq!(tile_range(max, 3, 2), (0, 0, 3, 2));
    }

    #[test]
    fn tile_range_picks_single_tile() {
        let bounds = Rect::new(tile() + 10.0, 10.0, 4.0, 4.0);
        assert_eq!(tile_range(bounds, 4, 4), (1, 0, 2, 1));
    }

    #[test]
    fn changing_one_tiles_item_leaves_other_tile_clean() {
        let mut r = two_by_two();
        let a = inside(0.0, 0.0);
        let b = inside(1.0, 1.0);
        let full = whole();

        r.display_list.push(item(1, a));
        r.display_list.push(item(2, b));
        let dirty = r.compute_dirty_tiles(Some(full), None, 2, 2);
        assert!(dirty[0], "tile (0,0) dirty on first computation");
        assert!(dirty[3], "tile (1,1) dirty on first computation");

        r.display_list.clear();
        r.display_list.push(item(10, a));
        r.display_list.push(item(2, b));
        let dirty = r.compute_dirty_tiles(Some(full), None, 2, 2);
        assert!(dirty[0], "tile (0,0) dirty: its item's cache key changed");
        assert!(!dirty[1], "tile (1,0) is empty and unchanged");
        assert!(!dirty[2], "tile (0,1) is empty and unchanged");
        assert!(!dirty[3], "tile (1,1) clean: its item is unchanged");
    }

    #[test]
    fn forced_damage_marks_tile_dirty_regardless_of_hash() {
        let mut r = two_by_two();
        let a = inside(0.0, 0.0);
        let forced = inside(1.0, 1.0);
        let full = whole();

        r.display_list.push(item(1, a));
        let _ = r.compute_dirty_tiles(Some(full), None, 2, 2);

        let dirty = r.compute_dirty_tiles(Some(full), Some(forced), 2, 2);
        assert!(!dirty[0], "tile (0,0) unchanged and not forced");
        assert!(dirty[3], "tile (1,1) forced dirty by set_damage");
    }

    #[test]
    fn a_spanning_item_does_not_drag_in_its_clean_tiles() {
        let mut r = two_by_two();
        let full = whole();
        let spanning = Rect::new(tile() - 16.0, 4.0, 40.0, 8.0);
        let small = inside(0.0, 0.0);

        r.display_list.push(item(100, spanning));
        r.display_list.push(item(1, small));
        let _ = r.compute_dirty_tiles(Some(full), None, 2, 2);

        r.display_list.clear();
        r.display_list.push(item(100, spanning));
        r.display_list.push(item(2, small));
        let dirty = r.compute_dirty_tiles(Some(full), None, 2, 2);

        assert!(dirty[0], "tile (0,0) dirty: small's cache key changed");
        assert!(
            !dirty[1],
            "tile (1,0) stays clean: the spanning item is clipped to the repaint"
        );
        assert!(!dirty[2], "tile (0,1) untouched");
        assert!(!dirty[3], "tile (1,1) untouched");
    }

    #[test]
    fn a_clean_tile_inside_the_repaint_keeps_its_items() {
        let mut r = CpuRasterizer::new(512, 512);
        let background = Paint::new().color(Color::rgb(1, 2, 3));
        let kept = Paint::new().color(Color::rgb(200, 100, 50));
        let frame = |r: &mut CpuRasterizer, corner: Color| {
            let corner = Paint::new().color(corner);
            let mut ctx = r.begin_frame().unwrap();
            ctx.draw_rect(Rect::new(0.0, 0.0, 512.0, 512.0), &background)
                .unwrap();
            ctx.draw_rect(Rect::new(4.0, 4.0, 8.0, 8.0), &corner)
                .unwrap();
            ctx.draw_rect(Rect::new(300.0, 4.0, 8.0, 8.0), &kept)
                .unwrap();
            ctx.draw_rect(Rect::new(300.0, 300.0, 8.0, 8.0), &corner)
                .unwrap();
            drop(ctx);
            r.end_frame().unwrap();
        };
        frame(&mut r, Color::rgb(10, 20, 30));
        frame(&mut r, Color::rgb(0, 255, 0));

        let bw = r.width;
        assert_eq!(
            pixel::pixel_at(&r.frame_buffer, bw, 8, 8),
            pixel::color_to_u32(Color::rgb(0, 255, 0))
        );
        assert_eq!(
            pixel::pixel_at(&r.frame_buffer, bw, 304, 8),
            pixel::color_to_u32(kept.color)
        );
    }

    #[test]
    fn unrelated_tile_pixels_survive_a_localized_redraw() {
        let mut r = CpuRasterizer::new(512, 512);
        let paint_a = Paint::new().color(Color::rgb(10, 20, 30));
        let paint_b = Paint::new().color(Color::rgb(200, 100, 50));
        let paint_c = Paint::new().color(Color::rgb(0, 255, 0));

        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(1, 2, 3)).unwrap();
        ctx.draw_rect(Rect::new(4.0, 4.0, 8.0, 8.0), &paint_a)
            .unwrap();
        ctx.draw_rect(Rect::new(300.0, 300.0, 8.0, 8.0), &paint_b)
            .unwrap();
        drop(ctx);
        r.end_frame().unwrap();

        let bw = r.width;
        assert_eq!(
            pixel::pixel_at(&r.frame_buffer, bw, 304, 304),
            pixel::color_to_u32(paint_b.color)
        );

        let mut ctx = r.begin_frame().unwrap();
        ctx.clear(Color::rgb(1, 2, 3)).unwrap();
        ctx.draw_rect(Rect::new(4.0, 4.0, 8.0, 8.0), &paint_c)
            .unwrap();
        ctx.draw_rect(Rect::new(300.0, 300.0, 8.0, 8.0), &paint_b)
            .unwrap();
        drop(ctx);
        r.end_frame().unwrap();

        assert_eq!(
            pixel::pixel_at(&r.frame_buffer, bw, 8, 8),
            pixel::color_to_u32(paint_c.color)
        );
        assert_eq!(
            pixel::pixel_at(&r.frame_buffer, bw, 304, 304),
            pixel::color_to_u32(paint_b.color)
        );
    }

    #[test]
    fn an_image_blit_adds_nothing_of_its_own() {
        let size = 170u32;
        let last = size - 1;
        let mut data = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let v = u8::try_from(255 - y * 255 / last).expect("fits");
                let s = u8::try_from(x * 255 / last).expect("fits");
                let min = u8::try_from(u32::from(v) * (255 - u32::from(s)) / 255).expect("fits");
                data.extend_from_slice(&[v, min, min, 255]);
            }
        }
        let image = Image::new(size, size, data);
        let dest = Rect::new(33.4, 57.6, 170.0, 170.0);
        let card = Paint::new().color(Color::rgb(40, 44, 52));

        let mut r = CpuRasterizer::new(512, 512);
        for _ in 0..2 {
            let mut ctx = r.begin_frame().unwrap();
            ctx.draw_rect(Rect::new(0.0, 0.0, 512.0, 512.0), &card)
                .unwrap();
            ctx.draw_image_rect(&image, dest).unwrap();
            drop(ctx);
            r.end_frame().unwrap();
        }

        let bw = r.width;
        for y in 131..169u32 {
            for x in 1..40u32 {
                let px = pixel::pixel_at(&r.frame_buffer, bw, 34 + x, 58 + y);
                let brightest = [(px >> 16) & 0xFF, (px >> 8) & 0xFF, px & 0xFF]
                    .into_iter()
                    .max()
                    .unwrap_or(0);
                assert!(brightest < 60, "pixel ({x}, {y}) of the image is {px:08x}");
            }
        }
    }

    /// Draws `shape` at each place, repainting only what changed, and compares every frame with
    /// the same frame drawn fresh: a shape reaching past its bounds leaves pixels behind.
    fn moves_clean(
        name: &str,
        shape: impl Fn(&mut dyn DrawingContext, f32, f32) -> AureaResult<()>,
    ) {
        let frame = |r: &mut CpuRasterizer, x: f32, y: f32| {
            let mut ctx = r.begin_frame().expect("frame");
            ctx.clear(Color::rgb(10, 10, 14)).expect("clear");
            shape(&mut *ctx, x, y).expect("shape");
            drop(ctx);
            r.end_frame().expect("end");
        };
        let mut moving = CpuRasterizer::new(320, 240);
        // Fine enough to bring every edge just short of a tile's side.
        for step in 0..600_u16 {
            let x = 8.0 + f32::from(step) * 0.3125;
            let y = 6.0 + f32::from(step) * 0.28125;
            frame(&mut moving, x, y);
            let mut fresh = CpuRasterizer::new(320, 240);
            frame(&mut fresh, x, y);
            assert!(
                moving.pixels() == fresh.pixels(),
                "{name} leaves pixels at step {step}"
            );
        }
    }

    #[test]
    fn a_moving_shape_leaves_nothing_behind() {
        let fill = Paint::new().color(Color::rgb(240, 200, 80));
        let line = Paint::new()
            .color(Color::rgb(240, 200, 80))
            .style(PaintStyle::Stroke)
            .stroke_width(1.5);
        moves_clean("a filled circle", |c, x, y| {
            c.draw_circle(Point::new(x, y), 15.0, &fill)
        });
        moves_clean("a stroked circle", |c, x, y| {
            c.draw_circle(Point::new(x, y), 15.0, &line)
        });
        moves_clean("a filled round rect", |c, x, y| {
            c.draw_round_rect(Rect::new(x, y, 30.0, 20.0), 6.0, &fill)
        });
        moves_clean("a stroked round rect", |c, x, y| {
            c.draw_round_rect(Rect::new(x, y, 30.0, 20.0), 6.0, &line)
        });
        moves_clean("a filled rect", |c, x, y| {
            c.draw_rect(Rect::new(x, y, 30.0, 20.0), &fill)
        });
        moves_clean("a stroked rect", |c, x, y| {
            c.draw_rect(Rect::new(x, y, 30.0, 20.0), &line)
        });
        moves_clean("a stroked path", |c, x, y| {
            c.draw_path(&Path::rounded_rect(Rect::new(x, y, 30.0, 20.0), 6.0), &line)
        });
        moves_clean("a shadow", |c, x, y| {
            c.draw_shadow(
                Rect::new(x, y, 30.0, 20.0),
                6.0,
                4.0,
                Color::rgba(0, 0, 0, 160),
            )
        });
    }
}

#[cfg(test)]
mod scale_tests {
    use super::*;
    use crate::cpu::raster::pixel;
    use crate::geometry::Transform;
    use crate::numeric::f32_to_u32_clamped;
    use crate::renderer::{DrawingContext, Renderer};
    use crate::surface::{Surface, SurfaceInfo};
    use crate::types::{Color, Image, Paint, PaintStyle, Rect, Sampling};

    fn frame_at(scale: f32, draw: impl FnOnce(&mut dyn DrawingContext)) -> CpuRasterizer {
        let mut r = CpuRasterizer::new(40, 40);
        r.init(
            Surface::Cpu,
            SurfaceInfo {
                width: 40,
                height: 40,
                scale_factor: scale,
                pixels: None,
            },
        )
        .unwrap();
        let mut ctx = r.begin_frame().unwrap();
        draw(&mut *ctx);
        drop(ctx);
        r.end_frame().unwrap();
        r
    }

    #[test]
    fn logical_rects_land_on_their_physical_pixels() {
        let red = Color::rgb(255, 0, 0);
        for scale in [1.25f32, 1.5, 2.0] {
            let r = frame_at(scale, |ctx| {
                ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                ctx.draw_rect(Rect::new(10.0, 10.0, 20.0, 20.0), &Paint::new().color(red))
                    .unwrap();
            });
            let (_, _, w, h) = r.get_buffer();
            let side = f32_to_u32_clamped((40.0 * scale).round());
            assert_eq!((w, h), (side, side), "buffer at {scale}");
            let at = |logical: f32| f32_to_u32_clamped((logical * scale).floor());
            let buf = r.pixels();
            assert_eq!(
                pixel::pixel_at(buf, w, at(10.5), at(10.5)),
                pixel::color_to_u32(red),
                "inside at {scale}"
            );
            assert_eq!(
                pixel::pixel_at(buf, w, at(29.5), at(29.5)),
                pixel::color_to_u32(red),
                "far corner at {scale}"
            );
            assert_ne!(
                pixel::pixel_at(buf, w, at(9.0), at(9.0)),
                pixel::color_to_u32(red),
                "outside at {scale}"
            );
            assert_ne!(
                pixel::pixel_at(buf, w, at(31.0), at(31.0)),
                pixel::color_to_u32(red),
                "past it at {scale}"
            );
        }
    }

    #[test]
    fn a_stroke_scales_its_width_with_the_display() {
        for scale in [1.25f32, 1.5, 2.0] {
            let r = frame_at(scale, |ctx| {
                ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                let paint = Paint::new()
                    .color(Color::rgb(255, 255, 255))
                    .style(PaintStyle::Stroke)
                    .stroke_width(4.0);
                ctx.draw_line(0.0, 20.0, 40.0, 20.0, &paint).unwrap();
            });
            let (_, _, w, h) = r.get_buffer();
            let column: Vec<u32> = (0..h)
                .map(|y| pixel::pixel_at(r.pixels(), w, w / 2, y))
                .collect();
            let lit = column.iter().filter(|&&p| p & 0xFF > 0x80).count();
            let want = 4.0 * scale;
            #[allow(clippy::cast_precision_loss)]
            let got = lit as f32;
            assert!(
                (got - want).abs() <= 1.0,
                "a 4px line is {want} device pixels at {scale}, got {got}"
            );
        }
    }

    #[test]
    fn snapped_borders_have_no_soft_edges() {
        let white = Color::rgb(255, 255, 255);
        for scale in [1.25f32, 1.5, 1.75] {
            for snap in [false, true] {
                let r = frame_at(scale, |ctx| {
                    ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                    ctx.set_pixel_snap(snap).unwrap();
                    let line = Paint::new()
                        .color(white)
                        .style(PaintStyle::Stroke)
                        .stroke_width(1.0);
                    ctx.draw_line(0.0, 10.3, 40.0, 10.3, &line).unwrap();
                    ctx.draw_rect(Rect::new(5.2, 15.1, 20.0, 12.0), &line)
                        .unwrap();
                    let hair = Paint::new().color(white);
                    ctx.draw_rect(Rect::new(0.0, 33.3, 40.0, 0.4), &hair)
                        .unwrap();
                });
                let (_, _, w, h) = r.get_buffer();
                let column: Vec<u32> = (0..h)
                    .map(|y| {
                        pixel::pixel_at(r.pixels(), w, f32_to_u32_clamped(2.5 * scale), y) & 0xFF
                    })
                    .collect();
                let soft = column.iter().filter(|&&v| v > 8 && v < 247).count();
                let lit = column.iter().filter(|&&v| v >= 247).count();
                if snap {
                    let rule = f32_to_u32_clamped(scale.round()) as usize;
                    assert_eq!(soft, 0, "soft rows at {scale}: {column:?}");
                    assert_eq!(
                        lit,
                        rule + 1,
                        "the rule and the hairline at {scale}: {column:?}"
                    );
                }
                if snap {
                    let border = f32_to_u32_clamped(scale.round()) as usize;
                    let row = f32_to_u32_clamped(20.0 * scale);
                    let line: Vec<u32> = (0..w)
                        .map(|x| pixel::pixel_at(r.pixels(), w, x, row) & 0xFF)
                        .collect();
                    let soft = line.iter().filter(|&&v| v > 8 && v < 247).count();
                    assert_eq!(soft, 0, "{line:?}");
                    let lit = line.iter().filter(|&&v| v >= 247).count();
                    assert_eq!(lit, 2 * border, "{line:?}");
                }
            }
        }
    }

    #[test]
    fn image_dest_rects_land_on_their_physical_pixels() {
        let green = Color::rgb(0, 255, 0);
        let image = Image::new(4, 4, [0, 255, 0, 255].repeat(16));
        for scale in [1.25f32, 1.5, 2.0] {
            for zoomed in [false, true] {
                let r = frame_at(scale, |ctx| {
                    ctx.clear(Color::rgb(0, 0, 0)).unwrap();
                    ctx.set_sampling(Sampling::Nearest).unwrap();
                    if zoomed {
                        ctx.save().unwrap();
                        ctx.transform(
                            Transform::scale(5.0, 5.0).multiply(Transform::translate(10.0, 10.0)),
                        )
                        .unwrap();
                        ctx.draw_image_rect(&image, Rect::new(0.0, 0.0, 4.0, 4.0))
                            .unwrap();
                        ctx.restore().unwrap();
                    } else {
                        ctx.draw_image_rect(&image, Rect::new(10.0, 10.0, 20.0, 20.0))
                            .unwrap();
                    }
                });
                let (_, _, w, _) = r.get_buffer();
                let at = |logical: f32| f32_to_u32_clamped((logical * scale).floor());
                let px = |x: f32, y: f32| pixel::pixel_at(r.pixels(), w, at(x), at(y));
                let case = format!("at {scale}, zoomed {zoomed}");
                assert_eq!(px(10.5, 10.5), pixel::color_to_u32(green), "inside {case}");
                assert_eq!(
                    px(29.5, 29.5),
                    pixel::color_to_u32(green),
                    "far corner {case}"
                );
                assert_ne!(px(9.0, 9.0), pixel::color_to_u32(green), "outside {case}");
                assert_ne!(px(31.0, 31.0), pixel::color_to_u32(green), "past it {case}");
            }
        }
    }
}

#[cfg(test)]
mod frame_counter_tests {
    use super::*;
    use crate::cpu::CommandKind;
    use crate::renderer::{FrameOutcome, Renderer};
    use crate::types::{Paint, Rect};
    use std::time::Duration;

    #[test]
    fn counted_frame_reports_hidden_rect_and_rect_time() {
        let mut r = CpuRasterizer::new(256, 256);
        r.set_counting(true);
        let paint = Paint::new();
        let mut ctx = r.begin_frame().unwrap();
        ctx.draw_rect(Rect::new(0.0, 0.0, 100.0, 100.0), &paint)
            .unwrap();
        ctx.draw_rect(Rect::new(0.0, 0.0, 200.0, 200.0), &paint)
            .unwrap();
        ctx.draw_rect(Rect::new(128.0, 128.0, 100.0, 100.0), &paint)
            .unwrap();
        drop(ctx);

        assert_eq!(r.end_frame().unwrap(), FrameOutcome::Presented);
        let c = *r.frame_counters().expect("counters");
        assert_eq!((c.items, c.drawn, c.hidden, c.skipped_clean), (3, 2, 1, 0));
        let rects = c.kinds[CommandKind::Rect as usize];
        assert_eq!(rects.count, 2);
        assert!(rects.time > Duration::ZERO);
        assert!(c.total > Duration::ZERO);
    }

    #[test]
    fn counters_are_none_while_counting_is_off() {
        let mut r = CpuRasterizer::new(32, 32);
        let paint = Paint::new();
        let mut ctx = r.begin_frame().unwrap();
        ctx.draw_rect(Rect::new(0.0, 0.0, 8.0, 8.0), &paint)
            .unwrap();
        drop(ctx);
        r.end_frame().unwrap();
        assert!(r.frame_counters().is_none());

        r.set_counting(true);
        r.set_counting(false);
        assert!(r.frame_counters().is_none());
    }
}

#[cfg(test)]
mod band_tests {
    use super::*;
    use crate::cpu::clip::ClipBox;

    use crate::geometry::{FillRule, PathCommand};
    use crate::renderer::{DrawingContext, Renderer};
    use crate::surface::{Surface, SurfaceInfo};
    use crate::types::{
        BlendMode, Color, GradientStop, Image, LinearGradient, Paint, PaintStyle, Path, Point,
        RadialGradient, Rect, Sampling,
    };
    use aurea_foundation::AureaResult;

    fn path(points: &[(f32, f32)], rule: FillRule) -> Path {
        let mut path = Path::new();
        path.fill_rule = rule;
        for (i, &(x, y)) in points.iter().enumerate() {
            let p = Point::new(x, y);
            path.commands.push(if i == 0 {
                PathCommand::MoveTo(p)
            } else {
                PathCommand::LineTo(p)
            });
        }
        path.commands.push(PathCommand::Close);
        path
    }

    /// One of every kind of draw, over row edges a band may split.
    fn scene(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        let stops = vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgba(255, 0, 0, 255),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgba(0, 0, 255, 120),
            },
        ];
        let mut pixels = Vec::new();
        for y in 0..16u8 {
            for x in 0..16u8 {
                let a = if (x + y) % 3 == 0 { 120 } else { 255 };
                pixels.extend_from_slice(&[x * 16, y * 16, 90, a]);
            }
        }
        let image = Image::new(16, 16, pixels);
        ctx.clear(Color::rgb(20, 24, 30))?;
        ctx.draw_rect(
            Rect::new(3.3, 2.7, 150.4, 40.6),
            &Paint::new().color(Color::rgba(200, 120, 40, 180)),
        )?;
        ctx.draw_round_rect(
            Rect::new(20.5, 30.25, 90.0, 70.0),
            14.0,
            &Paint::new().color(Color::rgb(40, 160, 90)),
        )?;
        ctx.draw_shadow(
            Rect::new(110.0, 20.0, 60.0, 50.0),
            8.0,
            12.0,
            Color::rgba(0, 0, 0, 160),
        )?;
        ctx.draw_circle(
            Point::new(60.0, 120.0),
            33.3,
            &Paint::new().color(Color::rgba(250, 250, 0, 200)),
        )?;
        let ring = Paint::new()
            .color(Color::rgb(255, 255, 255))
            .style(PaintStyle::Stroke)
            .stroke_width(3.5);
        ctx.draw_circle(Point::new(140.0, 130.0), 28.0, &ring)?;
        let star = path(
            &[
                (100.0, 60.0),
                (130.0, 150.0),
                (60.0, 95.0),
                (140.0, 95.0),
                (70.0, 150.0),
            ],
            FillRule::EvenOdd,
        );
        ctx.draw_path(&star, &Paint::new().color(Color::rgba(90, 200, 255, 210)))?;
        let dashed = Paint::new()
            .color(Color::rgb(255, 80, 80))
            .style(PaintStyle::Stroke)
            .stroke_width(2.5)
            .dash(&[6.0, 4.0], 1.5);
        ctx.draw_path(&star, &dashed)?;
        ctx.fill_linear_gradient(
            &LinearGradient {
                start: Point::new(0.0, 150.0),
                end: Point::new(180.0, 190.0),
                stops: stops.clone(),
            },
            Rect::new(0.0, 150.0, 180.0, 40.0),
        )?;
        ctx.fill_radial_gradient(
            &RadialGradient {
                center: Point::new(160.0, 60.0),
                radius: 40.0,
                stops,
            },
            Rect::new(120.0, 20.0, 80.0, 80.0),
        )?;
        ctx.draw_image_rect(&image, Rect::new(5.0, 60.0, 50.0, 70.0))?;
        ctx.set_sampling(Sampling::Smooth)?;
        ctx.draw_image_rect(&image, Rect::new(150.0, 150.0, 9.0, 11.0))?;
        ctx.set_sampling(Sampling::Nearest)?;
        ctx.draw_image_region(
            &image,
            Rect::new(2.0, 2.0, 8.0, 8.0),
            Rect::new(170.0, 100.0, 27.0, 33.0),
        )?;
        ctx.draw_text(
            "Bands 한글 ok",
            Point::new(12.0, 60.0),
            &Paint::new().color(Color::rgb(240, 240, 240)),
        )?;
        ctx.save()?;
        ctx.clip_path(&path(
            &[(10.3, 10.7), (190.6, 30.1), (150.2, 190.9), (20.4, 170.35)],
            FillRule::NonZero,
        ))?;
        ctx.set_alpha(0.6)?;
        ctx.set_blend_mode(BlendMode::Multiply)?;
        ctx.draw_rect(
            Rect::new(0.0, 80.0, 200.0, 37.0),
            &Paint::new().color(Color::rgb(30, 220, 200)),
        )?;
        ctx.restore()?;
        ctx.save()?;
        ctx.clip_rect(Rect::new(40.5, 40.5, 50.0, 113.0))?;
        ctx.rotate(0.3)?;
        ctx.draw_rect(
            Rect::new(40.0, 20.0, 90.0, 20.0),
            &Paint::new().color(Color::rgb(255, 0, 255)),
        )?;
        ctx.draw_image_rect(&image, Rect::new(60.0, 40.0, 40.0, 40.0))?;
        ctx.restore()
    }

    fn drawn(threads: usize, band_min: usize, (scale, side): (f32, u32)) -> Vec<u32> {
        let (w, h) = (side, side);
        let mut r = CpuRasterizer::new(w, h);
        r.init(
            Surface::Cpu,
            SurfaceInfo {
                width: w,
                height: h,
                scale_factor: scale,
                pixels: None,
            },
        )
        .expect("init");
        r.set_threads(Some(threads));
        r.band_min = band_min;
        let mut ctx = r.begin_frame().expect("frame");
        scene(&mut *ctx).expect("scene");
        drop(ctx);
        r.end_frame().expect("end");
        r.frame_buffer.clone()
    }

    #[test]
    fn bands_draw_the_same_pixels_as_one_thread() {
        for scale in [(1.0, 200), (1.5, 300), (2.0, 400)] {
            let one = drawn(1, 1, scale);
            for threads in [2, 3, 7] {
                let banded = drawn(threads, 1, scale);
                let differ = one.iter().zip(&banded).filter(|(a, b)| a != b).count();
                assert_eq!(differ, 0, "{threads} bands at {}x", scale.0);
            }
        }
    }

    #[test]
    fn small_damage_stays_on_one_thread() {
        let mut r = CpuRasterizer::new(100, 100);
        r.set_threads(Some(8));
        let small = ClipBox::surface(100, 100);
        assert_eq!(r.band_count(small), 1, "10k pixels");
        r.band_min = 1;
        assert_eq!(r.band_count(small), 8);
        r.set_counting(true);
        assert_eq!(r.band_count(small), 1, "counters time items on one thread");
    }
}

#[cfg(test)]
mod offscreen_tests {
    use super::*;

    use crate::renderer::DrawingContext;
    use crate::types::{Color, Paint, Rect};
    use aurea_foundation::AureaResult;

    fn draw_rects(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        ctx.draw_rect(
            Rect::new(0.0, 0.0, 2.0, 1.0),
            &Paint::new().color(Color::rgb(255, 0, 0)),
        )?;
        ctx.draw_rect(
            Rect::new(2.0, 1.0, 2.0, 2.0),
            &Paint::new().color(Color::rgba(0, 0, 255, 128)),
        )
    }

    fn pixel(p: &RgbaPixels, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.width + x) * 4) as usize;
        [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2], p.rgba[i + 3]]
    }

    #[test]
    fn offscreen_at_scale_one_is_logical_size() {
        let p = CpuRasterizer::draw_offscreen(4, 3, 1.0, draw_rects).expect("draw");
        assert_eq!((p.width, p.height), (4, 3));
        assert_eq!(p.rgba.len(), 4 * 3 * 4);
        assert_eq!(pixel(&p, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 2, 1), [0, 0, 255, 128]);
        assert_eq!(pixel(&p, 3, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&p, 0, 2), [0, 0, 0, 0]);
    }

    #[test]
    fn offscreen_at_scale_two_doubles_physical_size() {
        let p = CpuRasterizer::draw_offscreen(4, 3, 2.0, draw_rects).expect("draw");
        assert_eq!((p.width, p.height), (8, 6));
        assert_eq!(p.rgba.len(), 8 * 6 * 4);
        assert_eq!(pixel(&p, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 3, 1), [255, 0, 0, 255]);
        assert_eq!(pixel(&p, 4, 2), [0, 0, 255, 128]);
        assert_eq!(pixel(&p, 7, 5), [0, 0, 255, 128]);
        assert_eq!(pixel(&p, 7, 0), [0, 0, 0, 0]);
    }
}

#[cfg(test)]
mod clip_band_tests {
    use super::*;

    use crate::geometry::{FillRule, PathCommand};
    use crate::renderer::{DrawingContext, Renderer};
    use crate::surface::{Surface, SurfaceInfo};
    use crate::types::{BlendMode, Color, Paint, PaintStyle, Path, Point, Rect};
    use aurea_foundation::AureaResult;

    type Scene = fn(&mut dyn DrawingContext) -> AureaResult<()>;

    fn poly(points: &[(f32, f32)], closed: bool) -> Path {
        let mut path = Path::new();
        path.fill_rule = FillRule::NonZero;
        for (i, &(x, y)) in points.iter().enumerate() {
            let p = Point::new(x, y);
            path.commands.push(if i == 0 {
                PathCommand::MoveTo(p)
            } else {
                PathCommand::LineTo(p)
            });
        }
        if closed {
            path.commands.push(PathCommand::Close);
        }
        path
    }

    fn fill(color: Color) -> Paint {
        Paint::new().color(color)
    }

    fn stroke(color: Color, width: f32) -> Paint {
        Paint::new()
            .color(color)
            .style(PaintStyle::Stroke)
            .stroke_width(width)
    }

    const ROUNDED: [(f32, f32); 8] = [
        (16.0, 8.0),
        (80.0, 8.0),
        (88.0, 16.0),
        (88.0, 80.0),
        (80.0, 88.0),
        (16.0, 88.0),
        (8.0, 80.0),
        (8.0, 16.0),
    ];

    fn rounded_rect_partly_outside(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        ctx.clear(Color::rgb(20, 30, 40))?;
        ctx.save()?;
        ctx.clip_path(&poly(&ROUNDED, true))?;
        ctx.set_alpha(0.7)?;
        ctx.draw_rect(
            Rect::new(-10.0, 20.0, 80.0, 30.0),
            &fill(Color::rgba(200, 40, 40, 200)),
        )?;
        ctx.restore()
    }

    fn stroke_items(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        ctx.clear(Color::rgb(20, 30, 40))?;
        ctx.save()?;
        ctx.clip_path(&poly(&[(10.0, 90.0), (48.0, 6.0), (86.0, 90.0)], true))?;
        ctx.draw_round_rect(
            Rect::new(20.0, 30.0, 50.0, 40.0),
            8.0,
            &stroke(Color::rgb(10, 200, 120), 6.0),
        )?;
        ctx.draw_path(
            &poly(
                &[(0.0, 50.0), (30.0, 20.0), (60.0, 70.0), (96.0, 30.0)],
                false,
            ),
            &stroke(Color::rgba(255, 255, 255, 180), 4.5),
        )?;
        ctx.restore()
    }

    fn two_items_share_clip(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        ctx.clear(Color::rgb(20, 30, 40))?;
        ctx.save()?;
        ctx.clip_path(&poly(
            &[(48.0, 4.0), (92.0, 48.0), (48.0, 92.0), (4.0, 48.0)],
            true,
        ))?;
        ctx.draw_rect(
            Rect::new(0.0, 0.0, 96.0, 50.0),
            &fill(Color::rgba(255, 200, 0, 180)),
        )?;
        ctx.draw_circle(Point::new(60.0, 60.0), 30.0, &fill(Color::rgb(0, 120, 255)))?;
        ctx.draw_shadow(
            Rect::new(30.0, 30.0, 30.0, 30.0),
            4.0,
            6.0,
            Color::rgba(0, 0, 0, 160),
        )?;
        ctx.restore()
    }

    fn nested_clips(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        ctx.clear(Color::rgb(20, 30, 40))?;
        ctx.save()?;
        ctx.clip_path(&poly(&[(0.0, 0.0), (96.0, 0.0), (48.0, 96.0)], true))?;
        ctx.save()?;
        ctx.clip_path(&poly(
            &[(20.0, 20.0), (80.0, 20.0), (80.0, 70.0), (20.0, 70.0)],
            true,
        ))?;
        ctx.set_blend_mode(BlendMode::Multiply)?;
        ctx.draw_rect(
            Rect::new(0.0, 0.0, 96.0, 96.0),
            &fill(Color::rgb(180, 220, 255)),
        )?;
        ctx.restore()?;
        ctx.draw_circle(
            Point::new(48.0, 48.0),
            40.0,
            &fill(Color::rgba(255, 0, 0, 128)),
        )?;
        ctx.restore()
    }

    fn item_outside_clip(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        ctx.clear(Color::rgb(20, 30, 40))?;
        ctx.save()?;
        ctx.clip_path(&poly(&[(0.0, 0.0), (6.0, 0.0), (0.0, 6.0)], true))?;
        ctx.draw_rect(
            Rect::new(50.0, 50.0, 20.0, 20.0),
            &fill(Color::rgb(255, 0, 0)),
        )?;
        ctx.draw_round_rect(
            Rect::new(40.0, 60.0, 30.0, 30.0),
            10.0,
            &stroke(Color::rgb(0, 255, 0), 5.0),
        )?;
        ctx.restore()
    }

    fn clip_rect_and_path(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
        ctx.clear(Color::rgb(20, 30, 40))?;
        ctx.save()?;
        ctx.clip_rect(Rect::new(5.5, 10.25, 60.0, 70.0))?;
        ctx.clip_path(&poly(&ROUNDED, true))?;
        ctx.set_blend_mode(BlendMode::Multiply)?;
        ctx.draw_path(
            &poly(
                &[(4.0, 80.0), (30.0, 4.0), (56.0, 80.0), (92.0, 4.0)],
                false,
            ),
            &stroke(Color::rgb(230, 180, 60), 9.0),
        )?;
        ctx.draw_rect(
            Rect::new(-5.0, -5.0, 40.0, 120.0),
            &fill(Color::rgba(90, 200, 255, 150)),
        )?;
        ctx.restore()
    }

    const SCENES: [Scene; 6] = [
        rounded_rect_partly_outside,
        stroke_items,
        two_items_share_clip,
        nested_clips,
        item_outside_clip,
        clip_rect_and_path,
    ];

    /// FNV-1a over the little-endian bytes of every pixel.
    fn frame(threads: usize, band_min: usize, scene: Scene) -> Vec<u32> {
        let (w, h) = (96, 96);
        let mut r = CpuRasterizer::new(w, h);
        r.init(
            Surface::Cpu,
            SurfaceInfo {
                width: w,
                height: h,
                scale_factor: 1.0,
                pixels: None,
            },
        )
        .expect("init");
        r.set_threads(Some(threads));
        r.band_min = band_min;
        let mut ctx = r.begin_frame().expect("frame");
        scene(&mut *ctx).expect("scene");
        drop(ctx);
        r.end_frame().expect("end");
        r.frame_buffer.clone()
    }

    #[test]
    fn clipped_scenes_match_on_any_number_of_bands() {
        for (i, scene) in SCENES.into_iter().enumerate() {
            let one = frame(1, 1, scene);
            for threads in [2, 3] {
                assert!(
                    frame(threads, 1, scene) == one,
                    "scene {i}, {threads} bands"
                );
            }
        }
    }
}
