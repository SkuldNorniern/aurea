//! iOS: the app's UI filling the screen, drawn on the CPU each frame. Touches come in as the
//! mouse, two-finger scrolls and pinches as the wheel and [`WindowEvent::Magnify`]; see
//! `crate::touch`.

use std::cell::{Cell, RefCell};
use std::process::exit;
use std::ptr::null;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use aurea_ffi::{NGIosHost, ng_ios_run_host, ng_ios_wake};

use crate::ffi::callbacks::event_time;
use crate::render::{CanvasId, CpuRasterizer, Renderer, SurfaceInfo, register_host_canvas};
use crate::touch::{Phase, Touches};
use crate::ui::{Theme, Ui, UiState};
use crate::{AureaResult, Modifiers, PenPhase, PenSample, PenTool, Point, WindowEvent};
use aurea_render::FrameOutcome;

/// Draws the app's UI each frame, `width` x `height` logical pixels.
type App = Box<dyn FnMut(&mut Ui<'_, u64>, f32, f32) -> AureaResult<()>>;

struct Host {
    app: Option<App>,
    theme: Theme,
    raster: CpuRasterizer,
    state: UiState<u64>,
    pixels: (u32, u32),
    scale: f32,
    insets: [f32; 4],
    touches: Touches,
}

/// Set from any thread by [`request_frame`]; the display link polls it.
static WAKE: AtomicBool = AtomicBool::new(false);

static CANVAS: LazyLock<CanvasId> = LazyLock::new(|| register_host_canvas(request_frame));

/// Hears what the user does, before the UI does.
type Listener = Box<dyn FnMut(&WindowEvent)>;

thread_local! {
    /// The screen's scale and safe area as of the last frame, apart from `HOST` so the app can read
    /// them while it draws.
    static SCREEN: Cell<(f32, [f32; 4])> = const { Cell::new((1.0, [0.0; 4])) };
    static LISTENER: RefCell<Option<Listener>> = const { RefCell::new(None) };
    static NEXT_THEME: RefCell<Option<Theme>> = const { RefCell::new(None) };
    static HOST: RefCell<Host> = RefCell::new(Host {
        app: None,
        theme: Theme::dark(),
        raster: CpuRasterizer::new(1, 1),
        state: UiState::new(),
        pixels: (0, 0),
        scale: 1.0,
        insets: [0.0; 4],
        touches: Touches::new(),
    });
}

/// Runs `app` as the whole iOS app: it draws the UI each frame, given the safe area's logical width
/// and height. UIKit owns the main thread from here, so this never returns.
pub fn run(
    theme: Theme,
    app: impl FnMut(&mut Ui<'_, u64>, f32, f32) -> AureaResult<()> + 'static,
) -> ! {
    HOST.with_borrow_mut(|host| {
        host.app = Some(Box::new(app));
        host.theme = theme;
    });
    let host = NGIosHost {
        frame,
        touch,
        pen,
        light_theme,
        wake,
        background,
        lifecycle,
    };
    exit(unsafe { ng_ios_run_host(host) })
}

/// Calls `listener` with each touch, as mouse events, and each Apple Pencil sample, as
/// [`WindowEvent::Pen`] with pressure and tilt, the way a desktop window reports a pen. It also
/// hears the app's lifecycle: focus, [`WindowEvent::Minimized`] on going to the background (save
/// there; iOS may stop the app after), and [`WindowEvent::MemoryWarning`]. The next frame is drawn
/// after each.
pub fn on_event(listener: impl FnMut(&WindowEvent) + 'static) {
    LISTENER.set(Some(Box::new(listener)));
}

fn hear(events: &[WindowEvent]) {
    LISTENER.with_borrow_mut(|listener| {
        if let Some(listener) = listener {
            events.iter().for_each(listener);
        }
    });
}

/// Asks for a frame from any thread, such as when background work finishes. The UI's own
/// animations and input already draw frames; this is for what they cannot see.
pub fn request_frame() {
    WAKE.store(true, Ordering::Release);
    // the display link pauses when idle; this starts it again on the main thread.
    unsafe { ng_ios_wake() };
}

/// The screen as a canvas id, so code that wakes a [`crate::Canvas`] with
/// [`crate::render::request_canvas_redraw`] or [`crate::render::request_canvas_redraw_at`], such
/// as background jobs, wakes the iOS host the same way. Any thread may use it.
pub fn canvas_id() -> CanvasId {
    *CANVAS
}

/// The screen's pixels per point: 2 or 3 on current devices, 1 before the host draws a frame.
pub fn scale() -> f32 {
    SCREEN.get().0
}

/// The safe area's insets, left, top, right and bottom, in points. The app draws only inside them:
/// the UI's origin is the safe area's top left, the drawing context is its size, and the theme's
/// background fills the rest of the screen.
pub fn safe_area() -> [f32; 4] {
    SCREEN.get().1
}

/// Switches the theme from the next frame.
pub fn set_theme(theme: Theme) {
    NEXT_THEME.set(Some(theme));
}

unsafe extern "C" fn light_theme() -> i32 {
    HOST.with_borrow(|host| {
        let c = host.theme.palette.background;
        i32::from(u32::from(c.r) * 299 + u32::from(c.g) * 587 + u32::from(c.b) * 114 > 128_000)
    })
}

unsafe extern "C" fn background() -> u32 {
    HOST.with_borrow(|host| {
        let c = host.theme.palette.background;
        u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b)
    })
}

/// The app going to and from the background: [`WindowEvent::Focused`] and
/// [`WindowEvent::Unfocused`] as it gains and loses the screen, [`WindowEvent::Minimized`] when it
/// goes to the background, where iOS may stop it without warning, so save then, and
/// [`WindowEvent::Restored`] when it comes back.
unsafe extern "C" fn lifecycle(state: i32) {
    let event = match state {
        0 => WindowEvent::Focused,
        1 => WindowEvent::Unfocused,
        2 => WindowEvent::Minimized,
        3 => WindowEvent::Restored,
        4 => WindowEvent::MemoryWarning,
        _ => return,
    };
    HOST.with_borrow_mut(|host| host.state.handle(&event));
    hear(&[event]);
}

unsafe extern "C" fn wake() -> i32 {
    let asked = WAKE.swap(false, Ordering::AcqRel);
    i32::from(asked || HOST.with_borrow(|host| host.touches.waiting()))
}

unsafe extern "C" fn touch(action: i32, x: f32, y: f32, id: usize, pencil: i32) {
    let phase = match action {
        0 => Phase::Down,
        1 => Phase::Up,
        2 => Phase::Move,
        3 => Phase::Cancel,
        _ => return,
    };
    let events = HOST.with_borrow_mut(|host| {
        let [left, top, ..] = host.insets;
        let at = Point::new(x - left, y - top);
        let events = host
            .touches
            .touch(phase, id, at, pencil != 0, Instant::now());
        for event in &events {
            host.state.handle(event);
        }
        events
    });
    hear(&events);
}

/// A finger held still long enough becomes the mouse; checked before each frame.
fn settle_touches() {
    let events = HOST.with_borrow_mut(|host| {
        let events = host.touches.tick(Instant::now());
        for event in &events {
            host.state.handle(event);
        }
        events
    });
    hear(&events);
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn pen(
    phase: i32,
    x: f64,
    y: f64,
    pressure: f32,
    tilt_x: f32,
    tilt_y: f32,
    twist: f32,
    age: f64,
) {
    let event = HOST.with_borrow_mut(|host| {
        let [left, top, ..] = host.insets;
        let event = WindowEvent::Pen(PenSample {
            x: x - f64::from(left),
            y: y - f64::from(top),
            pressure: pressure.clamp(0.0, 1.0),
            tilt_x: tilt_x.clamp(-90.0, 90.0),
            tilt_y: tilt_y.clamp(-90.0, 90.0),
            twist: twist.rem_euclid(360.0),
            phase: PenPhase::from_raw(u32::try_from(phase).unwrap_or(0)),
            tool: PenTool::Tip,
            barrel: false,
            modifiers: Modifiers::default(),
            time: event_time(age),
        });
        host.state.handle(&event);
        event
    });
    hear(&[event]);
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn frame(
    width: u32,
    height: u32,
    scale: f32,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    words: *mut *const u32,
    out_width: *mut u32,
    out_height: *mut u32,
) -> i32 {
    // SAFETY: the host passes valid out pointers for the length of the call.
    unsafe {
        *words = null();
        *out_width = 0;
        *out_height = 0;
    }
    settle_touches();
    HOST.with_borrow_mut(|host| {
        let scale = scale.max(1.0);
        host.insets = [left, top, right, bottom].map(|v| v.max(0.0));
        let [left, top, right, bottom] = host.insets;
        // the surface is the safe area alone, so the UI's size is the size it can use.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            clippy::cast_sign_loss
        )]
        let inside = |px: u32, before: f32, after: f32| {
            ((px as f32 - (before + after) * scale).round().max(1.0)) as u32
        };
        let pixels = (inside(width, left, right), inside(height, top, bottom));
        if host.pixels != pixels || (host.scale - scale).abs() > f32::EPSILON {
            host.pixels = pixels;
            host.scale = scale;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let logical = |px: u32| (f64::from(px) / f64::from(scale)).ceil() as u32;
            let info = SurfaceInfo::new(logical(pixels.0).max(1), logical(pixels.1).max(1), scale)
                .with_pixels(pixels.0, pixels.1);
            if let Err(e) = host.raster.configure(info) {
                log::warn!("ios surface: {e}");
                return 0;
            }
        }
        SCREEN.set((host.scale, host.insets));
        let Host {
            app: Some(app),
            theme,
            raster,
            state,
            pixels: (pw, ph),
            ..
        } = host
        else {
            return 0;
        };
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (*pw as f32 / scale, *ph as f32 / scale);
        let drawn = (|| -> AureaResult<FrameOutcome> {
            let mut ctx = raster.begin_frame()?;
            ctx.clear(theme.palette.background)?;
            let mut ui = state.frame(&mut *ctx, theme);
            app(&mut ui, w, h)?;
            drop(ui);
            drop(ctx);
            raster.end_frame()
        })();
        let outcome = match drawn {
            Ok(outcome) => outcome,
            Err(e) => {
                log::warn!("ios frame: {e}");
                return 0;
            }
        };
        // an unchanged frame keeps the layer's picture: no copy, no upload.
        if let (FrameOutcome::Presented, Some(out)) = (outcome, raster.frame_output()) {
            // SAFETY: as above; the words stay alive in the rasterizer until the next frame.
            unsafe {
                *words = out.words().as_ptr();
                *out_width = out.width;
                *out_height = out.height;
            }
        }
        if let Some(next) = NEXT_THEME.take() {
            *theme = next;
            return 1;
        }
        i32::from(state.wants_frame())
    })
}
