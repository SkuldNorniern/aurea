#[cfg(not(target_os = "windows"))]
use super::Handler;
#[cfg(not(target_os = "windows"))]
use crate::AureaResult;
#[cfg(not(target_os = "windows"))]
use crate::render::Canvas;
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
use crate::{AureaError, Platform};
#[cfg(not(target_os = "windows"))]
use accesskit::TreeUpdate;

#[cfg(target_os = "macos")]
use accesskit_macos::SubclassingAdapter as NativeAdapter;
#[cfg(target_os = "linux")]
use accesskit_unix::Adapter as NativeAdapter;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(super) use windows::Adapter;

#[cfg(target_os = "macos")]
pub(super) struct Adapter(NativeAdapter);

#[cfg(target_os = "macos")]
impl Adapter {
    pub(super) fn new(canvas: &Canvas, handler: Handler) -> AureaResult<Self> {
        // Canvas keeps this NSView alive and the bridge is UI-thread affine.
        Ok(Self(unsafe {
            NativeAdapter::new(canvas.native_handle(), handler.clone(), handler)
        }))
    }
    pub(super) fn update(&mut self, update: TreeUpdate) {
        if let Some(events) = self.0.update_if_active(|| update) {
            events.raise();
        }
    }
    pub(super) fn focused(&mut self, focused: bool) {
        if let Some(events) = self.0.update_view_focus_state(focused) {
            events.raise();
        }
    }
}

#[cfg(target_os = "linux")]
pub(super) struct Adapter(NativeAdapter);

#[cfg(target_os = "linux")]
mod gtk_geometry {
    use crate::render::Canvas;
    use accesskit::Rect;
    use std::ffi::{c_int, c_void};

    unsafe extern "C" {
        fn gtk_widget_get_toplevel(widget: *mut c_void) -> *mut c_void;
        fn gtk_widget_get_window(widget: *mut c_void) -> *mut c_void;
        fn gtk_widget_translate_coordinates(
            widget: *mut c_void,
            dest: *mut c_void,
            x: c_int,
            y: c_int,
            dest_x: *mut c_int,
            dest_y: *mut c_int,
        ) -> c_int;
        fn gtk_widget_get_allocated_width(widget: *mut c_void) -> c_int;
        fn gtk_widget_get_allocated_height(widget: *mut c_void) -> c_int;
        fn gdk_window_get_origin(window: *mut c_void, x: *mut c_int, y: *mut c_int) -> c_int;
    }

    pub(super) fn bounds(canvas: &Canvas) -> Option<Rect> {
        // Called only on the UI thread for a live GtkWidget. Wayland cannot
        // expose a global window position; GDK supplies the available origin.
        unsafe {
            let top = gtk_widget_get_toplevel(canvas.handle);
            let window = gtk_widget_get_window(top);
            if window.is_null() {
                return None;
            }
            let (mut x, mut y, mut dx, mut dy) = (0, 0, 0, 0);
            gdk_window_get_origin(window, &mut x, &mut y);
            if gtk_widget_translate_coordinates(canvas.handle, top, 0, 0, &mut dx, &mut dy) == 0 {
                return None;
            }
            let x = f64::from(x) + f64::from(dx);
            let y = f64::from(y) + f64::from(dy);
            Some(Rect::new(
                x,
                y,
                x + f64::from(gtk_widget_get_allocated_width(canvas.handle)),
                y + f64::from(gtk_widget_get_allocated_height(canvas.handle)),
            ))
        }
    }
}

#[cfg(target_os = "linux")]
impl Adapter {
    pub(super) fn new(_: &Canvas, handler: Handler) -> AureaResult<Self> {
        Ok(Self(NativeAdapter::new(
            handler.clone(),
            handler.clone(),
            handler,
        )))
    }
    pub(super) fn update(&mut self, update: TreeUpdate) {
        self.0.update_if_active(|| update);
    }
    pub(super) fn focused(&mut self, focused: bool) {
        self.0.update_window_focus_state(focused);
    }
    pub(super) fn geometry(&mut self, canvas: &Canvas) {
        if let Some(rect) = gtk_geometry::bounds(canvas) {
            self.0.set_root_window_bounds(rect, rect);
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub(super) struct Adapter;

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
impl Adapter {
    pub(super) fn new(_: &Canvas, _: Handler) -> AureaResult<Self> {
        Err(AureaError::Unsupported {
            operation: "canvas_accessibility",
            platform: Platform::current(),
        })
    }
    pub(super) fn update(&mut self, _: TreeUpdate) {}
    pub(super) fn focused(&mut self, _: bool) {}
}
