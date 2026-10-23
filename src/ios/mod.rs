//! iOS: the app's UI filling the screen, drawn on the GPU with the `zengpu` feature and on the CPU
//! otherwise, or when the GPU cannot start. Touches come in as the
//! mouse, two-finger scrolls and pinches as the wheel and [`WindowEvent::Magnify`]; see
//! `crate::touch`.

mod documents;

pub use documents::{export_files, pick_files};

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, c_char, c_int};
use std::process::exit;
use std::ptr::null;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[cfg(feature = "zengpu")]
use aurea_ffi::ng_ios_gpu_view;
use aurea_ffi::{
    NGIosHost, ng_ios_drop_gpu_view, ng_ios_keyboard_height, ng_ios_run_host, ng_ios_set_text_area,
    ng_ios_show_keyboard, ng_ios_wake,
};

#[cfg(not(feature = "zengpu"))]
use crate::AureaError;
use crate::ffi::callbacks::event_time;
use crate::render::{CanvasId, CpuRasterizer, Renderer, SurfaceInfo, register_host_canvas};
use crate::touch::{Phase, Touches};
use crate::ui::{Theme, Ui, UiState};
use crate::{
    AureaResult, KeyCode, Modifiers, PenPhase, PenSample, PenTool, Point, Rect, WindowEvent,
};
#[cfg(feature = "zengpu")]
use crate::{
    platform::{handles::NativeWindowHandle, zengpu::window_handles},
    render::{ZenGpuContext, ZenGpuRenderer},
};
use aurea_render::FrameOutcome;
#[cfg(feature = "zengpu")]
use std::sync::Arc;

/// Draws the app's UI each frame, `width` x `height` logical pixels.
type App = Box<dyn FnMut(&mut Ui<'_, u64>, f32, f32) -> AureaResult<()>>;

struct Host {
    app: Option<App>,
    theme: Theme,
    raster: Box<dyn Renderer>,
    /// Whether the GPU was tried yet; it is, on the first frame.
    gpu_tried: bool,
    state: UiState<u64>,
    pixels: (u32, u32),
    scale: f32,
    insets: [f32; 4],
    touches: Touches,
}

impl Host {
    /// Sizes the surface to the safe area of a `width` x `height` pixel screen, trying the GPU on
    /// the first call; false when the surface could not take the size.
    fn fit(&mut self, width: u32, height: u32, scale: f32, insets: [f32; 4]) -> bool {
        self.insets = insets.map(|v| v.max(0.0));
        let [left, top, right, bottom] = self.insets;
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
        if self.pixels == pixels && (self.scale - scale).abs() <= f32::EPSILON {
            return true;
        }
        self.pixels = pixels;
        self.scale = scale;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let logical = |px: u32| (f64::from(px) / f64::from(scale)).ceil() as u32;
        let info = SurfaceInfo::new(logical(pixels.0).max(1), logical(pixels.1).max(1), scale)
            .with_pixels(pixels.0, pixels.1);
        if !self.gpu_tried {
            self.gpu_tried = true;
            self.try_gpu(&info);
        }
        if let Err(e) = self.raster.configure(info) {
            log::warn!("ios surface: {e}");
            return false;
        }
        true
    }

    fn try_gpu(&mut self, info: &SurfaceInfo) {
        if !GPU_WANTED.load(Ordering::Relaxed) || !cfg!(feature = "zengpu") {
            return;
        }
        match gpu_renderer(info) {
            Ok(renderer) => self.raster = renderer,
            Err(e) => {
                log::warn!("ios: no gpu, drawing on the cpu: {e}");
                // SAFETY: on the main thread, as frames are.
                unsafe { ng_ios_drop_gpu_view() };
            }
        }
    }
}

/// Whether the host may draw on the GPU; see [`use_gpu`].
static GPU_WANTED: AtomicBool = AtomicBool::new(true);

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
    #[cfg(feature = "zengpu")]
    static GPU: RefCell<Option<Arc<ZenGpuContext>>> = const { RefCell::new(None) };
    static HOST: RefCell<Host> = RefCell::new(Host {
        app: None,
        theme: Theme::dark(),
        raster: Box::new(CpuRasterizer::new(1, 1)),
        gpu_tried: false,
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
        key,
        preedit,
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

/// Whether to draw on the GPU, through ZenGPU's Metal backend; call before [`run`]. On by default
/// with the `zengpu` feature, which it needs; the CPU draws when it is off, or when the GPU cannot
/// start.
pub fn use_gpu(on: bool) {
    GPU_WANTED.store(on, Ordering::Relaxed);
}

/// The GPU the host draws with, once it has drawn a frame on it, for the app's own textures and
/// renderers to share.
#[cfg(feature = "zengpu")]
pub fn gpu_context() -> Option<Arc<ZenGpuContext>> {
    GPU.with_borrow(Clone::clone)
}

/// A renderer on the host's GPU view, the safe area's size.
#[cfg(feature = "zengpu")]
fn gpu_renderer(info: &SurfaceInfo) -> AureaResult<Box<dyn Renderer>> {
    // SAFETY: called on the main thread while the host view exists.
    let ui_view = unsafe { ng_ios_gpu_view() };
    let handles = window_handles(&NativeWindowHandle::IOS { ui_view })?;
    let context = Arc::new(ZenGpuContext::new()?);
    let renderer = ZenGpuRenderer::with_context(
        &handles,
        Arc::clone(&context),
        info.width,
        info.height,
        info.scale_factor,
    )?;
    GPU.set(Some(context));
    log::info!("ios: drawing with the gpu");
    Ok(Box::new(renderer))
}

#[cfg(not(feature = "zengpu"))]
fn gpu_renderer(_: &SurfaceInfo) -> AureaResult<Box<dyn Renderer>> {
    Err(AureaError::RenderingFailed)
}

/// Shows the on-screen keyboard, or hides it; call on the main thread, such as from the UI when a
/// text field takes or loses focus. What it types comes as [`WindowEvent::TextInput`], delete and
/// return as [`KeyCode::Backspace`] and [`KeyCode::Enter`] presses, and what an input method such
/// as Korean or Japanese is still composing as [`WindowEvent::ImePreedit`].
pub fn show_keyboard(show: bool) {
    // SAFETY: on the main thread, as documented.
    unsafe { ng_ios_show_keyboard(c_int::from(show)) };
}

/// Where text is being typed, in points from the safe area's top left, so input methods show
/// their candidates beside it; call on the main thread when the caret moves.
pub fn set_text_area(area: Rect) {
    // SAFETY: on the main thread, as documented.
    unsafe { ng_ios_set_text_area(area.x, area.y, area.width, area.height) };
}

/// How much of the safe area the on-screen keyboard covers from the bottom, in points; 0 when it
/// is down. Keep what is being typed into above it.
pub fn keyboard_height() -> f32 {
    // SAFETY: reads host state on the main thread, where the app draws.
    let covered = unsafe { ng_ios_keyboard_height() };
    (covered - safe_area()[3]).max(0.0)
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

/// A key, and the text a press types: from a hardware keyboard, or text alone from the on-screen
/// one.
unsafe extern "C" fn key(code: u32, pressed: i32, modifiers: u32, text: *const c_char) {
    // SAFETY: the host passes a NUL-terminated string that lives for the call, or null.
    let text = (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_string_lossy());
    let text = text.filter(|t| !t.is_empty());
    let key = KeyCode::from_raw(code);
    let mut events = Vec::new();
    // the on-screen keyboard types text with no key.
    if !matches!(key, KeyCode::Unknown(_)) || text.is_none() {
        events.push(WindowEvent::KeyInput {
            key,
            pressed: pressed != 0,
            modifiers: Modifiers::from_bits(modifiers),
        });
    }
    if let Some(text) = text {
        events.push(WindowEvent::TextInput {
            text: text.into_owned(),
        });
    }
    HOST.with_borrow_mut(|host| {
        for event in &events {
            host.state.handle(event);
        }
    });
    hear(&events);
}

/// What an input method is composing, before it becomes text.
unsafe extern "C" fn preedit(text: *const c_char, cursor: c_int) {
    // SAFETY: the host passes a NUL-terminated string that lives for the call, or null.
    let text = if text.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    };
    let event = WindowEvent::ImePreedit {
        text,
        cursor: usize::try_from(cursor).ok(),
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
    damage: *mut u32,
) -> i32 {
    // SAFETY: the host passes valid out pointers for the length of the call, `damage` four words.
    unsafe {
        *words = null();
        *out_width = 0;
        *out_height = 0;
        damage.write_bytes(0, 4);
    }
    settle_touches();
    HOST.with_borrow_mut(|host| {
        let scale = scale.max(1.0);
        if !host.fit(width, height, scale, [left, top, right, bottom]) {
            return 0;
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
            let changed = changed_pixels(raster.last_frame_damage(), out.width, out.height);
            unsafe {
                *words = out.words().as_ptr();
                *out_width = out.width;
                *out_height = out.height;
                for (i, v) in changed.into_iter().enumerate() {
                    *damage.add(i) = v;
                }
            }
        }
        if let Some(next) = NEXT_THEME.take() {
            *theme = next;
            return 1;
        }
        i32::from(state.wants_frame())
    })
}

/// The part of a `width` x `height` frame that changed, as x, y, width and height in whole pixels;
/// all of it when the rasterizer does not say.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn changed_pixels(damage: Option<Rect>, width: u32, height: u32) -> [u32; 4] {
    let Some(r) = damage else {
        return [0, 0, width, height];
    };
    let x0 = (r.x.floor().max(0.0) as u32).min(width);
    let y0 = (r.y.floor().max(0.0) as u32).min(height);
    let x1 = ((r.x + r.width).ceil().max(0.0) as u32).min(width);
    let y1 = ((r.y + r.height).ceil().max(0.0) as u32).min(height);
    [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
}
