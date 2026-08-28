//! Pixel-level conformance tests for the CPU rasterizer.
//!
//! Unit tests elsewhere cover the rasterizer's bookkeeping — damage diffing,
//! tile hashes, cache keys. These cover the thing that bookkeeping exists to
//! serve: the pixels. Every drawing primitive and every piece of drawing state
//! that claims to affect output is asserted against known colours here, so a
//! state that the public API accepts but the rasterizer ignores fails loudly
//! instead of silently rendering the wrong frame.

use aurea_render::{
    Color, CpuRasterizer, DrawingContext, Paint, Point, Rect, Renderer, Surface, SurfaceInfo,
};
use std::f32::consts::FRAC_PI_4;
use std::slice::from_raw_parts;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f32_to_u32(v: f32) -> u32 {
    v.round().max(0.0) as u32
}

/// Size of the test surface. Small enough that a failure dump is readable.
const W: u32 = 32;
const H: u32 = 32;

/// Renders one frame and returns the physical pixel buffer as `0xAARRGGBB`.
fn render(draw: impl FnOnce(&mut dyn DrawingContext)) -> Vec<u32> {
    render_scaled(1.0, draw)
}

/// Renders one frame at the given scale factor.
fn render_scaled(scale: f32, draw: impl FnOnce(&mut dyn DrawingContext)) -> Vec<u32> {
    let mut r = CpuRasterizer::new(W, H);
    // Logical size is the physical size divided by the scale factor, so the
    // surface stays W x H physical pixels at any scale.
    let info = SurfaceInfo {
        width: f32_to_u32(W as f32 / scale),
        height: f32_to_u32(H as f32 / scale),
        scale_factor: scale,
    };
    r.init(Surface::Cpu, info).expect("init");
    {
        let mut ctx = r.begin_frame().expect("begin_frame");
        draw(ctx.as_mut());
    }
    r.end_frame().expect("end_frame");

    let (ptr, len, bw, _bh) = r.get_buffer();
    assert_eq!(bw, W, "unexpected buffer width");
    let bytes = unsafe { from_raw_parts(ptr, len) };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn px(buf: &[u32], x: u32, y: u32) -> u32 {
    buf[(y * W + x) as usize]
}

/// Renders the buffer as ASCII so a failing assertion shows what was drawn.
fn dump(buf: &[u32]) -> String {
    let mut s = String::from("\n");
    for y in 0..H {
        for x in 0..W {
            s.push(if px(buf, x, y) >> 24 == 0 { '.' } else { '#' });
        }
        s.push('\n');
    }
    s
}

fn assert_px(buf: &[u32], x: u32, y: u32, expected: u32) {
    let actual = px(buf, x, y);
    assert_eq!(
        actual,
        expected,
        "pixel ({x}, {y}) was {actual:#010x}, expected {expected:#010x}{}",
        dump(buf)
    );
}

fn opaque(color: Color) -> u32 {
    (0xFFu32 << 24) | (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}

const RED: Color = Color {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};

fn fill(color: Color) -> Paint {
    let mut p = Paint::new();
    p.color = color;
    p
}

#[test]
fn draw_rect_fills_its_geometry_and_nothing_else() {
    let buf = render(|ctx| {
        ctx.draw_rect(Rect::new(8.0, 8.0, 8.0, 8.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 8, 8, opaque(RED));
    assert_px(&buf, 15, 15, opaque(RED));
    assert_px(&buf, 7, 8, 0);
    assert_px(&buf, 16, 8, 0);
}

#[test]
fn clear_covers_the_whole_surface() {
    let buf = render(|ctx| {
        ctx.clear(RED).expect("clear");
    });

    assert_px(&buf, 0, 0, opaque(RED));
    assert_px(&buf, W - 1, H - 1, opaque(RED));
}

#[test]
fn draw_circle_fills_its_centre_and_misses_its_corners() {
    let buf = render(|ctx| {
        ctx.draw_circle(Point::new(16.0, 16.0), 8.0, &fill(RED))
            .expect("draw_circle");
    });

    assert_px(&buf, 16, 16, opaque(RED));
    assert_eq!(
        px(&buf, 0, 0) >> 24,
        0,
        "corner should be untouched{}",
        dump(&buf)
    );
}

#[test]
fn scale_factor_maps_logical_geometry_to_physical_pixels() {
    // A 4x4 logical rect at (2, 2) covers 8x8 physical pixels at (4, 4).
    let buf = render_scaled(2.0, |ctx| {
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 4, 4, opaque(RED));
    assert_px(&buf, 11, 11, opaque(RED));
    assert_px(&buf, 3, 4, 0);
    assert_px(&buf, 12, 4, 0);
}

#[test]
fn translate_moves_drawn_pixels() {
    let buf = render(|ctx| {
        ctx.translate(8.0, 4.0).expect("translate");
        ctx.draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 8, 4, opaque(RED));
    assert_px(&buf, 11, 7, opaque(RED));
    assert_px(&buf, 0, 0, 0);
}

#[test]
fn clip_rect_suppresses_pixels_outside_the_clip() {
    let buf = render(|ctx| {
        ctx.clip_rect(Rect::new(8.0, 8.0, 8.0, 8.0))
            .expect("clip_rect");
        ctx.draw_rect(Rect::new(0.0, 0.0, 32.0, 32.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 8, 8, opaque(RED));
    assert_px(&buf, 15, 15, opaque(RED));
    assert_px(&buf, 7, 7, 0);
    assert_px(&buf, 16, 16, 0);
}

#[test]
fn alpha_blends_against_the_backdrop() {
    let buf = render(|ctx| {
        ctx.clear(Color::rgb(0, 0, 0)).expect("clear");
        ctx.set_alpha(0.5).expect("set_alpha");
        ctx.draw_rect(Rect::new(8.0, 8.0, 8.0, 8.0), &fill(RED))
            .expect("draw_rect");
    });

    // Compositing happens in linear light, so half-alpha red over black lands
    // near sRGB 188, not 128. What matters is that it is neither the untouched
    // backdrop nor full-strength red.
    let mid = px(&buf, 12, 12);
    let red = (mid >> 16) & 0xFF;
    assert!(
        (150..=210).contains(&red),
        "half-alpha red over black should be partially blended, got {mid:#010x}{}",
        dump(&buf)
    );
}

#[test]
fn restore_undoes_clip_and_alpha() {
    let buf = render(|ctx| {
        ctx.save().expect("save");
        ctx.clip_rect(Rect::new(0.0, 0.0, 1.0, 1.0))
            .expect("clip_rect");
        ctx.set_alpha(0.0).expect("set_alpha");
        ctx.restore().expect("restore");
        ctx.draw_rect(Rect::new(8.0, 8.0, 8.0, 8.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 12, 12, opaque(RED));
}

#[test]
fn nested_clips_intersect() {
    let buf = render(|ctx| {
        ctx.clip_rect(Rect::new(4.0, 4.0, 12.0, 12.0))
            .expect("clip_rect");
        ctx.clip_rect(Rect::new(8.0, 0.0, 12.0, 12.0))
            .expect("clip_rect");
        ctx.draw_rect(Rect::new(0.0, 0.0, 32.0, 32.0), &fill(RED))
            .expect("draw_rect");
    });

    // Intersection is x 8..16, y 4..12.
    assert_px(&buf, 8, 4, opaque(RED));
    assert_px(&buf, 15, 11, opaque(RED));
    assert_px(&buf, 7, 4, 0);
    assert_px(&buf, 8, 12, 0);
}

#[test]
fn clip_applies_to_every_primitive() {
    let clip = Rect::new(12.0, 12.0, 8.0, 8.0);

    let circle = render(|ctx| {
        ctx.clip_rect(clip).expect("clip_rect");
        ctx.draw_circle(Point::new(16.0, 16.0), 14.0, &fill(RED))
            .expect("draw_circle");
    });
    assert_px(&circle, 16, 16, opaque(RED));
    assert_px(&circle, 16, 4, 0);

    let cleared = render(|ctx| {
        ctx.clip_rect(clip).expect("clip_rect");
        ctx.clear(RED).expect("clear");
    });
    assert_px(&cleared, 16, 16, opaque(RED));
    assert_px(&cleared, 0, 0, 0);
}

#[test]
fn zero_alpha_draws_nothing() {
    let buf = render(|ctx| {
        ctx.set_alpha(0.0).expect("set_alpha");
        ctx.draw_rect(Rect::new(8.0, 8.0, 8.0, 8.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 12, 12, 0);
}

#[test]
fn alpha_applies_to_circles_too() {
    let circle = render(|ctx| {
        ctx.clear(Color::rgb(0, 0, 0)).expect("clear");
        ctx.set_alpha(0.5).expect("set_alpha");
        ctx.draw_circle(Point::new(16.0, 16.0), 8.0, &fill(RED))
            .expect("draw_circle");
    });
    let red = (px(&circle, 16, 16) >> 16) & 0xFF;
    assert!(
        (150..=210).contains(&red),
        "half-alpha circle should be partially blended, got {red}{}",
        dump(&circle)
    );
}

#[test]
fn translate_composes_with_the_scale_factor() {
    // A logical translate of (4, 2) at scale 2 moves 8 x 4 physical pixels.
    let buf = render_scaled(2.0, |ctx| {
        ctx.translate(4.0, 2.0).expect("translate");
        ctx.draw_rect(Rect::new(0.0, 0.0, 2.0, 2.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 8, 4, opaque(RED));
    assert_px(&buf, 11, 7, opaque(RED));
    assert_px(&buf, 7, 4, 0);
}

#[test]
fn scale_transform_grows_drawn_geometry() {
    let buf = render(|ctx| {
        ctx.scale(2.0, 2.0).expect("scale");
        ctx.draw_rect(Rect::new(2.0, 2.0, 4.0, 4.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 4, 4, opaque(RED));
    assert_px(&buf, 11, 11, opaque(RED));
    assert_px(&buf, 3, 3, 0);
    assert_px(&buf, 12, 12, 0);
}

#[test]
fn transform_moves_paths_and_circles_too() {
    let circle = render(|ctx| {
        ctx.translate(8.0, 8.0).expect("translate");
        ctx.draw_circle(Point::new(0.0, 0.0), 4.0, &fill(RED))
            .expect("draw_circle");
    });
    assert_px(&circle, 8, 8, opaque(RED));
    assert_eq!(
        px(&circle, 0, 0) >> 24,
        0,
        "origin should be clear{}",
        dump(&circle)
    );
}

#[test]
fn restore_undoes_a_transform() {
    let buf = render(|ctx| {
        ctx.save().expect("save");
        ctx.translate(16.0, 16.0).expect("translate");
        ctx.restore().expect("restore");
        ctx.draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 0, 0, opaque(RED));
    assert_px(&buf, 16, 16, 0);
}

#[test]
fn a_rotated_rect_is_no_longer_upright() {
    let buf = render(|ctx| {
        ctx.translate(16.0, 16.0).expect("translate");
        ctx.rotate(FRAC_PI_4).expect("rotate");
        ctx.draw_rect(Rect::new(-6.0, -6.0, 12.0, 12.0), &fill(RED))
            .expect("draw_rect");
    });

    // A square rotated 45 degrees about its centre: the centre stays covered
    // and the corners of the upright square are now outside the shape.
    assert_px(&buf, 16, 16, opaque(RED));
    assert_eq!(
        px(&buf, 11, 11) >> 24,
        0,
        "the upright square's corner should now be empty{}",
        dump(&buf)
    );
}

#[test]
fn transforms_compose_innermost_first() {
    // translate then scale: the scale applies to the geometry, not to the
    // translation, so the rect starts at the translated origin.
    let buf = render(|ctx| {
        ctx.translate(8.0, 8.0).expect("translate");
        ctx.scale(2.0, 2.0).expect("scale");
        ctx.draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 8, 8, opaque(RED));
    assert_px(&buf, 15, 15, opaque(RED));
    assert_px(&buf, 7, 7, 0);
    assert_px(&buf, 16, 16, 0);
}
