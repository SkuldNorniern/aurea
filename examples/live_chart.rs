//! A scrolling live chart.

use aurea::prelude::*;
use aurea::render::request_canvas_redraw;
use aurea::ui::{GraphView, Theme, UiState};
use aurea_foundation::lock;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

const W: u32 = 800;
const H: u32 = 400;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Live chart", W as i32, H as i32)?;
    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    let theme = Theme::dark();
    canvas.set_background_color(theme.palette.background);

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

    let ui = Rc::new(RefCell::new(UiState::<u8>::new()));
    let drawn = Rc::clone(&ui);
    canvas.set_draw_callback(move |ctx| {
        let area = Rect::new(0.0, 0.0, ctx.width() as f32, ctx.height() as f32);
        let mut state = drawn.borrow_mut();
        let mut ui = state.frame(ctx, &theme);
        GraphView::new(&mut lock(&chart)).show(&mut ui, 0, area.inset(12.0))?;
        Ok(())
    })?;

    let id = canvas.id();
    window.on_event(move |event| {
        if ui.borrow_mut().handle(&event) {
            request_canvas_redraw(id);
        }
    });

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas.clone())?;
    window.set_content(layout)?;
    window.show();
    window.run()
}
