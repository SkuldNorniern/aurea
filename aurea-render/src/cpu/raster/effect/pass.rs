//! Backdrop effects on the CPU: the box under a shape copied out, changed, and written back.

use std::thread::available_parallelism;

use crate::cpu::clip::ClipBox;
use crate::cpu::pool;
use crate::cpu::raster::band::Band;
use crate::effect::{BackdropPixels, Effect};
use crate::scene::command::DrawCommand;
use crate::scene::display_list::DisplayItem;
use crate::types::{Color, Rect};

/// Buffers a backdrop reuses from one frame to the next.
#[derive(Default)]
pub(crate) struct EffectScratch {
    pixels: Vec<u32>,
    line: Vec<u32>,
    small: Vec<u32>,
}

/// Fewer pixels than this a step runs on one thread: spreading would cost more.
const SPREAD_PIXELS: usize = 1 << 16;
/// Most runs of rows a step is split into.
const MAX_RUNS: usize = 8;

/// `f` on runs of whole rows of `pixels`, `w` wide, across the pool, each with its first row's
/// number. Rows apart touch pixels apart, so the result is one thread's.
fn by_rows(pixels: &mut [u32], w: usize, f: impl Fn(&mut [u32], usize) + Sync) {
    let rows = pixels.len() / w.max(1);
    let runs = if pixels.len() < SPREAD_PIXELS {
        1
    } else {
        available_parallelism().map_or(1, usize::from).min(MAX_RUNS)
    };
    if runs <= 1 || rows < 2 {
        f(pixels, 0);
        return;
    }
    let per = rows.div_ceil(runs);
    let f = &f;
    let mut tasks: Vec<_> = pixels
        .chunks_mut(per * w)
        .enumerate()
        .map(|(k, chunk)| move || f(chunk, k * per))
        .collect();
    // a worker that panicked leaves its rows as they were; the frame goes on
    let _ = pool::run(&mut tasks);
}

/// Box passes per direction; three come close to a Gaussian.
const PASSES: usize = 3;

/// Runs `effects` on the pixels of `bounds` inside `clip`, reading as far around as they reach.
/// `buf` must hold every row the effects read, as a whole frame does.
pub(crate) fn backdrop(
    effects: &[Effect],
    bounds: Rect,
    buf: &mut Band<'_>,
    (bw, bh): (u32, u32),
    clip: ClipBox,
    s: &mut EffectScratch,
) {
    let write = clip.intersect(bounds);
    if write.is_empty() || effects.is_empty() {
        return;
    }
    let reach = effects.iter().map(Effect::reach).sum::<f32>().ceil();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let reach = reach.max(0.0) as u32;
    let (x0, y0) = (
        write.x0.saturating_sub(reach),
        write.y0.saturating_sub(reach),
    );
    let (x1, y1) = ((write.x1 + reach).min(bw), (write.y1 + reach).min(bh));
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    s.pixels.clear();
    for y in y0..y1 {
        let at = (y * bw + x0) as usize;
        match buf.get_mut(at..at + w) {
            Some(row) => s.pixels.extend_from_slice(row),
            // rows outside the band read as clear
            None => s.pixels.resize(s.pixels.len() + w, 0),
        }
    }
    for effect in effects {
        run(effect, &mut s.pixels, w, h, (&mut s.line, &mut s.small));
    }
    let (wx, wy) = ((write.x0 - x0) as usize, (write.y0 - y0) as usize);
    let ww = (write.x1 - write.x0) as usize;
    for row in 0..(write.y1 - write.y0) as usize {
        let at = ((write.y0 as usize + row) * bw as usize) + write.x0 as usize;
        if let Some(out) = buf.get_mut(at..at + ww) {
            let from = (wy + row) * w + wx;
            out.copy_from_slice(&s.pixels[from..from + ww]);
        }
    }
}

fn run(
    effect: &Effect,
    pixels: &mut [u32],
    w: usize,
    h: usize,
    (line, small): (&mut Vec<u32>, &mut Vec<u32>),
) {
    match effect {
        Effect::Blur(radius) => blur(pixels, w, h, *radius, (line, small)),
        Effect::Saturate(amount) => by_rows(pixels, w, |rows, _| saturate(rows, *amount)),
        Effect::Brightness(amount) => by_rows(pixels, w, |rows, _| brightness(rows, *amount)),
        Effect::Tint(color) => by_rows(pixels, w, |rows, _| tint(rows, *color)),
        Effect::Custom(filter) => filter.filter(&mut BackdropPixels::new(pixels, w, h)),
    }
}

/// `amount` in 256ths, for integer steps.
fn fixed(amount: f32) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let v = (amount * 256.0).round().clamp(-65_536.0, 65_536.0) as i32;
    v
}

fn channels(p: u32) -> [i32; 3] {
    [16, 8, 0].map(|shift| i32::try_from((p >> shift) & 0xFF).unwrap_or(0))
}

fn pack(p: u32, [r, g, b]: [i32; 3]) -> u32 {
    let c = |v: i32| u32::try_from(v.clamp(0, 255)).unwrap_or(0);
    (p & 0xFF00_0000) | (c(r) << 16) | (c(g) << 8) | c(b)
}

fn saturate(pixels: &mut [u32], amount: f32) {
    let s = fixed(amount);
    for p in pixels {
        let [r, g, b] = channels(*p);
        // Rec. 709 weights in 256ths
        let grey = (54 * r + 183 * g + 19 * b) >> 8;
        *p = pack(*p, [r, g, b].map(|c| grey + (((c - grey) * s) >> 8)));
    }
}

fn brightness(pixels: &mut [u32], amount: f32) {
    let s = fixed(amount);
    for p in pixels {
        *p = pack(*p, channels(*p).map(|c| (c * s) >> 8));
    }
}

fn tint(pixels: &mut [u32], color: Color) {
    let a = u32::from(color.a);
    if a == 0 {
        return;
    }
    let tc = [color.r, color.g, color.b].map(u32::from);
    for p in pixels {
        let mut out = *p & 0xFF00_0000;
        for (shift, t) in [16, 8, 0].into_iter().zip(tc) {
            let c = (*p >> shift) & 0xFF;
            out |= ((c * (255 - a) + t * a + 127) / 255) << shift;
        }
        *p = out;
    }
}

/// A blur spreading about `radius` pixels: box passes along rows, then along columns. Wide blurs
/// run on a copy shrunk by [`shrink_step`] and grow back smoothly: they keep no detail that small.
fn blur(
    pixels: &mut [u32],
    w: usize,
    h: usize,
    radius: f32,
    (line, small): (&mut Vec<u32>, &mut Vec<u32>),
) {
    if w == 0 || h == 0 {
        return;
    }
    let step = shrink_step(radius);
    if step == 1 {
        box_passes(pixels, w, h, radius, line);
        return;
    }
    let (sw, sh) = (w.div_ceil(step), h.div_ceil(step));
    shrink(pixels, w, h, step, small);
    #[allow(clippy::cast_precision_loss)]
    box_passes(small, sw, sh, radius / step as f32, line);
    grow(small, (sw, sh), step, pixels, (w, h));
}

/// How much a blur of `radius` shrinks first: none for small ones.
fn shrink_step(radius: f32) -> usize {
    match radius {
        r if r >= 48.0 => 8,
        r if r >= 24.0 => 4,
        r if r >= 12.0 => 2,
        _ => 1,
    }
}

fn box_passes(pixels: &mut [u32], w: usize, h: usize, radius: f32, line: &mut Vec<u32>) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let half = (radius / PASSES as f32).round().max(0.0) as usize;
    if half == 0 {
        return;
    }
    for _ in 0..PASSES {
        for y in 0..h {
            box_line(pixels, y * w, 1, w, half, line);
        }
        for x in 0..w {
            box_line(pixels, x, w, h, half, line);
        }
    }
}

/// `pixels` averaged in `step` by `step` blocks into `small`.
fn shrink(pixels: &[u32], w: usize, h: usize, step: usize, small: &mut Vec<u32>) {
    let (sw, sh) = (w.div_ceil(step), h.div_ceil(step));
    small.clear();
    small.resize(sw * sh, 0);
    by_rows(small, sw, |rows, first| {
        for (i, row) in rows.chunks_mut(sw).enumerate() {
            let by = first + i;
            for (bx, out) in row.iter_mut().enumerate() {
                *out = average(pixels, w, (bx * step, by * step), (step, w, h));
            }
        }
    });
}

/// The mean of the `step` by `step` block of `pixels` from `(x, y)`, cut at the edges.
fn average(
    pixels: &[u32],
    w: usize,
    (x, y): (usize, usize),
    (step, width, height): (usize, usize, usize),
) -> u32 {
    let mut sum = [0u32; 4];
    let mut n = 0;
    for row in y..(y + step).min(height) {
        for &p in &pixels[row * w + x..row * w + (x + step).min(width)] {
            for (k, shift) in [24, 16, 8, 0].into_iter().enumerate() {
                sum[k] += (p >> shift) & 0xFF;
            }
            n += 1;
        }
    }
    let mut out = 0;
    for (k, shift) in [24, 16, 8, 0].into_iter().enumerate() {
        out |= ((sum[k] + n / 2) / n) << shift;
    }
    out
}

/// Red and blue, or alpha and green, of a pixel: two channels in one word, 16 bits apart.
const LANES: u32 = 0x00FF_00FF;

/// `a` toward `b` by `t` of 256, two channels at a time.
fn lerp(a: u32, b: u32, t: u32) -> u32 {
    let mix = |a: u32, b: u32| ((a * (256 - t) + b * t) >> 8) & LANES;
    mix(a & LANES, b & LANES) | (mix((a >> 8) & LANES, (b >> 8) & LANES) << 8)
}

/// Where a grown pixel falls in the small copy: the cell before it and how far on, of 256.
fn place(i: usize, step: usize, len: usize) -> (usize, u32) {
    // pixel centres: (i + 0.5) / step - 0.5, in 256ths
    let at = ((2 * i + 1) * 256 / (2 * step)).saturating_sub(128);
    (
        (at / 256).min(len - 1),
        u32::try_from(at % 256).unwrap_or(0),
    )
}

/// `small` back up to `w` by `h`, each pixel between its four nearest, into `pixels`.
fn grow(
    small: &[u32],
    (sw, sh): (usize, usize),
    step: usize,
    pixels: &mut [u32],
    (w, h): (usize, usize),
) {
    let cell = |x: usize, y: usize| small[y.min(sh - 1) * sw + x.min(sw - 1)];
    debug_assert_eq!(pixels.len(), w * h);
    by_rows(pixels, w, |rows, first| {
        for (i, row) in rows.chunks_mut(w).enumerate() {
            grow_row(row, first + i, step, (sw, sh), &cell);
        }
    });
}

/// Row `y` of the grown image, from the small copy's `cell`s.
fn grow_row(
    row: &mut [u32],
    y: usize,
    step: usize,
    (sw, sh): (usize, usize),
    cell: &impl Fn(usize, usize) -> u32,
) {
    {
        let (y0, ty) = place(y, step, sh);
        for (x, out) in row.iter_mut().enumerate() {
            let (x0, tx) = place(x, step, sw);
            let top = lerp(cell(x0, y0), cell(x0 + 1, y0), tx);
            let bottom = lerp(cell(x0, y0 + 1), cell(x0 + 1, y0 + 1), tx);
            *out = lerp(top, bottom, ty);
        }
    }
}

/// One box pass over `len` pixels from `start`, `step` apart; ends repeat their edge pixel.
fn box_line(
    pixels: &mut [u32],
    start: usize,
    step: usize,
    len: usize,
    half: usize,
    line: &mut Vec<u32>,
) {
    line.clear();
    line.extend((0..len).map(|i| pixels[start + i * step]));
    #[allow(clippy::cast_possible_wrap)]
    let last = len as isize - 1;
    let at = |i: isize| line[i.clamp(0, last).cast_unsigned()];
    let width = u32::try_from(2 * half + 1).unwrap_or(u32::MAX);
    let mut sum = [0u32; 4];
    let add = |sum: &mut [u32; 4], p: u32, sign: bool| {
        for (k, shift) in [24, 16, 8, 0].into_iter().enumerate() {
            let c = (p >> shift) & 0xFF;
            sum[k] = if sign { sum[k] + c } else { sum[k] - c };
        }
    };
    let half = half as isize;
    for i in -half..=half {
        add(&mut sum, at(i), true);
    }
    for i in 0..len {
        let mut out = 0;
        for (k, shift) in [24, 16, 8, 0].into_iter().enumerate() {
            out |= ((sum[k] + width / 2) / width) << shift;
        }
        pixels[start + i * step] = out;
        let i = i as isize;
        add(&mut sum, at(i - half), false);
        add(&mut sum, at(i + half + 1), true);
    }
}

/// `damage` grown over every backdrop whose reach it touches: the box it writes must be drawn
/// whole, since each pixel of it reads the pixels around.
pub(crate) fn grow_over(items: &[DisplayItem], damage: Rect) -> Rect {
    let mut grown = damage;
    for item in items {
        let DrawCommand::Backdrop(bounds, effects) = &item.command else {
            continue;
        };
        let reach: f32 = effects.iter().map(Effect::reach).sum();
        let read = Rect::new(
            bounds.x - reach,
            bounds.y - reach,
            bounds.width + 2.0 * reach,
            bounds.height + 2.0 * reach,
        );
        if read.intersects(grown) {
            grown = grown.union(*bounds);
        }
    }
    grown
}

/// The first backdrop at or after `from`, where the frame is drawn up to it, then it on the whole
/// frame.
pub(crate) fn next_backdrop(items: &[DisplayItem], from: usize) -> Option<usize> {
    items[from..]
        .iter()
        .position(|item| matches!(item.command, DrawCommand::Backdrop(..)))
        .map(|at| from + at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blur_spreads_a_bright_pixel_and_keeps_the_total() {
        let (w, h) = (21, 21);
        let mut px = vec![0xFF00_0000; w * h];
        px[10 * w + 10] = 0xFFFF_FFFF;
        blur(&mut px, w, h, 6.0, (&mut Vec::new(), &mut Vec::new()));
        let red = |p: u32| (p >> 16) & 0xFF;
        let total: u32 = px.iter().map(|&p| red(p)).sum();
        assert!(red(px[10 * w + 10]) < 255, "the centre dims");
        assert!(red(px[10 * w + 12]) > 0, "light reaches two pixels away");
        assert!(
            (200..=310).contains(&total),
            "about the light it began with: {total}"
        );
    }

    #[test]
    fn a_wide_blur_on_a_shrunk_copy_matches_the_full_one_closely() {
        let (w, h) = (96, 64);
        let mut stripes: Vec<u32> = (0..w * h)
            .map(|i| {
                if (i % w) / 6 % 2 == 0 {
                    0xFFFF_FFFF
                } else {
                    0xFF00_0000
                }
            })
            .collect();
        let mut full = stripes.clone();
        box_passes(&mut full, w, h, 30.0, &mut Vec::new());
        blur(&mut stripes, w, h, 30.0, (&mut Vec::new(), &mut Vec::new()));
        let red = |p: u32| ((p >> 16) & 0xFF) as i32;
        let worst = full
            .iter()
            .zip(&stripes)
            .skip(w * 8)
            .take(w * (h - 16))
            .map(|(a, b)| (red(*a) - red(*b)).abs())
            .max()
            .unwrap_or(0);
        assert!(worst <= 24, "shrunk blur strays {worst} from the full one");
    }

    #[test]
    fn saturate_zero_is_grey_and_brightness_scales() {
        let mut px = [0xFF_FF_00_00u32];
        run(
            &Effect::Saturate(0.0),
            &mut px,
            1,
            1,
            (&mut Vec::new(), &mut Vec::new()),
        );
        let [r, g, b] = [(px[0] >> 16) & 0xFF, (px[0] >> 8) & 0xFF, px[0] & 0xFF];
        assert!(r == g && g == b, "{px:x?}");
        let mut px = [0xFF_40_40_40u32];
        run(
            &Effect::Brightness(2.0),
            &mut px,
            1,
            1,
            (&mut Vec::new(), &mut Vec::new()),
        );
        assert_eq!(px[0], 0xFF_80_80_80);
    }

    use crate::cpu::CpuRasterizer;
    use crate::renderer::Renderer;
    use crate::types::{Paint, Path};

    /// Stripes 8 pixels wide, black and white, with the white column at `shift` moved.
    fn stripes(raster: &mut CpuRasterizer, shift: f32, glass: bool) -> Vec<u32> {
        let (w, h) = (800u32, 600u32);
        {
            let mut ctx = raster.begin_frame().expect("frame");
            ctx.clear(Color::rgb(0, 0, 0)).expect("clear");
            let mut x = 0.0;
            while x < w as f32 {
                let at = if x == 400.0 { x + shift } else { x };
                ctx.draw_rect(
                    Rect::new(at, 0.0, 8.0, h as f32),
                    &Paint::new().color(Color::rgb(255, 255, 255)),
                )
                .expect("stripe");
                x += 16.0;
            }
            if glass {
                let shape = Path::rounded_rect(Rect::new(100.0, 100.0, 300.0, 200.0), 40.0);
                ctx.backdrop(&shape, &[Effect::Blur(24.0)])
                    .expect("backdrop");
            }
        }
        raster.end_frame().expect("end");
        raster
            .frame_output()
            .expect("out")
            .bytes()
            .chunks(4)
            .map(|p| u32::from(p[1]))
            .collect()
    }

    #[test]
    fn a_backdrop_blurs_only_inside_its_shape_across_bands() {
        // big enough that the frame is drawn in bands
        let mut raster = CpuRasterizer::new(800, 600);
        let px = stripes(&mut raster, 0.0, true);
        let at = |x: usize, y: usize| px[y * 800 + x];
        let inside = at(250, 200);
        assert!(
            (60..200).contains(&inside),
            "blurred to grey inside: {inside}"
        );
        assert!(
            at(250, 50) == 0 || at(250, 50) == 255,
            "sharp outside: {}",
            at(250, 50)
        );
        assert!(
            at(102, 102) == 0 || at(102, 102) == 255,
            "sharp in the cut corner: {}",
            at(102, 102)
        );
        // rows across a band edge blur alike
        let (a, b) = (at(250, 149), at(250, 151));
        assert!(a.abs_diff(b) < 8, "no seam between bands: {a} {b}");
    }

    #[test]
    fn a_change_beside_the_glass_repaints_all_of_it() {
        let mut raster = CpuRasterizer::new(800, 600);
        stripes(&mut raster, 0.0, true);
        let moved = stripes(&mut raster, 4.0, true);
        let mut fresh = CpuRasterizer::new(800, 600);
        let whole = stripes(&mut fresh, 4.0, true);
        assert_eq!(moved, whole, "a partial repaint matches a whole one");
    }

    #[test]
    fn a_tint_lays_its_colour_over_by_its_alpha() {
        let mut px = [0xFF_00_00_00u32];
        run(
            &Effect::Tint(Color::rgba(255, 255, 255, 128)),
            &mut px,
            1,
            1,
            (&mut Vec::new(), &mut Vec::new()),
        );
        assert_eq!(px[0], 0xFF_80_80_80);
    }
}
