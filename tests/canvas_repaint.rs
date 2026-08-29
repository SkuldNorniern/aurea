//! Repaint behaviour of a live canvas inside a real window.
//!
//! These need a display and create actual native windows, so they are
//! `#[ignore]`d by default. Run them with:
//!
//! ```text
//! cargo test --test canvas_repaint -- --ignored --test-threads=1
//! ```

use aurea::elements::{Orientation, Stack};
use aurea::embed::{aurea_embed_create_canvas, aurea_embed_destroy_canvas};
use aurea::registry::elements::invoke_button_callback;
use aurea::render::{Canvas, Color, DrawingContext, Paint, PaintStyle, Rect, RendererBackend};
use aurea::{AureaResult, Container, Window, gpu_support};
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

/// A split view takes ownership of its children too. It used to attach them
/// and leave the child still believing it owned its handle, which is a double
/// free waiting for the split view to be dropped.
#[test]
#[ignore = "creates native elements; run with --ignored"]
fn a_split_view_takes_ownership_of_what_it_is_given() -> AureaResult<()> {
    use aurea::elements::{Label, SplitOrientation, SplitView};

    let _window = Window::new("split ownership", 300, 200)?;

    let mut split = SplitView::new(SplitOrientation::Horizontal)?;
    split.add(Label::new("left")?)?;
    split.add(Label::new("right")?)?;
    drop(split);
    Ok(())
}

/// Replacing a window's content destroys the old content. The platform does
/// not free it, and the old wrapper cannot: it gave up ownership when it
/// became the content.
#[test]
#[ignore = "creates native elements; run with --ignored"]
fn replacing_window_content_destroys_the_old_content() -> AureaResult<()> {
    use aurea::elements::{Element, Label};

    let mut window = Window::new("replace", 300, 200)?;

    let first = Label::new("first")?;
    let first_handle = first.handle();
    window.set_content(first)?;

    let second = Label::new("second")?;
    let second_handle = second.handle();
    window.set_content(second)?;

    assert_ne!(first_handle, second_handle);
    // The old label is gone and the window still works.
    window.set_title("replaced")?;
    Ok(())
}

/// A wrapper that forwards ownership behaves like the widget underneath. The
/// trait method has no default, so a wrapper that forgets does not compile.
#[test]
#[ignore = "creates native elements; run with --ignored"]
fn wrapper_elements_forward_ownership() -> AureaResult<()> {
    use aurea::elements::{Divider, Spacer};

    let _window = Window::new("wrappers", 300, 200)?;

    let mut stack = Stack::new(Orientation::Vertical)?;
    stack.add(Spacer::new()?)?;
    stack.add(Divider::horizontal(200)?)?;
    drop(stack);
    Ok(())
}

/// A dropped widget takes its callback registration with it, and everything the
/// application captured in it.
///
/// This lives here rather than as a unit test because it creates a real native
/// button: GTK aborts the process when asked to build a widget with no display
/// connection, which took the whole suite down on a headless Linux box.
#[test]
#[ignore = "creates a native widget; run with --ignored"]
fn dropping_a_button_drops_its_callback() -> AureaResult<()> {
    use aurea::elements::Button;
    use std::cell::Cell;
    use std::rc::Rc;

    let _window = Window::new("callback lifetime", 200, 100)?;

    let fired = Rc::new(Cell::new(0));
    let f = Rc::clone(&fired);
    let button = Button::with_callback("go", move || f.set(f.get() + 1))?;
    let id = button.callback_id();

    invoke_button_callback(id);
    assert_eq!(fired.get(), 1, "callback should run while the button lives");

    drop(button);
    invoke_button_callback(id);

    assert_eq!(fired.get(), 1, "callback should be gone with the button");
    assert_eq!(
        Rc::strong_count(&fired),
        1,
        "the registry still holds what the callback captured"
    );
    Ok(())
}

/// Creating a divider builds a real native canvas, so it needs a display.
/// As a unit test it took the whole suite down on a headless Linux box: GTK
/// aborts rather than returning an error when there is no display connection.
#[test]
#[ignore = "creates a native widget; run with --ignored"]
fn dividers_can_be_created() -> AureaResult<()> {
    use aurea::elements::Divider;

    let _window = Window::new("dividers", 200, 100)?;

    assert!(Divider::horizontal(100).is_ok());
    assert!(Divider::vertical(50).is_ok());
    Ok(())
}

/// Same for a spacer, which is a native label underneath.
#[test]
#[ignore = "creates a native widget; run with --ignored"]
fn spacers_can_be_created() -> AureaResult<()> {
    use aurea::elements::Spacer;

    let _window = Window::new("spacers", 200, 100)?;

    assert!(Spacer::new().is_ok());
    Ok(())
}

/// A text field is a native control, so what it reports differs per platform.
/// On Windows and Linux it is real and starts empty; elsewhere it may fall
/// back. Either way it needs a display to exist.
#[test]
#[ignore = "creates a native widget; run with --ignored"]
fn a_text_field_starts_empty() -> AureaResult<()> {
    use aurea::elements::TextField;

    let _window = Window::new("text field", 200, 100)?;

    let field = TextField::new()?;
    assert_eq!(field.get_content()?, "");
    Ok(())
}

/// The embedding entry point hands back a canvas for a sensible size. It needs
/// a display: with none it returns null, which is the right answer and the
/// wrong one to assert in a unit test.
#[test]
#[ignore = "creates a native widget; run with --ignored"]
fn embedding_creates_a_canvas() {
    let handle = aurea_embed_create_canvas(100, 100);

    assert!(!handle.is_null());
    aurea_embed_destroy_canvas(handle);
}

/// Ownership held over many create/drop cycles.
///
/// A double free or a leak usually survives one round trip and shows up under
/// repetition, so this exercises the shapes that transfer ownership: a loose
/// widget, an adopted one, nesting, and replacing a window's content.
#[test]
#[ignore = "creates native widgets; run with --ignored"]
fn ownership_survives_repetition() -> AureaResult<()> {
    use aurea::elements::{Button, Label};

    let mut window = Window::new("stress", 300, 200)?;

    for round in 0..200 {
        // Created and dropped without ever being adopted: this one frees
        // itself.
        drop(Label::new("loose")?);

        // Adopted, then the parent is dropped: the parent frees it.
        let mut stack = Stack::new(Orientation::Vertical)?;
        stack.add(Button::new("adopted")?)?;
        stack.add(Label::new("also adopted")?)?;
        drop(stack);

        // Nested containers, dropped from the outside in.
        let mut outer = Stack::new(Orientation::Vertical)?;
        let mut inner = Stack::new(Orientation::Horizontal)?;
        inner.add(Label::new("deep")?)?;
        outer.add(inner)?;
        drop(outer);

        // Replacing content destroys what was there before.
        let mut content = Stack::new(Orientation::Vertical)?;
        content.add(Label::new(&format!("round {round}"))?)?;
        window.set_content(content)?;
    }

    // Still usable after all that.
    window.set_title("done")?;
    Ok(())
}

/// A canvas is reference counted, so the native element must be destroyed once
/// when the last clone goes, not once per clone.
#[test]
#[ignore = "creates native widgets; run with --ignored"]
fn canvas_clones_free_once() -> AureaResult<()> {
    let _window = Window::new("canvas clones", 300, 200)?;

    for _ in 0..100 {
        let canvas = Canvas::new(64, 64, RendererBackend::Cpu)?;
        let a = canvas.clone();
        let b = canvas.clone();
        drop(canvas);
        drop(a);
        drop(b);
    }
    Ok(())
}

/// A GPU capability is not usable just because the hardware has it: the
/// backend has to be compiled in. The platform-level answer says otherwise,
/// so this narrows it.
#[test]
fn gpu_capability_follows_the_compiled_backend() {
    use aurea::{Capability, CapabilityChecker, Support};

    let checker = CapabilityChecker::new();
    let narrowed = gpu_support(&checker, Capability::Vulkan);

    if cfg!(any(feature = "zengpu", feature = "wgpu")) {
        assert_eq!(narrowed, checker.support(Capability::Vulkan));
    } else {
        assert!(
            matches!(narrowed, Support::Unimplemented | Support::Unavailable),
            "no GPU backend is compiled in, so it cannot be usable: {narrowed:?}"
        );
    }
}

/// Narrowing applies to GPU capabilities only; everything else is unchanged.
#[test]
fn other_capabilities_are_not_narrowed() {
    use aurea::{Capability, CapabilityChecker};

    let checker = CapabilityChecker::new();
    for capability in [
        Capability::MenuBar,
        Capability::Clipboard,
        Capability::FileDialogs,
        Capability::KeyboardInput,
    ] {
        assert_eq!(
            gpu_support(&checker, capability),
            checker.support(capability),
            "{capability:?} is not a GPU capability"
        );
    }
}

/// A menu belongs to the window it is attached to. It used to be handed back
/// owned, so dropping it destroyed the menu of a window still showing it.
#[test]
#[ignore = "creates a native window; run with --ignored"]
fn a_menu_bar_belongs_to_its_window() -> AureaResult<()> {
    use aurea::Capability;

    let mut window = Window::new("menus", 300, 200)?;
    if !window.capabilities().has(Capability::MenuBar) {
        return Ok(());
    }

    {
        let menu_bar = window.create_menu_bar()?;
        let mut file = menu_bar.add_submenu("File")?;
        file.add_item("New", || {})?;
    }

    // The borrow is over, and the menu is still the window's: asking again
    // replaces it rather than finding nothing there.
    {
        let menu_bar = window.create_menu_bar()?;
        menu_bar.add_submenu("Edit")?;
    }

    // Dropping the window takes the menu with it, which must not fault.
    drop(window);
    Ok(())
}
