//! One Aurea UI on an Android `SurfaceView`: frames drawn on the GPU when built with `zengpu`,
//! otherwise on the CPU and posted to its window; fingers and the pen fed in as the pointer.

use std::cell::RefCell;
#[cfg(feature = "zengpu")]
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

use jni::JNIEnv;
use jni::objects::JObject;

use super::window::NativeWindow;
use crate::ffi::callbacks::event_time;
#[cfg(feature = "zengpu")]
use crate::gpu_ready;
use crate::render::{CpuRasterizer, Renderer, SurfaceInfo};
use crate::touch::{Phase, Touches};
use crate::ui::{Theme, Ui, UiState};
use crate::wake::wake_at;
use crate::{
    AureaResult, KeyCode, Modifiers, MouseButtons, PenPhase, PenSample, PenTool, Point, Rect,
    WindowEvent,
};
use aurea_render::FrameOutcome;
#[cfg(feature = "zengpu")]
use aurea_render::{ZenGpuContext, ZenGpuRenderer};

#[cfg(feature = "zengpu")]
use crate::platform::{handles::NativeWindowHandle, zengpu::window_handles};

/// Draws the app's UI each frame, `width` x `height` logical pixels.
pub type App = Box<dyn FnMut(&mut Ui<'_, u64>, f32, f32) -> AureaResult<()>>;

struct Host {
    app: Option<App>,
    theme: Theme,
    window: Option<NativeWindow>,
    raster: CpuRasterizer,
    #[cfg(feature = "zengpu")]
    gpu: Gpu,
    state: UiState<u64>,
    scale: f32,
    logical: (u32, u32),
    /// System bar insets in buffer pixels; they can arrive before the surface and its scale.
    insets: [f32; 4],
    touches: Touches,
}

/// Hears every event the host feeds the UI.
pub(super) type Listener = Box<dyn FnMut(&WindowEvent)>;

/// The scale, then the logical insets left, top, right and bottom, as f32 bits, for any thread.
static SCREEN: [AtomicU32; 5] = [
    AtomicU32::new(0x3F80_0000),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];
/// A theme to switch to from the next frame, from any thread.
static NEXT_THEME: Mutex<Option<Theme>> = Mutex::new(None);
/// Whether the app wants the GPU, when built with `zengpu`.
pub(super) static GPU_WANTED: AtomicBool = AtomicBool::new(true);

/// The GPU renderer for the current window, and the device it shares across windows.
#[cfg(feature = "zengpu")]
#[derive(Default)]
struct Gpu {
    context: Option<Arc<ZenGpuContext>>,
    renderer: Option<ZenGpuRenderer>,
    /// The device could not be opened; frames stay on the CPU.
    failed: bool,
}

#[cfg(feature = "zengpu")]
impl Gpu {
    /// A renderer on `window`, or `None` to draw on the CPU.
    fn attach(&mut self, window: &NativeWindow, info: &SurfaceInfo) -> Option<ZenGpuRenderer> {
        if self.failed || !GPU_WANTED.load(Ordering::Relaxed) {
            return None;
        }
        let native = NativeWindowHandle::Android {
            native_window: window.raw(),
        };
        let made = (|| {
            let handles = window_handles(&native)?;
            let context = match &self.context {
                Some(context) => Arc::clone(context),
                None => {
                    let made = Arc::new(ZenGpuContext::new()?);
                    gpu_ready::made(&made);
                    Arc::clone(self.context.insert(made))
                }
            };
            let mut renderer = ZenGpuRenderer::with_context(
                &handles,
                context,
                info.width,
                info.height,
                info.scale_factor,
            )?;
            renderer.configure(info.clone())?;
            log::info!("android: drawing with the gpu");
            AureaResult::Ok(renderer)
        })();
        made.inspect_err(|e| {
            log::warn!("android gpu, drawing on the cpu: {e}");
            self.failed = true;
        })
        .ok()
    }
}

impl Host {
    /// Left, top, right, bottom insets in logical pixels.
    fn logical_insets(&self) -> [f32; 4] {
        self.insets.map(|v| v / self.scale)
    }

    /// Publishes the scale and insets for [`scale`] and [`safe_area`].
    fn publish(&self) {
        let [left, top, right, bottom] = self.logical_insets();
        for (slot, v) in SCREEN.iter().zip([self.scale, left, top, right, bottom]) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    /// A point in buffer pixels as logical pixels from the safe area's top left.
    fn logical(&self, x: f32, y: f32) -> Point {
        let [left, top, ..] = self.logical_insets();
        Point::new(x / self.scale - left, y / self.scale - top)
    }
}

/// The screen's pixels per logical pixel.
pub(super) fn scale() -> f32 {
    f32::from_bits(SCREEN[0].load(Ordering::Relaxed))
}

/// The safe area's insets in logical pixels.
pub(super) fn safe_area() -> [f32; 4] {
    [1, 2, 3, 4].map(|i| f32::from_bits(SCREEN[i].load(Ordering::Relaxed)))
}

/// The GPU the host draws with, once it has one.
#[cfg(feature = "zengpu")]
pub(super) fn gpu_context() -> Option<Arc<ZenGpuContext>> {
    gpu_ready::current()
}

thread_local! {
    static LISTENERS: RefCell<Vec<Listener>> = const { RefCell::new(Vec::new()) };
    static HOST: RefCell<Host> = RefCell::new(Host {
        app: None,
        theme: Theme::dark(),
        window: None,
        raster: CpuRasterizer::new(1, 1),
        #[cfg(feature = "zengpu")]
        gpu: Gpu::default(),
        state: UiState::new(),
        scale: 1.0,
        logical: (1, 1),
        insets: [0.0; 4],
        touches: Touches::new(),
    });
}

pub(super) fn add_listener(listener: Listener) {
    LISTENERS.with_borrow_mut(|all| all.push(listener));
}

/// Feeds `events` to the UI, then to the app's listeners.
fn feed(events: &[WindowEvent]) {
    if events.is_empty() {
        return;
    }
    HOST.with_borrow_mut(|host| {
        for event in events {
            host.state.handle(event);
        }
    });
    // taken out while they run, so one can add another; that one hears from the next event on.
    let mut running = LISTENERS.take();
    for event in events {
        for listener in &mut running {
            listener(event);
        }
    }
    LISTENERS.with_borrow_mut(|added| {
        running.append(added);
        *added = running;
    });
}

/// The app's lifecycle: focus, the background, and memory running low.
pub(super) fn lifecycle(state: i32) {
    let event = match state {
        0 => WindowEvent::Focused,
        1 => WindowEvent::Unfocused,
        2 => WindowEvent::Minimized,
        3 => WindowEvent::Restored,
        4 => WindowEvent::MemoryWarning,
        _ => return,
    };
    feed(&[event]);
}

pub(super) fn set_app(app: App, theme: Theme) {
    HOST.with_borrow_mut(|host| {
        host.app = Some(app);
        host.theme = theme;
    });
}

pub(super) fn set_theme(theme: Theme) {
    if let Ok(mut next) = NEXT_THEME.lock() {
        *next = Some(theme);
    }
    super::request_frame();
}

pub(super) fn surface_changed(
    env: &JNIEnv,
    surface: &JObject,
    width: u32,
    height: u32,
    scale: f32,
) {
    HOST.with_borrow_mut(|host| {
        #[cfg(feature = "zengpu")]
        {
            host.gpu.renderer = None;
        }
        host.window = None;
        host.window = NativeWindow::from_surface(env, surface, width, height);
        host.scale = scale.max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let logical = |px: u32| (f64::from(px) / f64::from(host.scale)).ceil() as u32;
        host.logical = (logical(width).max(1), logical(height).max(1));
        host.publish();
        let info =
            SurfaceInfo::new(host.logical.0, host.logical.1, host.scale).with_pixels(width, height);
        #[cfg(feature = "zengpu")]
        if let Some(window) = &host.window {
            host.gpu.renderer = host.gpu.attach(window, &info);
            if host.gpu.renderer.is_some() {
                return;
            }
        }
        if let Err(e) = host.raster.configure(info) {
            log::warn!("android surface: {e}");
        }
    });
}

pub(super) fn set_insets(left: f32, top: f32, right: f32, bottom: f32) {
    HOST.with_borrow_mut(|host| {
        host.insets = [left, top, right, bottom].map(|v| v.max(0.0));
        host.publish();
    });
}

pub(super) fn light_theme() -> bool {
    HOST.with_borrow(|host| {
        let c = host.theme.palette.background;
        u32::from(c.r) * 299 + u32::from(c.g) * 587 + u32::from(c.b) * 114 > 128_000
    })
}

pub(super) fn surface_destroyed() {
    HOST.with_borrow_mut(|host| {
        #[cfg(feature = "zengpu")]
        {
            host.gpu.renderer = None;
        }
        host.window = None;
    });
}

/// A key from the keyboard, as Android's key code and meta state.
pub(super) fn key(code: i32, pressed: bool, meta: i32) {
    let key = key_code(code);
    if matches!(key, KeyCode::Unknown(_)) {
        return;
    }
    feed(&[WindowEvent::KeyInput {
        key,
        pressed,
        modifiers: modifiers(meta),
    }]);
}

/// Text the keyboard typed.
pub(super) fn text(text: String) {
    if !text.is_empty() {
        feed(&[WindowEvent::TextInput { text }]);
    }
}

/// What an input method is still composing; empty when it is done or called off.
pub(super) fn preedit(text: String, cursor: i32) {
    let cursor = usize::try_from(cursor).ok();
    feed(&[WindowEvent::ImePreedit { text, cursor }]);
}

/// Android's `KeyEvent` meta state as modifiers.
fn modifiers(meta: i32) -> Modifiers {
    Modifiers {
        shift: meta & 0x1 != 0,
        alt: meta & 0x2 != 0,
        ctrl: meta & 0x1000 != 0,
        meta: meta & 0x1_0000 != 0,
    }
}

/// Android's `KeyEvent` key codes as Aurea's.
fn key_code(code: i32) -> KeyCode {
    let raw = match code {
        // letters, then digits, as Aurea's raw codes number them
        29..=54 => code - 29,
        7..=16 => code - 7 + 26,
        62 => 36,
        66 | 160 => 37,
        111 => 38,
        61 => 39,
        67 => 40,
        112 => 41,
        124 => 42,
        122 => 43,
        123 => 44,
        92 => 45,
        93 => 46,
        19 => 47,
        20 => 48,
        21 => 49,
        22 => 50,
        131..=142 => code - 131 + 51,
        59 | 60 => 63,
        113 | 114 => 64,
        57 | 58 => 65,
        117 | 118 => 66,
        69 => 67,
        70 => 68,
        71 => 69,
        72 => 70,
        73 => 71,
        74 => 72,
        75 => 73,
        68 => 74,
        55 => 75,
        56 => 76,
        76 => 77,
        _ => return KeyCode::Unknown(u32::try_from(code).unwrap_or(u32::MAX)),
    };
    KeyCode::from_raw(u32::try_from(raw).unwrap_or(u32::MAX))
}

/// Android's `MotionEvent` tool types.
const TOOL_STYLUS: i32 = 2;
const TOOL_MOUSE: i32 = 3;
const TOOL_ERASER: i32 = 4;

/// Where a reading falls, as `AureaActivity` sends it.
const HOVER: i32 = 4;
const EXIT: i32 = 5;

/// One pointer reading from `AureaActivity`, at `x`, `y` buffer pixels. Fingers go through
/// [`Touches`], so two fingers scroll and pinch; a pen is also a [`WindowEvent::Pen`] with its
/// pressure and lean, and presses at once.
#[allow(clippy::too_many_arguments)]
pub(super) fn pointer(
    phase: i32,
    id: i32,
    tool: i32,
    (x, y): (f32, f32),
    pressure: f32,
    (tilt, orientation): (f32, f32),
    barrel: bool,
    age: f64,
) {
    let pen = tool == TOOL_STYLUS || tool == TOOL_ERASER;
    let (at, touched) = HOST.with_borrow_mut(|host| {
        let at = host.logical(x, y);
        let touch = match phase {
            0 => Phase::Down,
            1 => Phase::Up,
            2 => Phase::Move,
            3 => Phase::Cancel,
            _ => return (at, Vec::new()),
        };
        // a mouse presses at once, like a pen.
        let at_once = pen || tool == TOOL_MOUSE;
        let id = usize::try_from(id).unwrap_or(0);
        (
            at,
            host.touches.touch(touch, id, at, at_once, Instant::now()),
        )
    });
    let mut events = Vec::with_capacity(touched.len() + 1);
    if pen {
        let lean = (tilt, orientation);
        events.push(WindowEvent::Pen(pen_sample(
            phase, tool, at, pressure, lean, barrel, age,
        )));
    }
    match phase {
        // a pen above the screen moves the pointer without pressing.
        HOVER => events.push(WindowEvent::MouseMove {
            x: f64::from(at.x),
            y: f64::from(at.y),
            buttons: MouseButtons::default(),
            modifiers: Modifiers::default(),
            time: event_time(age),
        }),
        EXIT => events.push(WindowEvent::MouseExited),
        _ => {}
    }
    events.extend(touched);
    feed(&events);
}

/// A pen reading as a [`PenSample`]: Android gives the tilt from upright and the direction the pen
/// leans, which become the lean toward the right and toward the user.
fn pen_sample(
    phase: i32,
    tool: i32,
    at: Point,
    pressure: f32,
    (tilt, orientation): (f32, f32),
    barrel: bool,
    age: f64,
) -> PenSample {
    let lean = tilt.clamp(0.0, 1.5).tan();
    PenSample {
        x: f64::from(at.x),
        y: f64::from(at.y),
        pressure: if phase >= HOVER {
            0.0
        } else {
            pressure.clamp(0.0, 1.0)
        },
        tilt_x: (lean * orientation.sin()).atan().to_degrees(),
        tilt_y: (-lean * orientation.cos()).atan().to_degrees(),
        twist: 0.0,
        phase: match phase {
            0 => PenPhase::Down,
            1 | 3 => PenPhase::Up,
            2 => PenPhase::Move,
            _ => PenPhase::Hover,
        },
        tool: if tool == TOOL_ERASER {
            PenTool::Eraser
        } else {
            PenTool::Tip
        },
        barrel,
        modifiers: Modifiers::default(),
        time: event_time(age),
    }
}

pub(super) fn frame() -> bool {
    // a finger held still long enough becomes the mouse.
    let settled = HOST.with_borrow_mut(|host| host.touches.tick(Instant::now()));
    feed(&settled);
    HOST.with_borrow_mut(|host| {
        let insets = host.logical_insets();
        let Host {
            app: Some(app),
            theme,
            window: Some(window),
            raster,
            state,
            logical,
            ..
        } = host
        else {
            return false;
        };
        let size = (logical.0 as f32, logical.1 as f32);
        #[cfg(feature = "zengpu")]
        if let Some(gpu) = &mut host.gpu.renderer {
            if let Err(e) = draw(gpu, app, theme, state, size, insets) {
                log::warn!("android frame: {e}");
                return false;
            }
            return next_frame(theme, state, host.touches.waiting());
        }
        match draw(raster, app, theme, state, size, insets) {
            Err(e) => {
                log::warn!("android frame: {e}");
                return false;
            }
            Ok(FrameOutcome::Unchanged) => {}
            Ok(_) => {
                let damage = raster.last_frame_damage();
                let posted = raster
                    .frame_output()
                    .is_some_and(|out| window.post(out.words(), out.width, out.height, damage));
                // the window kept its old picture: paint all of it next time.
                if !posted {
                    raster.set_damage(Some(Rect::new(0.0, 0.0, size.0, size.1)));
                }
            }
        }
        next_frame(theme, state, host.touches.waiting())
    })
}

/// One frame of the app's UI inside the insets, `size` logical pixels in all.
fn draw(
    renderer: &mut dyn Renderer,
    app: &mut App,
    theme: &Theme,
    state: &mut UiState<u64>,
    (width, height): (f32, f32),
    [left, top, right, bottom]: [f32; 4],
) -> AureaResult<FrameOutcome> {
    let mut ctx = renderer.begin_frame()?;
    ctx.clear(theme.palette.background)?;
    ctx.translate(left, top)?;
    let mut ui = state.frame(&mut *ctx, theme);
    app(&mut ui, width - left - right, height - top - bottom)?;
    drop(ui);
    drop(ctx);
    renderer.end_frame()
}

/// Takes a pending theme; whether another frame is wanted, as for a finger still `waiting` to
/// become the mouse.
fn next_frame(theme: &mut Theme, state: &UiState<u64>, waiting: bool) -> bool {
    if let Some(next) = NEXT_THEME.lock().ok().and_then(|mut next| next.take()) {
        *theme = next;
        return true;
    }
    let more = state.wants_frame() || waiting;
    if !more && let Some(at) = state.next_frame_at() {
        wake_at(at, super::request_frame);
    }
    more
}
