//! Oscilloscope view with two channels and a trigger.
//!
//! Run with: cargo run --example scope
//!
//! Feeds a sine and a square into a `Scope` from a frame ticker and draws it
//! through the canvas. The trigger keeps the trace still; turn it off and the
//! signal slides across the screen instead.

use aurea::elements::{Orientation, Stack};
use aurea::render::graph::{Channel, GridStyle, Margin, Scope, Stroke, Timebase, Trigger};
use aurea::render::{Canvas, Color, Rect, RendererBackend};
use aurea::{AureaResult, Container, Window};
use aurea_foundation::lock;
use aurea_runtime::FrameScheduler;
use std::f64::consts::TAU;
use std::sync::{Arc, Mutex};

const W: u32 = 900;
const H: u32 = 500;
const SAMPLE_RATE: f64 = 20_000.0;
const CAPTURE: usize = 8192;

/// How many samples cover `seconds`, capped so one long frame cannot ask for
/// an unbounded burst.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn samples_for(seconds: f64) -> usize {
    (seconds * SAMPLE_RATE).clamp(0.0, 4096.0) as usize
}

fn main() -> AureaResult<()> {
    let mut window = Window::new("Scope", W as i32, H as i32)?;
    let canvas = Canvas::new(W, H, RendererBackend::Cpu)?;
    canvas.set_background_color(Color::rgb(16, 18, 22));

    let mut scope = Scope::new(CAPTURE);
    scope.timebase = Timebase::new(0.0005, SAMPLE_RATE);
    scope.vertical_divisions = 4;
    scope.trigger = Trigger::rising(0.0).with_position(0.1);

    // A graticule wants a faint, even grid and room for the readout along the
    // top, not the default plot furniture.
    scope.style.plot_background = Some(Color::rgb(8, 10, 13));
    scope.style.margin = Margin::new(56.0, 16.0, 24.0, 30.0);
    scope.style.grid = GridStyle {
        major: Stroke::new(Color::rgba(120, 200, 255, 26), 1.0),
        minor: Stroke::new(Color::rgba(120, 200, 255, 12), 1.0),
        zero: Some(Stroke::new(Color::rgba(180, 220, 255, 70), 1.0)),
        show_vertical: true,
        show_horizontal: true,
    };
    scope.style.border = Some(Stroke::new(Color::rgba(120, 200, 255, 70), 1.0));
    scope.style.x_axis.minor_tick_length = 3.0;
    scope.style.y_axis.minor_tick_length = 3.0;

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

    // The signal generator. `phase` is where the waveform got to last frame, so
    // the two channels stay continuous across frames instead of restarting.
    //
    // on_frame rather than a plain ticker: a retained draw callback only runs
    // when the canvas is dirty, so feeding samples from a ticker that does not
    // redraw paints one frame and then sits there looking frozen.
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

    let draw = Arc::clone(&scope);
    canvas.set_draw_callback(move |ctx| {
        let mut scope = lock(&draw);
        let area = Rect::new(0.0, 0.0, ctx.width() as f32, ctx.height() as f32);
        scope.draw(ctx, area)
    })?;

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas.clone())?;
    window.set_content(layout)?;
    window.show();
    window.run()?;

    FrameScheduler::unregister_ticker(ticker);
    Ok(())
}
