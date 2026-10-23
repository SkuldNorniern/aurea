//! Oscilloscope view with two channels and a trigger.

use aurea::elements::{Orientation, Stack};
use aurea::render::{Canvas, Color, Rect, RendererBackend};
use aurea::ui::graph::{Channel, Scope, Timebase, Trigger};
use aurea::ui::{ScopeStyle, ScopeView, Stroke, Theme, UiState};
use aurea::{AureaResult, Container, Window};
use aurea_foundation::lock;
use aurea_runtime::FrameScheduler;
use std::cell::RefCell;
use std::f64::consts::TAU;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

const W: u32 = 900;
const H: u32 = 500;
const SAMPLE_RATE: f64 = 20_000.0;
const CAPTURE: usize = 8192;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn samples_for(seconds: f64) -> usize {
    (seconds * SAMPLE_RATE).clamp(0.0, 4096.0) as usize
}

fn main() -> AureaResult<()> {
    let mut window = Window::new("Scope", W as i32, H as i32)?;
    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    let theme = Theme::dark();
    canvas.set_background_color(theme.palette.background);

    let mut style = ScopeStyle::new(&theme);
    style.graph.plot_background = Some(Color::rgb(8, 10, 13));
    style.graph.grid.major = Stroke::new(Color::rgba(120, 200, 255, 26), 1.0);
    style.graph.grid.minor = Stroke::new(Color::rgba(120, 200, 255, 12), 1.0);
    style.graph.grid.zero = Some(Stroke::new(Color::rgba(180, 220, 255, 70), 1.0));
    style.graph.border = Some(Stroke::new(Color::rgba(120, 200, 255, 70), 1.0));

    let mut scope = Scope::new(CAPTURE);
    scope.timebase = Timebase::new(0.0005, SAMPLE_RATE);
    scope.vertical_divisions = 4;
    scope.trigger = Trigger::rising(0.0).with_position(0.1);

    scope.add_channel(
        Channel::with_capacity("CH1", CAPTURE)
            .with_color(Color::rgb(140, 225, 255))
            .with_volts_per_division(0.5)
            .with_offset(0.9),
    );
    scope.add_channel(
        Channel::with_capacity("CH2", CAPTURE)
            .with_color(Color::rgb(255, 200, 120))
            .with_volts_per_division(0.5)
            .with_offset(-0.9),
    );

    let scope = Arc::new(Mutex::new(scope));

    let feed = Arc::clone(&scope);
    let mut phase = 0.0f64;
    let ticker = canvas.on_frame(move |info| {
        let seconds = info.delta.as_secs_f64().min(0.1);
        let count = samples_for(seconds);

        let mut scope = lock(&feed);
        for _ in 0..count {
            phase = (phase + 400.0 * TAU / SAMPLE_RATE) % TAU;
            scope.push(0, phase.sin());
            scope.push(1, if phase < TAU / 2.0 { 1.0 } else { -1.0 });
        }
        true
    });

    let ui = Rc::new(RefCell::new(UiState::<u8>::new()));
    canvas.set_draw_callback(move |ctx| {
        let area = Rect::new(0.0, 0.0, ctx.width() as f32, ctx.height() as f32);
        let mut state = ui.borrow_mut();
        let mut ui = state.frame(ctx, &theme);
        ScopeView::new(&mut lock(&scope))
            .style(style)
            .show(&mut ui, 0, area)?;
        Ok(())
    })?;

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.set_padding(0)?;
    layout.add(canvas.clone())?;
    window.set_content(layout)?;
    window.show();
    window.run()?;

    FrameScheduler::unregister_ticker(ticker);
    Ok(())
}
