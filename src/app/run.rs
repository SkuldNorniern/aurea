//! One way to start an app on every platform.

use crate::AureaResult;
use crate::ui::{FrameAsk, Theme, Ui};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::render::{Canvas, RendererBackend, request_canvas_redraw, request_canvas_redraw_at};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::ui::UiState;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::{
    Window, WindowEvent,
    elements::{Container, Orientation, Stack},
};

#[cfg(target_os = "android")]
use crate::android;
#[cfg(target_os = "ios")]
use crate::ios;

/// A desktop window's first size, in logical pixels.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const FIRST_SIZE: (i32, i32) = (1280, 800);

/// Runs `app` as the app's UI, whichever platform this is: a desktop window named from
/// [`super::identity`], or the iOS or Android host. `app` draws a frame, given the area it has,
/// the safe area on a phone, and may say it wants another frame with a [`FrameAsk`], or end with
/// `Ok(())`.
///
/// Returns when the desktop window closes; Android's returns at once as its Activity drives the
/// frames, and iOS's doesn't return. Call it from `main`, or from `aurea_main` on a phone.
///
/// ```no_run
/// use aurea::ui::Theme;
///
/// fn main() -> aurea::AureaResult<()> {
///     aurea::app::run(Theme::dark(), |ui, width, height| {
///         ui.fill(aurea::Rect::new(0.0, 0.0, width, height), ui.theme.palette.surface)
///     })
/// }
/// ```
///
/// # Errors
///
/// What creating the window or canvas reports, on the desktop.
pub fn run<R: Into<FrameAsk>>(
    theme: Theme,
    app: impl FnMut(&mut Ui<'_, u64>, f32, f32) -> AureaResult<R> + 'static,
) -> AureaResult<()> {
    #[cfg(target_os = "ios")]
    ios::run(theme, app);
    #[cfg(target_os = "android")]
    {
        android::run(theme, app);
        Ok(())
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let (width, height) = FIRST_SIZE;
        let mut app = app;
        let mut window = Window::new(super::identity().name, width, height)?;
        let canvas = Canvas::new(
            width.unsigned_abs(),
            height.unsigned_abs(),
            RendererBackend::Cpu,
        )?;
        let id = canvas.id();
        let state = Rc::new(RefCell::new(UiState::<u64>::new()));
        let drawn = Rc::clone(&state);
        let window_ref: Rc<RefCell<Weak<Window>>> = Rc::default();
        let seen = Rc::clone(&window_ref);
        canvas.set_draw_callback(move |ctx| {
            let mut state = drawn.borrow_mut();
            ctx.clear(theme.palette.background)?;
            #[allow(clippy::cast_precision_loss)]
            let (w, h) = (ctx.width() as f32, ctx.height() as f32);
            {
                let mut ui = state.frame(ctx, &theme);
                let ask = app(&mut ui, w, h)?.into();
                ui.ask(ask);
            }
            if let Some(window) = seen.borrow().upgrade() {
                state.sync_window(&window);
            }
            if state.wants_frame() {
                request_canvas_redraw(id);
            } else if let Some(at) = state.next_frame_at() {
                request_canvas_redraw_at(id, at);
            }
            Ok(())
        })?;
        let input = Rc::clone(&state);
        window.on_event(move |event| {
            let resized = matches!(event, WindowEvent::Resized { .. });
            if input.borrow_mut().handle(event) || resized {
                request_canvas_redraw(id);
            }
        });
        let mut layout = Stack::new(Orientation::Vertical)?;
        layout.set_padding(0)?;
        layout.add(canvas)?;
        window.set_content(layout)?;
        let window = Rc::new(window);
        *window_ref.borrow_mut() = Rc::downgrade(&window);
        window.show();
        window.run()
    }
}
