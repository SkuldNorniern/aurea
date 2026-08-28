//! Repaint behaviour of a live canvas inside a real window.
//!
//! These need a display and create actual native windows, so they are
//! `#[ignore]`d by default. Run them with:
//!
//! ```text
//! cargo test --test canvas_repaint -- --ignored --test-threads=1
//! ```

use aurea::elements::{Orientation, Stack};
use aurea::render::{Canvas, Color, DrawingContext, Paint, PaintStyle, Rect, RendererBackend};
use aurea::{AureaResult, Container, Window};
use aurea_render::CURRENT_BUFFER;
use aurea_runtime::FrameScheduler;
use std::slice::from_raw_parts;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const BG: Color = Color {
    r: 240,
    g: 240,
    b: 240,
    a: 255,
};

fn paint_a_square(ctx: &mut dyn DrawingContext) -> AureaResult<()> {
    let fill = Paint::new()
        .color(Color::rgb(100, 150, 200))
        .style(PaintStyle::Fill);
    ctx.draw_rect(Rect::new(10.0, 10.0, 100.0, 100.0), &fill)
}

/// Pixels in the published buffer that are not the background colour.
fn drawn_pixels() -> usize {
    CURRENT_BUFFER.with(|buf| match *buf.borrow() {
        Some((ptr, size, _, _)) if !ptr.is_null() && size > 0 => {
            let bg = (u32::from(BG.a) << 24)
                | (u32::from(BG.r) << 16)
                | (u32::from(BG.g) << 8)
                | u32::from(BG.b);
            let bytes = unsafe { from_raw_parts(ptr, size) };
            bytes
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .filter(|&p| p != bg)
                .count()
        }
        _ => 0,
    })
}

fn window_with_canvas(title: &str) -> AureaResult<(Window, Canvas)> {
    let mut window = Window::new(title, 400, 300)?;
    let canvas = Canvas::new(400, 300, RendererBackend::Cpu)?;
    canvas.set_background_color(BG);
    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas.clone())?;
    window.set_content(layout)?;
    window.show();
    // Let the canvas settle at the size the window actually gave it.
    window.poll_events();
    window.process_frames()?;
    Ok((window, canvas))
}

/// A canvas drawn through `set_draw_callback` keeps its content across the
/// resize that happens when it is placed in a window.
#[test]
#[ignore = "creates a native window; run with --ignored"]
fn retained_draw_survives_the_layout_resize() -> AureaResult<()> {
    let mut window = Window::new("retained", 400, 300)?;
    let canvas = Canvas::new(400, 300, RendererBackend::Cpu)?;
    canvas.set_background_color(BG);
    canvas.set_draw_callback(paint_a_square)?;

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.add(canvas.clone())?;
    window.set_content(layout)?;
    window.show();

    for _ in 0..3 {
        window.poll_events();
        window.process_frames()?;
    }

    assert!(
        drawn_pixels() > 1000,
        "retained canvas went blank after the window resized it"
    );
    Ok(())
}

/// `invalidate_all` must not erase what `Canvas::draw` put in the buffer.
/// There is no callback to re-run, so the pixels are all there is.
#[test]
#[ignore = "creates a native window; run with --ignored"]
fn invalidate_keeps_immediate_mode_pixels() -> AureaResult<()> {
    let (window, mut canvas) = window_with_canvas("immediate")?;

    canvas.draw(paint_a_square)?;
    let before = drawn_pixels();
    assert!(before > 1000, "immediate draw produced nothing");

    canvas.invalidate_all();
    for _ in 0..3 {
        window.poll_events();
        window.process_frames()?;
    }

    assert_eq!(
        drawn_pixels(),
        before,
        "invalidate_all wiped an immediate-mode canvas"
    );
    Ok(())
}

/// A ticker that feeds a retained draw callback has to mark the canvas dirty,
/// or the callback runs once and the canvas sits there looking frozen.
/// `on_frame` is what ties the two together.
#[test]
#[ignore = "creates a native window; run with --ignored"]
fn on_frame_keeps_the_draw_callback_running() -> AureaResult<()> {
    let (window, canvas) = window_with_canvas("on_frame")?;

    let draws = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&draws);
    canvas.set_draw_callback(move |ctx| {
        counter.fetch_add(1, Ordering::Relaxed);
        // Something different every frame, so the damage diff cannot skip it.
        let n = counter.load(Ordering::Relaxed) % 40;
        let fill = Paint::new()
            .color(Color::rgb(100, 150, 200))
            .style(PaintStyle::Fill);
        ctx.draw_rect(Rect::new(n as f32, 10.0, 20.0, 20.0), &fill)
    })?;

    let ticks = Arc::new(AtomicUsize::new(0));
    let tick_counter = Arc::clone(&ticks);
    let ticker = canvas.on_frame(move |_| {
        tick_counter.fetch_add(1, Ordering::Relaxed);
        true
    });

    for _ in 0..4 {
        window.poll_events();
        window.process_frames()?;
    }
    FrameScheduler::unregister_ticker(ticker);

    assert!(
        ticks.load(Ordering::Relaxed) >= 3,
        "the ticker should run each frame"
    );
    assert!(
        draws.load(Ordering::Relaxed) >= 3,
        "the canvas redrew {} time(s); a ticker alone leaves it at 1",
        draws.load(Ordering::Relaxed)
    );
    Ok(())
}

/// Returning false from `on_frame` stops it.
#[test]
#[ignore = "creates a native window; run with --ignored"]
fn on_frame_stops_when_it_returns_false() -> AureaResult<()> {
    let (window, canvas) = window_with_canvas("on_frame stop")?;
    canvas.set_draw_callback(|_| Ok(()))?;

    let ticks = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&ticks);
    canvas.on_frame(move |_| {
        counter.fetch_add(1, Ordering::Relaxed);
        false
    });

    for _ in 0..4 {
        window.poll_events();
        window.process_frames()?;
    }

    assert_eq!(
        ticks.load(Ordering::Relaxed),
        1,
        "it should run once and stop"
    );
    Ok(())
}

/// A widget frees its native element when it is dropped, and stops doing so
/// once a container has taken it: the platform frees a container's children
/// along with the container, so freeing it again would be a double free.
#[test]
#[ignore = "creates native elements; run with --ignored"]
fn a_container_takes_ownership_of_what_it_is_given() -> AureaResult<()> {
    use aurea::elements::{Button, Element, Label};

    let _window = Window::new("ownership", 200, 100)?;

    let loose = Label::new("loose")?;
    assert!(!loose.handle().is_null());
    // Nothing has adopted it, so dropping it destroys the native label.
    drop(loose);

    let mut stack = Stack::new(Orientation::Vertical)?;
    let adopted = Button::new("adopted")?;
    let handle = adopted.handle();
    stack.add(adopted)?;

    // Still a live native element: the stack holds it now.
    assert!(!stack.handle().is_null());
    assert!(!handle.is_null());

    // Dropping the stack frees the stack and its child together. Doing it
    // twice is what the handover prevents; this must not fault.
    drop(stack);
    Ok(())
}

/// Dropping a canvas destroys its native object. It used to unregister from
/// the scheduler and leave the platform element behind.
#[test]
#[ignore = "creates native elements; run with --ignored"]
fn dropping_a_canvas_destroys_the_native_element() -> AureaResult<()> {
    use aurea::elements::Element;

    let _window = Window::new("canvas ownership", 200, 100)?;

    let canvas = Canvas::new(100, 100, RendererBackend::Cpu)?;
    let handle = canvas.handle();
    assert!(!handle.is_null());

    // Clones share one native canvas and one cleanup, so it is freed once.
    let clone = canvas.clone();
    drop(canvas);
    drop(clone);
    Ok(())
}
