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
