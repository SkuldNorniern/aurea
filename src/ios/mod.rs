//! iOS: the app's UI filling the screen, drawn on the CPU each frame, touches fed in as the
//! pointer.

use std::cell::RefCell;
use std::process::exit;
use std::ptr::null;

use aurea_ffi::{NGIosHost, ng_ios_run_host};

use crate::ffi::callbacks::event_time;
use crate::render::{CpuRasterizer, Renderer, SurfaceInfo};
use crate::ui::{Theme, Ui, UiState};
use crate::{
    AureaResult, Modifiers, MouseButton, MouseButtons, PenPhase, PenSample, PenTool, WindowEvent,
};

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
}

/// Hears what the user does, before the UI does.
type Listener = Box<dyn FnMut(&WindowEvent)>;

thread_local! {
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
    };
    exit(unsafe { ng_ios_run_host(host) })
}

/// Calls `listener` with each touch, as mouse events, and each Apple Pencil sample, as
/// [`WindowEvent::Pen`] with pressure and tilt, the way a desktop window reports a pen. The next
/// frame is drawn after each.
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

unsafe extern "C" fn touch(action: i32, x: f32, y: f32) {
    let events = HOST.with_borrow_mut(|host| {
        let [left, top, ..] = host.insets;
        let (x, y) = (f64::from(x - left), f64::from(y - top));
        let modifiers = Modifiers::default();
        let held = MouseButtons::from_bits(1 << MouseButton::Left.to_raw());
        let at = |buttons| WindowEvent::MouseMove {
            x,
            y,
            buttons,
            modifiers,
            time: Default::default(),
        };
        let button = |pressed| WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed,
            modifiers,
            x,
            y,
            click_count: 1,
            time: Default::default(),
        };
        let events = match action {
            0 => vec![at(MouseButtons::default()), button(true)],
            2 => vec![at(held)],
            1 => vec![button(false), WindowEvent::MouseExited],
            3 => vec![WindowEvent::MouseExited],
            _ => Vec::new(),
        };
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
    HOST.with_borrow_mut(|host| {
        let scale = scale.max(1.0);
        if host.pixels != (width, height) || (host.scale - scale).abs() > f32::EPSILON {
            host.pixels = (width, height);
            host.scale = scale;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let logical = |px: u32| (f64::from(px) / f64::from(scale)).ceil() as u32;
            let info = SurfaceInfo::new(logical(width).max(1), logical(height).max(1), scale)
                .with_pixels(width, height);
            if let Err(e) = host.raster.configure(info) {
                log::warn!("ios surface: {e}");
                return 0;
            }
        }
        host.insets = [left, top, right, bottom].map(|v| v.max(0.0));
        let Host {
            app: Some(app),
            theme,
            raster,
            state,
            insets: [left, top, right, bottom],
            ..
        } = host
        else {
            return 0;
        };
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (
            width as f32 / scale - *left - *right,
            height as f32 / scale - *top - *bottom,
        );
        let drawn = (|| -> AureaResult<()> {
            let mut ctx = raster.begin_frame()?;
            ctx.clear(theme.palette.background)?;
            ctx.translate(*left, *top)?;
            let mut ui = state.frame(&mut *ctx, theme);
            app(&mut ui, w, h)?;
            drop(ui);
            drop(ctx);
            raster.end_frame()?;
            Ok(())
        })();
        if let Err(e) = drawn {
            log::warn!("ios frame: {e}");
            return 0;
        }
        if let Some(out) = raster.frame_output() {
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
