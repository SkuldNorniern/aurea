//! Pixel-level conformance tests for the CPU rasterizer.
//!
//! Unit tests elsewhere cover the rasterizer's bookkeeping — damage diffing,
//! tile hashes, cache keys. These cover the thing that bookkeeping exists to
//! serve: the pixels. Every drawing primitive and every piece of drawing state
//! that claims to affect output is asserted against known colours here, so a
//! state that the public API accepts but the rasterizer ignores fails loudly
//! instead of silently rendering the wrong frame.

use aurea_render::{
    Color, CpuRasterizer, DrawingContext, Paint, PaintStyle, Path, PathCommand, Point, Rect,
    Renderer, Surface, SurfaceInfo,
};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::slice::from_raw_parts;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Paths are recorded in physical pixels like every other command, so a HiDPI
/// path lands where the scale factor says it should. It used to stay logical
/// and get scaled again at tessellation time.
#[test]
fn paths_are_recorded_in_physical_pixels() {
    let mut path = Path::new();
    path.commands
        .push(PathCommand::MoveTo(Point::new(2.0, 2.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(6.0, 2.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(6.0, 6.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(2.0, 6.0)));
    path.commands.push(PathCommand::Close);

    let buf = render_scaled(2.0, |ctx| {
        ctx.draw_path(&path, &fill(RED)).expect("draw_path");
    });

    // Logical 2..6 at scale 2 covers physical 4..12.
    assert_px(&buf, 8, 8, opaque(RED));
    assert_eq!(
        px(&buf, 2, 2) >> 24,
        0,
        "logical coords should be empty{}",
        dump(&buf)
    );
}

/// A click on an interactive path is compared against the recorded geometry,
/// so both have to be in the same space. Under HiDPI they were not.
#[test]
fn interactive_paths_hit_test_in_the_same_space_as_the_click() {
    use aurea_foundation::AureaResult;
    use aurea_render::cpu::RecordingContext;
    use aurea_render::{DisplayList, InteractionRegistry, InteractiveId};

    let mut path = Path::new();
    path.commands
        .push(PathCommand::MoveTo(Point::new(10.0, 10.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(20.0, 10.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(20.0, 20.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(10.0, 20.0)));
    path.commands.push(PathCommand::Close);

    let mut list = DisplayList::new();
    {
        let mut ctx = RecordingContext::new(&mut list, 64, 64);
        ctx.set_scale_factor(2.0);
        ctx.set_interactive_id(Some(InteractiveId(7)));
        ctx.draw_path(&path, &fill(RED)).expect("draw_path");
    }

    let registry = InteractionRegistry::new();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = Arc::clone(&hits);
    registry.register_click(
        InteractiveId(7),
        Arc::new(move |_| -> AureaResult<()> {
            hits_clone.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }),
    );

    // Logical (15, 15) is physical (30, 30) at scale 2 — inside the path.
    registry
        .handle_click(&list, Point::new(30.0, 30.0))
        .expect("handle_click");
    assert_eq!(
        hits.load(Ordering::Relaxed),
        1,
        "click inside the path should hit"
    );

    registry
        .handle_click(&list, Point::new(15.0, 15.0))
        .expect("handle_click");
    assert_eq!(
        hits.load(Ordering::Relaxed),
        1,
        "logical coords are outside the physical path"
    );
}

fn stroke(color: Color, width: f32) -> Paint {
    Paint::new()
        .color(color)
        .style(PaintStyle::Stroke)
        .stroke_width(width)
}

/// A stroked path used to be filled instead, so a polyline came out as a solid
/// polygon and a straight line came out as nothing.
#[test]
fn a_stroked_line_is_a_band_not_a_filled_shape() {
    let mut path = Path::new();
    path.commands
        .push(PathCommand::MoveTo(Point::new(4.0, 16.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(28.0, 16.0)));

    let buf = render(|ctx| {
        ctx.draw_path(&path, &stroke(RED, 4.0)).expect("draw_path");
    });

    assert_ne!(px(&buf, 16, 16) >> 24, 0, "the line itself{}", dump(&buf));
    assert_eq!(
        px(&buf, 16, 4) >> 24,
        0,
        "well above the line{}",
        dump(&buf)
    );
    assert_eq!(
        px(&buf, 16, 28) >> 24,
        0,
        "well below the line{}",
        dump(&buf)
    );
}

#[test]
fn a_stroke_is_as_wide_as_it_was_asked_to_be() {
    let mut path = Path::new();
    path.commands
        .push(PathCommand::MoveTo(Point::new(4.0, 16.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(28.0, 16.0)));

    let buf = render(|ctx| {
        ctx.draw_path(&path, &stroke(RED, 6.0)).expect("draw_path");
    });

    let covered = (0..H).filter(|y| px(&buf, 16, *y) >> 24 != 0).count();
    assert!(
        (5..=8).contains(&covered),
        "a 6px stroke covered {covered} rows{}",
        dump(&buf)
    );
}

/// The inside of a bend must stay solid. Filling one self-overlapping outline
/// would cancel the overlap under the odd-even rule and open a hole.
#[test]
fn a_right_angle_join_has_no_hole() {
    let mut path = Path::new();
    path.commands
        .push(PathCommand::MoveTo(Point::new(6.0, 8.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(20.0, 8.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(20.0, 24.0)));

    let buf = render(|ctx| {
        ctx.draw_path(&path, &stroke(RED, 6.0)).expect("draw_path");
    });

    assert_ne!(px(&buf, 20, 8) >> 24, 0, "the corner itself{}", dump(&buf));
    assert_ne!(
        px(&buf, 19, 9) >> 24,
        0,
        "just inside the corner{}",
        dump(&buf)
    );
}

#[test]
fn draw_line_produces_a_line() {
    let buf = render(|ctx| {
        ctx.draw_line(4.0, 4.0, 28.0, 28.0, &stroke(RED, 3.0))
            .expect("draw_line");
    });

    assert_ne!(px(&buf, 16, 16) >> 24, 0, "on the diagonal{}", dump(&buf));
    assert_eq!(px(&buf, 4, 28) >> 24, 0, "off the diagonal{}", dump(&buf));
}

/// Filling the outline is what gives a stroke its antialiasing: the edge of a
/// diagonal line lands on partly covered pixels.
#[test]
fn a_diagonal_stroke_is_antialiased() {
    let buf = render(|ctx| {
        ctx.draw_line(4.0, 4.0, 28.0, 28.0, &stroke(RED, 3.0))
            .expect("draw_line");
    });

    let partial = buf
        .iter()
        .filter(|&&p| {
            let a = p >> 24;
            a > 0 && a < 255
        })
        .count();
    assert!(
        partial > 4,
        "expected soft edges, got {partial}{}",
        dump(&buf)
    );
}

#[test]
fn a_stroked_rect_path_leaves_its_middle_empty() {
    let mut path = Path::new();
    path.commands
        .push(PathCommand::MoveTo(Point::new(8.0, 8.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(24.0, 8.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(24.0, 24.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(8.0, 24.0)));
    path.commands.push(PathCommand::Close);

    let buf = render(|ctx| {
        ctx.draw_path(&path, &stroke(RED, 2.0)).expect("draw_path");
    });

    assert_ne!(px(&buf, 8, 16) >> 24, 0, "the left edge{}", dump(&buf));
    assert_eq!(px(&buf, 16, 16) >> 24, 0, "the middle{}", dump(&buf));
}

#[test]
fn a_filled_path_is_still_filled() {
    let mut path = Path::new();
    path.commands
        .push(PathCommand::MoveTo(Point::new(8.0, 8.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(24.0, 8.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(24.0, 24.0)));
    path.commands
        .push(PathCommand::LineTo(Point::new(8.0, 24.0)));
    path.commands.push(PathCommand::Close);

    let buf = render(|ctx| {
        ctx.draw_path(&path, &fill(RED)).expect("draw_path");
    });

    assert_ne!(px(&buf, 16, 16) >> 24, 0, "the middle{}", dump(&buf));
}

/// Gradients used to receive only the device scale, so the fill moved under a
/// transform while its colours stayed where they were.
#[test]
fn a_gradient_moves_with_its_rect() {
    use aurea_render::{GradientStop, LinearGradient};

    let gradient = LinearGradient {
        start: Point::new(0.0, 0.0),
        end: Point::new(8.0, 0.0),
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: RED,
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgb(0, 0, 255),
            },
        ],
    };

    let buf = render(|ctx| {
        ctx.translate(16.0, 8.0).expect("translate");
        ctx.fill_linear_gradient(&gradient, Rect::new(0.0, 0.0, 8.0, 8.0))
            .expect("gradient");
    });

    // The fill landed at the translated position...
    assert_ne!(
        px(&buf, 18, 10) >> 24,
        0,
        "gradient should be here{}",
        dump(&buf)
    );
    assert_eq!(
        px(&buf, 2, 2) >> 24,
        0,
        "and not at the origin{}",
        dump(&buf)
    );

    // ...and runs red to blue across it, rather than being a flat colour.
    let left = px(&buf, 17, 11);
    let right = px(&buf, 23, 11);
    assert!(
        (left >> 16) & 0xFF > (right >> 16) & 0xFF,
        "left {left:#010x} should be redder than right {right:#010x}"
    );
}

#[test]
fn a_radial_gradient_moves_and_scales_with_the_transform() {
    use aurea_render::{GradientStop, RadialGradient};

    let gradient = RadialGradient {
        center: Point::new(4.0, 4.0),
        radius: 4.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: RED,
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgb(0, 0, 255),
            },
        ],
    };

    let buf = render(|ctx| {
        ctx.translate(12.0, 12.0).expect("translate");
        ctx.fill_radial_gradient(&gradient, Rect::new(0.0, 0.0, 8.0, 8.0))
            .expect("gradient");
    });

    // The centre of the gradient sits at the translated centre.
    let centre = px(&buf, 16, 16);
    assert!(
        (centre >> 16) & 0xFF > 128,
        "the middle should be near the first stop, got {centre:#010x}{}",
        dump(&buf)
    );
}

/// Images used to receive only the device scale, so a translated image stayed
/// at the origin.
#[test]
fn an_image_moves_with_the_transform() {
    use aurea_render::Image;

    let image = Image {
        width: 4,
        height: 4,
        data: vec![255u8; 4 * 4 * 4].into(),
    };

    let buf = render(|ctx| {
        ctx.translate(10.0, 10.0).expect("translate");
        ctx.draw_image(&image, Point::new(0.0, 0.0))
            .expect("draw_image");
    });

    assert_ne!(
        px(&buf, 11, 11) >> 24,
        0,
        "image should be here{}",
        dump(&buf)
    );
    assert_eq!(
        px(&buf, 1, 1) >> 24,
        0,
        "and not at the origin{}",
        dump(&buf)
    );
}

#[test]
fn a_scaled_image_covers_more_ground() {
    use aurea_render::Image;

    let image = Image {
        width: 4,
        height: 4,
        data: vec![255u8; 4 * 4 * 4].into(),
    };

    let plain = render(|ctx| {
        ctx.draw_image(&image, Point::new(2.0, 2.0))
            .expect("draw_image");
    });
    let scaled = render(|ctx| {
        ctx.scale(2.0, 2.0).expect("scale");
        ctx.draw_image(&image, Point::new(1.0, 1.0))
            .expect("draw_image");
    });

    let covered = |b: &[u32]| b.iter().filter(|&&p| p >> 24 != 0).count();
    assert!(
        covered(&scaled) > covered(&plain),
        "scaled {} vs plain {}",
        covered(&scaled),
        covered(&plain)
    );
}

/// Text used to receive only the device scale, so it stayed put while the rest
/// of the drawing moved.
#[test]
fn text_moves_with_the_transform() {
    let plain = render(|ctx| {
        ctx.draw_text("Hg", Point::new(4.0, 20.0), &fill(RED))
            .expect("draw_text");
    });
    let moved = render(|ctx| {
        ctx.translate(12.0, 0.0).expect("translate");
        ctx.draw_text("Hg", Point::new(4.0, 20.0), &fill(RED))
            .expect("draw_text");
    });

    let leftmost = |b: &[u32]| (0..W).find(|x| (0..H).any(|y| px(b, *x, y) >> 24 != 0));
    let (Some(a), Some(b)) = (leftmost(&plain), leftmost(&moved)) else {
        // No font available in this environment; nothing to compare.
        return;
    };
    assert!(
        b > a,
        "translated text should start further right: {a} then {b}"
    );
}

fn rect_path(r: Rect) -> Path {
    let mut path = Path::new();
    path.commands
        .push(PathCommand::MoveTo(Point::new(r.x, r.y)));
    path.commands
        .push(PathCommand::LineTo(Point::new(r.x + r.width, r.y)));
    path.commands.push(PathCommand::LineTo(Point::new(
        r.x + r.width,
        r.y + r.height,
    )));
    path.commands
        .push(PathCommand::LineTo(Point::new(r.x, r.y + r.height)));
    path.commands.push(PathCommand::Close);
    path
}

/// A rectangular clip path is enforced like `clip_rect`.
#[test]
fn clip_path_with_a_rectangle_clips() {
    let buf = render(|ctx| {
        ctx.clip_path(&rect_path(Rect::new(8.0, 8.0, 8.0, 8.0)))
            .expect("clip_path");
        ctx.draw_rect(Rect::new(0.0, 0.0, 32.0, 32.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 12, 12, opaque(RED));
    assert_px(&buf, 4, 4, 0);
    assert_px(&buf, 20, 20, 0);
}

/// Anything else is refused. Accepting it and drawing unclipped would paint
/// over whatever the clip was meant to protect.
#[test]
fn clip_path_refuses_a_shape_it_cannot_enforce() {
    let mut triangle = Path::new();
    triangle
        .commands
        .push(PathCommand::MoveTo(Point::new(4.0, 4.0)));
    triangle
        .commands
        .push(PathCommand::LineTo(Point::new(28.0, 4.0)));
    triangle
        .commands
        .push(PathCommand::LineTo(Point::new(16.0, 28.0)));
    triangle.commands.push(PathCommand::Close);

    let mut r = CpuRasterizer::new(W, H);
    r.init(
        Surface::Cpu,
        SurfaceInfo {
            width: W,
            height: H,
            scale_factor: 1.0,
        },
    )
    .expect("init");

    let mut ctx = r.begin_frame().expect("begin_frame");
    let result = ctx.clip_path(&triangle);

    assert!(result.is_err(), "a triangular clip cannot be enforced");
}

#[test]
fn clip_path_survives_the_transform_that_makes_it_a_rectangle() {
    // Rotating a rectangle by a quarter turn leaves it axis-aligned.
    let buf = render(|ctx| {
        ctx.translate(16.0, 16.0).expect("translate");
        ctx.rotate(FRAC_PI_2).expect("rotate");
        ctx.clip_path(&rect_path(Rect::new(-8.0, -8.0, 16.0, 16.0)))
            .expect("a rotated rectangle is still a rectangle");
        ctx.draw_rect(Rect::new(-32.0, -32.0, 64.0, 64.0), &fill(RED))
            .expect("draw_rect");
    });

    assert_px(&buf, 16, 16, opaque(RED));
    assert_px(&buf, 2, 2, 0);
}

/// A frame where only a small thing moves must leave the rest of the picture
/// exactly as it was, even with a full-surface background drawn every frame.
///
/// The background spans every tile, so redrawing it used to drag the whole
/// surface into the repaint. Clipping each draw to the repainted region is what
/// removed that, and this checks the pixels it was protecting are still right.
#[test]
fn a_small_change_leaves_the_rest_of_the_frame_intact() {
    let mut r = CpuRasterizer::new(W, H);
    r.init(
        Surface::Cpu,
        SurfaceInfo {
            width: W,
            height: H,
            scale_factor: 1.0,
        },
    )
    .expect("init");

    let paint_scene = |ctx: &mut dyn DrawingContext, marker_x: f32| {
        // Full-surface background, redrawn every frame.
        ctx.clear(Color::rgb(20, 20, 20)).expect("clear");
        // Something static in a corner, far from the marker.
        ctx.draw_rect(Rect::new(2.0, 2.0, 6.0, 6.0), &fill(Color::rgb(0, 200, 0)))
            .expect("static");
        // And the one thing that moves.
        ctx.draw_rect(Rect::new(marker_x, 20.0, 2.0, 5.0), &fill(RED))
            .expect("marker");
    };

    // First frame, fully painted.
    r.set_damage(Some(Rect::new(0.0, 0.0, W as f32, H as f32)));
    {
        let mut ctx = r.begin_frame().expect("begin");
        paint_scene(ctx.as_mut(), 10.0);
    }
    r.end_frame().expect("end");

    // Second frame: only the marker moves.
    {
        let mut ctx = r.begin_frame().expect("begin");
        paint_scene(ctx.as_mut(), 24.0);
    }
    r.end_frame().expect("end");

    let (ptr, len, _, _) = r.get_buffer();
    let bytes = unsafe { from_raw_parts(ptr, len) };
    let buf: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    // The static square is untouched.
    assert_px(&buf, 4, 4, opaque(Color::rgb(0, 200, 0)));
    // The marker moved: gone from where it was, present where it is.
    assert_px(&buf, 24, 22, opaque(RED));
    assert_ne!(
        px(&buf, 10, 22),
        opaque(RED),
        "the old marker should have been painted over{}",
        dump(&buf)
    );
    // And the background is still the background.
    assert_px(&buf, 28, 28, opaque(Color::rgb(20, 20, 20)));
}
