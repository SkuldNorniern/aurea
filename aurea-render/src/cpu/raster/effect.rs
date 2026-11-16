//! Backdrop effects on the CPU: the box under a shape copied out, changed, and written back.

use super::band::Band;
use crate::cpu::clip::ClipBox;
use crate::effect::{BackdropPixels, Effect};
use crate::types::{Color, Rect};

/// Buffers a backdrop reuses from one frame to the next.
#[derive(Default)]
pub(crate) struct EffectScratch {
    pixels: Vec<u32>,
    line: Vec<u32>,
}

/// Box passes per direction; three come close to a Gaussian.
const PASSES: usize = 3;

/// Runs `effects` on the pixels of `bounds` inside `clip`, reading as far around as they reach.
/// `buf` must hold every row the effects read, as a whole frame does.
pub(crate) fn apply(
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
        run(effect, &mut s.pixels, w, h, &mut s.line);
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

fn run(effect: &Effect, pixels: &mut [u32], w: usize, h: usize, line: &mut Vec<u32>) {
    match effect {
        Effect::Blur(radius) => blur(pixels, w, h, *radius, line),
        Effect::Saturate(amount) => per_pixel(pixels, |[r, g, b]| {
            let grey = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            [r, g, b].map(|c| grey + (c - grey) * amount)
        }),
        Effect::Brightness(amount) => per_pixel(pixels, |rgb| rgb.map(|c| c * amount)),
        Effect::Tint(color) => tint(pixels, *color),
        Effect::Custom(filter) => filter.filter(&mut BackdropPixels::new(pixels, w, h)),
    }
}

/// `f` on each pixel's red, green and blue as 0..=255, alpha kept.
fn per_pixel(pixels: &mut [u32], f: impl Fn([f32; 3]) -> [f32; 3]) {
    for p in pixels {
        let rgb = [(*p >> 16) & 0xFF, (*p >> 8) & 0xFF, *p & 0xFF].map(|c| c as f32);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let [r, g, b] = f(rgb).map(|c| c.round().clamp(0.0, 255.0) as u32);
        *p = (*p & 0xFF00_0000) | (r << 16) | (g << 8) | b;
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

/// A blur spreading about `radius` pixels: box passes along rows, then along columns.
fn blur(pixels: &mut [u32], w: usize, h: usize, radius: f32, line: &mut Vec<u32>) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let half = (radius / PASSES as f32).round().max(0.0) as usize;
    if half == 0 || w == 0 || h == 0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blur_spreads_a_bright_pixel_and_keeps_the_total() {
        let (w, h) = (21, 21);
        let mut px = vec![0xFF00_0000; w * h];
        px[10 * w + 10] = 0xFFFF_FFFF;
        blur(&mut px, w, h, 6.0, &mut Vec::new());
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
    fn saturate_zero_is_grey_and_brightness_scales() {
        let mut px = [0xFF_FF_00_00u32];
        run(&Effect::Saturate(0.0), &mut px, 1, 1, &mut Vec::new());
        let [r, g, b] = [(px[0] >> 16) & 0xFF, (px[0] >> 8) & 0xFF, px[0] & 0xFF];
        assert!(r == g && g == b, "{px:x?}");
        let mut px = [0xFF_40_40_40u32];
        run(&Effect::Brightness(2.0), &mut px, 1, 1, &mut Vec::new());
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
            &mut Vec::new(),
        );
        assert_eq!(px[0], 0xFF_80_80_80);
    }
}
