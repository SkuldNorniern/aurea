//! A scrolling live chart in about a dozen lines.
//!
//! Run with: cargo run --example live_chart
//!
//! Shows the short path: `graph::quick` builds the chart, `Canvas::on_frame`
//! keeps it fed and redrawn.

use aurea::prelude::*;
use aurea_foundation::lock;
use std::sync::{Arc, Mutex};

const W: u32 = 800;
const H: u32 = 400;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Live chart", W as i32, H as i32)?;
    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(16, 18, 22));

    // A scrolling window over the last 600 samples, plus a second series.
    let (mut chart, load) = graph::quick::live("load", 600);
    let latency = chart.add_series(graph::Series::rolling("latency", 600));
    chart.y = graph::Axis::fixed(0.0, 100.0);

    let chart = Arc::new(Mutex::new(chart));

    let feed = Arc::clone(&chart);
    let mut t = 0.0f64;
    canvas.on_frame(move |info| {
        t += info.delta.as_secs_f64();
        let mut chart = lock(&feed);
        chart.push(load, 50.0 + 40.0 * (t * 1.7).sin());
        chart.push(latency, 30.0 + 20.0 * (t * 0.6).cos());
        true
    });

    let draw = Arc::clone(&chart);
    canvas.set_draw_callback(move |ctx| {
        let area = Rect::new(0.0, 0.0, ctx.width() as f32, ctx.height() as f32);
        lock(&draw).draw(ctx, area)
    })?;

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas.clone())?;
    window.set_content(layout)?;
    window.show();
    window.run()
}
