//! The graph draws real pixels.
//!
//! The unit tests cover the maths, which is where the bugs live. These cover
//! the part the maths exists for: that a trace, a grid and a scope trace end up
//! on the buffer, inside the frame they were given.

use aurea::render::graph::{
    Axis, Channel, Graph, GraphStyle, Margin, Scope, Series, Timebase, Trigger,
};
use aurea::render::{CpuRasterizer, DrawingContext, Point, Rect, Renderer, Surface, SurfaceInfo};
use std::slice::from_raw_parts;

const W: u32 = 240;
const H: u32 = 160;

/// Renders one frame and hands back the pixels as `0xAARRGGBB`.
fn render(draw: impl FnOnce(&mut dyn DrawingContext)) -> Vec<u32> {
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
    {
        let mut ctx = r.begin_frame().expect("begin_frame");
        draw(ctx.as_mut());
    }
    r.end_frame().expect("end_frame");

    let (ptr, len, _, _) = r.get_buffer();
    let bytes = unsafe { from_raw_parts(ptr, len) };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn px(buf: &[u32], x: u32, y: u32) -> u32 {
    buf[(y * W + x) as usize]
}

/// Pixels that are neither untouched nor the plot background.
fn ink(buf: &[u32], background: u32) -> usize {
    buf.iter().filter(|&&p| p != 0 && p != background).count()
}

fn frame() -> Rect {
    Rect::new(0.0, 0.0, W as f32, H as f32)
}

#[test]
fn a_line_series_puts_ink_on_the_buffer() {
    let buf = render(|ctx| {
        let mut graph = Graph::new();
        graph.add_series(Series::xy(
            "s",
            (0..50).map(|i| (f64::from(i), f64::from(i % 10))).collect(),
        ));
        graph.draw(ctx, frame()).expect("draw");
    });

    let bg = 0xFF101216;
    assert!(
        ink(&buf, bg) > 500,
        "expected a drawn plot, got {}",
        ink(&buf, bg)
    );
}

#[test]
fn an_empty_graph_still_draws_its_background() {
    let buf = render(|ctx| {
        Graph::new().draw(ctx, frame()).expect("draw");
    });

    // Dark theme background, everywhere.
    assert_ne!(px(&buf, 1, 1) >> 24, 0, "background should be painted");
}

#[test]
fn a_bare_style_leaves_the_frame_alone() {
    let buf = render(|ctx| {
        let mut graph = Graph::new().with_style(GraphStyle::bare());
        graph.add_series(Series::xy("s", vec![(0.0, 0.0), (1.0, 1.0)]));
        graph.draw(ctx, frame()).expect("draw");
    });

    // No background and no grid, so the corners stay untouched.
    assert_eq!(
        px(&buf, 0, 0),
        0,
        "bare style should not paint a background"
    );
}

/// A trace outside the axis range must not spill over the labels.
#[test]
fn the_trace_is_clipped_to_the_plot_area() {
    let style = GraphStyle {
        background: None,
        plot_background: None,
        margin: Margin::new(40.0, 10.0, 10.0, 20.0),
        ..GraphStyle::bare()
    };
    let buf = render(|ctx| {
        let mut graph = Graph::new()
            .with_style(style)
            .with_x(Axis::fixed(0.0, 1.0))
            .with_y(Axis::fixed(0.0, 1.0));
        // Well outside the range on both axes.
        graph.add_series(Series::xy("s", vec![(-50.0, -50.0), (50.0, 50.0)]));
        graph.draw(ctx, frame()).expect("draw");
    });

    // The left margin is reserved for y labels; nothing should be drawn there.
    for y in 0..H {
        for x in 0..38 {
            assert_eq!(
                px(&buf, x, y),
                0,
                "trace escaped into the margin at ({x}, {y})"
            );
        }
    }
}

#[test]
fn a_graph_smaller_than_its_margins_does_not_fail() {
    let buf = render(|ctx| {
        let mut graph = Graph::new();
        graph.add_series(Series::xy("s", vec![(0.0, 0.0), (1.0, 1.0)]));
        // Smaller than the default margins, so there is no plot area at all.
        graph
            .draw(ctx, Rect::new(0.0, 0.0, 20.0, 20.0))
            .expect("draw");
    });

    assert_eq!(buf.len(), (W * H) as usize);
}

#[test]
fn value_at_inverts_the_drawn_mapping() {
    let mut graph = Graph::new()
        .with_x(Axis::fixed(0.0, 10.0))
        .with_y(Axis::fixed(0.0, 100.0));

    render(|ctx| {
        graph.draw(ctx, frame()).expect("draw");
    });

    let area = graph.plot_area();
    let centre = Point::new(area.x + area.width / 2.0, area.y + area.height / 2.0);
    let (x, y) = graph.value_at(centre).expect("inside the plot area");

    assert!((x - 5.0).abs() < 0.1, "x was {x}");
    assert!((y - 50.0).abs() < 1.0, "y was {y}");
}

#[test]
fn value_at_is_none_outside_the_plot_area() {
    let mut graph = Graph::new();
    render(|ctx| {
        graph.draw(ctx, frame()).expect("draw");
    });

    assert_eq!(graph.value_at(Point::new(1.0, 1.0)), None);
}

#[test]
fn a_scope_draws_its_channels() {
    let buf = render(|ctx| {
        let mut scope = Scope::new(512);
        scope.timebase = Timebase::new(0.001, 10_000.0);
        let ch = scope.add_input("CH1");
        for i in 0..512 {
            let t = f64::from(i) / 10_000.0;
            scope.push(ch, (t * 1000.0).sin());
        }
        scope.trigger = Trigger::rising(0.0);
        scope.draw(ctx, frame()).expect("draw");
    });

    let bg = 0xFF101216;
    assert!(ink(&buf, bg) > 500, "expected a scope trace");
}

#[test]
fn a_scope_with_no_samples_draws_the_graticule() {
    let buf = render(|ctx| {
        let mut scope = Scope::new(64);
        scope.add_channel(Channel::new("CH1"));
        scope.draw(ctx, frame()).expect("draw");
    });

    assert_ne!(px(&buf, 1, 1) >> 24, 0, "background should be painted");
}

/// A trace has to appear whatever its density. The thinning that keeps a long
/// series cheap changes shape at a threshold, and a trace that vanishes on one
/// side of it is worse than one that is slow.
#[test]
fn a_line_is_drawn_at_every_density() {
    let bg = 0xFF101216;
    let mut blank = Vec::new();

    for count in [2usize, 10, 50, 200, 479, 480, 481, 960, 1000, 5000, 100_000] {
        let buf = render(|ctx| {
            let mut graph = Graph::new();
            graph.add_series(Series::xy(
                "s",
                (0..count)
                    .map(|i| {
                        let t = i as f64 / count as f64;
                        (t * 100.0, (t * 12.0).sin())
                    })
                    .collect(),
            ));
            graph.draw(ctx, frame()).expect("draw");
        });
        let drawn = ink(&buf, bg);
        if drawn < 200 {
            blank.push((count, drawn));
        }
    }

    assert!(blank.is_empty(), "traces with too little ink: {blank:?}");
}

/// A flat trace is still a trace. Every column holds one value, which is the
/// case the envelope has least to work with.
#[test]
fn a_flat_line_is_drawn_at_every_density() {
    let bg = 0xFF101216;
    let mut blank = Vec::new();

    for count in [50usize, 1000, 100_000] {
        let buf = render(|ctx| {
            let mut graph = Graph::new();
            graph.add_series(Series::xy(
                "s",
                (0..count).map(|i| (i as f64, 1.0)).collect(),
            ));
            graph.draw(ctx, frame()).expect("draw");
        });
        let drawn = ink(&buf, bg);
        if drawn < 100 {
            blank.push((count, drawn));
        }
    }

    assert!(
        blank.is_empty(),
        "flat traces with too little ink: {blank:?}"
    );
}

/// A live chart follows its newest data with a moving window, so the value
/// that matters most sits exactly on the right-hand edge.
#[test]
fn a_windowed_line_draws_its_newest_data() {
    let bg = 0xFF101216;
    let mut blank = Vec::new();

    for count in [50usize, 1000, 5000, 100_000] {
        let buf = render(|ctx| {
            let mut graph = Graph::new();
            graph.x = Axis::window(500.0);
            graph.add_series(Series::xy(
                "s",
                (0..count)
                    .map(|i| (i as f64, ((i as f64) * 0.05).sin()))
                    .collect(),
            ));
            graph.draw(ctx, frame()).expect("draw");
        });
        let drawn = ink(&buf, bg);
        if drawn < 200 {
            blank.push((count, drawn));
        }
    }

    assert!(
        blank.is_empty(),
        "windowed traces with too little ink: {blank:?}"
    );
}

/// A region that keeps being redrawn must not flicker: the same scene has to
/// produce the same pixels every frame, and a small change must leave the
/// rest of the buffer alone.
#[test]
fn repeated_frames_of_the_same_scene_are_stable() {
    use aurea::render::{Color, Paint, PaintStyle};

    const BG: Color = Color::rgb(16, 18, 22);

    let draw = |ctx: &mut dyn DrawingContext, caret_x: f32| {
        ctx.clear(BG).expect("clear");
        // A wide opaque panel, then content over it, then a moving caret:
        // the shape an editor draws, and what the occlusion pass reasons over.
        let panel = Paint::new().color(Color::rgb(30, 34, 40));
        ctx.draw_rect(Rect::new(0.0, 0.0, W as f32, 40.0), &panel)
            .expect("panel");
        let text = Paint::new().color(Color::rgb(200, 200, 210));
        for row in 0..6 {
            ctx.draw_rect(Rect::new(8.0, 50.0 + row as f32 * 14.0, 180.0, 8.0), &text)
                .expect("row");
        }
        let caret = Paint::new()
            .color(Color::rgb(255, 220, 120))
            .style(PaintStyle::Fill);
        ctx.draw_rect(Rect::new(caret_x, 50.0, 2.0, 84.0), &caret)
            .expect("caret");
    };

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

    let mut frame = |caret_x: f32| -> Vec<u32> {
        {
            let mut ctx = r.begin_frame().expect("begin");
            draw(ctx.as_mut(), caret_x);
        }
        r.end_frame().expect("end");
        let (ptr, len, _, _) = r.get_buffer();
        unsafe { from_raw_parts(ptr, len) }
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    };

    // Settle, then take the same scene twice: identical input, identical out.
    frame(20.0);
    let settled = frame(20.0);
    let again = frame(20.0);
    assert_eq!(settled, again, "an unchanged scene changed between frames");

    // Move the caret and put it back. The buffer has to return to what it was
    // rather than keeping a hole where something was cleared and not redrawn.
    frame(60.0);
    let returned = frame(20.0);
    let differing = settled
        .iter()
        .zip(&returned)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "{differing} pixels differ after moving the caret away and back"
    );
}

/// An overlay that comes and goes must leave the content underneath correct.
/// A panel covering something is exactly the case the occlusion pass skips
/// the covered draw for, so the frame the panel disappears has to bring it
/// back rather than show whatever the buffer happened to hold.
#[test]
fn content_returns_when_an_overlay_is_dismissed() {
    use aurea::render::{Color, Paint};

    const BG: Color = Color::rgb(16, 18, 22);

    let draw = |ctx: &mut dyn DrawingContext, overlay: bool| {
        ctx.clear(BG).expect("clear");
        let text = Paint::new().color(Color::rgb(200, 200, 210));
        for row in 0..8 {
            ctx.draw_rect(Rect::new(8.0, 20.0 + row as f32 * 14.0, 200.0, 8.0), &text)
                .expect("row");
        }
        if overlay {
            // Opaque, and wide enough to swallow whole tiles.
            let panel = Paint::new().color(Color::rgb(40, 44, 52));
            ctx.draw_rect(Rect::new(0.0, 0.0, W as f32, H as f32), &panel)
                .expect("overlay");
        }
    };

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

    let mut frame = |overlay: bool| -> Vec<u32> {
        {
            let mut ctx = r.begin_frame().expect("begin");
            draw(ctx.as_mut(), overlay);
        }
        r.end_frame().expect("end");
        let (ptr, len, _, _) = r.get_buffer();
        unsafe { from_raw_parts(ptr, len) }
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    };

    frame(false);
    let without = frame(false);
    frame(true);
    let restored = frame(false);

    let differing = without
        .iter()
        .zip(&restored)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "{differing} pixels wrong after the overlay was dismissed"
    );
}
