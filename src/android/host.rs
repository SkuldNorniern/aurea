//! One Aurea UI on an Android `SurfaceView`: frames drawn on the CPU and posted to its window,
//! touches fed in as the pointer.

use std::cell::RefCell;

use jni::JNIEnv;
use jni::objects::JObject;

use super::window::NativeWindow;
use crate::render::{CpuRasterizer, Renderer, SurfaceInfo};
use crate::ui::{Theme, Ui, UiState};
use crate::{AureaResult, Modifiers, MouseButton, MouseButtons, WindowEvent};

/// Draws the app's UI each frame, `width` x `height` logical pixels.
pub type App = Box<dyn FnMut(&mut Ui<'_, u64>, f32, f32) -> AureaResult<()>>;

struct Host {
    app: Option<App>,
    theme: Theme,
    window: Option<NativeWindow>,
    raster: CpuRasterizer,
    state: UiState<u64>,
    scale: f32,
    logical: (u32, u32),
    insets: [f32; 4],
}

thread_local! {
    static NEXT_THEME: RefCell<Option<Theme>> = const { RefCell::new(None) };
    static HOST: RefCell<Host> = RefCell::new(Host {
        app: None,
        theme: Theme::dark(),
        window: None,
        raster: CpuRasterizer::new(1, 1),
        state: UiState::new(),
        scale: 1.0,
        logical: (1, 1),
        insets: [0.0; 4],
    });
}

pub(super) fn set_app(app: App, theme: Theme) {
    HOST.with_borrow_mut(|host| {
        host.app = Some(app);
        host.theme = theme;
    });
}

pub(super) fn set_theme(theme: Theme) {
    NEXT_THEME.set(Some(theme));
}

pub(super) fn surface_changed(
    env: &JNIEnv,
    surface: &JObject,
    width: u32,
    height: u32,
    scale: f32,
) {
    HOST.with_borrow_mut(|host| {
        host.window = None;
        host.window = NativeWindow::from_surface(env, surface, width, height);
        host.scale = scale.max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let logical = |px: u32| (f64::from(px) / f64::from(host.scale)).ceil() as u32;
        host.logical = (logical(width).max(1), logical(height).max(1));
        let info =
            SurfaceInfo::new(host.logical.0, host.logical.1, host.scale).with_pixels(width, height);
        if let Err(e) = host.raster.configure(info) {
            log::warn!("android surface: {e}");
        }
    });
}

pub(super) fn set_insets(left: f32, top: f32, right: f32, bottom: f32) {
    HOST.with_borrow_mut(|host| {
        host.insets = [left, top, right, bottom].map(|v| v.max(0.0) / host.scale);
    });
}

pub(super) fn light_theme() -> bool {
    HOST.with_borrow(|host| {
        let c = host.theme.palette.background;
        u32::from(c.r) * 299 + u32::from(c.g) * 587 + u32::from(c.b) * 114 > 128_000
    })
}

pub(super) fn surface_destroyed() {
    HOST.with_borrow_mut(|host| host.window = None);
}

pub(super) fn touch(action: i32, x: f32, y: f32) {
    HOST.with_borrow_mut(|host| {
        let [left, top, ..] = host.insets;
        let (x, y) = (
            f64::from(x / host.scale - left),
            f64::from(y / host.scale - top),
        );
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
        let events: &[WindowEvent] = match action {
            0 => &[at(MouseButtons::default()), button(true)],
            2 => &[at(held)],
            1 => &[button(false), WindowEvent::MouseExited],
            3 => &[WindowEvent::MouseExited],
            _ => &[],
        };
        for event in events {
            host.state.handle(event);
        }
    });
}

pub(super) fn frame() -> bool {
    HOST.with_borrow_mut(|host| {
        let Host {
            app: Some(app),
            theme,
            window: Some(window),
            raster,
            state,
            logical,
            insets: [left, top, right, bottom],
            ..
        } = host
        else {
            return false;
        };
        let (w, h) = (
            logical.0 as f32 - *left - *right,
            logical.1 as f32 - *top - *bottom,
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
            log::warn!("android frame: {e}");
            return false;
        }
        if let Some(out) = raster.frame_output() {
            window.post(out.words(), out.width, out.height);
        }
        if let Some(next) = NEXT_THEME.take() {
            *theme = next;
            return true;
        }
        state.wants_frame()
    })
}
