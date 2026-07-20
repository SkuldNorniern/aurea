use crate::render::Canvas;
use aurea_runtime::FrameScheduler;
use crate::window::{Window, WindowEvent, push_window_event};
use std::os::raw::c_void;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceErrorAction {
    /// Surface should be reconfigured/recreated.
    Recreate,
    /// Transient error; skip this frame.
    Skip,
    /// Fatal error; surface cannot be recovered.
    Fatal,
}

/// Classify a `CurrentSurfaceTexture` result for the given handle.
/// Pushes `WindowEvent::SurfaceLost` on Lost/Outdated and returns the action.
pub fn handle_surface_result_for_handle(
    handle: *mut c_void,
    result: &wgpu::CurrentSurfaceTexture,
) -> SurfaceErrorAction {
    match result {
        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
            if !handle.is_null() {
                push_window_event(handle, WindowEvent::SurfaceLost);
                FrameScheduler::schedule();
            }
            SurfaceErrorAction::Recreate
        }
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
            SurfaceErrorAction::Skip
        }
        wgpu::CurrentSurfaceTexture::Validation => SurfaceErrorAction::Fatal,
        wgpu::CurrentSurfaceTexture::Success(_) | wgpu::CurrentSurfaceTexture::Suboptimal(_) => {
            SurfaceErrorAction::Skip
        }
    }
}

/// Call this when `Surface::get_current_texture()` returns a non-success result for a window.
/// Pushes `WindowEvent::SurfaceLost` on Lost/Outdated and returns Recreate/Skip/Fatal.
pub fn handle_surface_result_for_window(
    window: &Window,
    result: &wgpu::CurrentSurfaceTexture,
) -> SurfaceErrorAction {
    handle_surface_result_for_handle(window.handle(), result)
}

/// Call this when `Surface::get_current_texture()` returns a non-success result for a canvas.
/// Pushes `WindowEvent::SurfaceLost` on Lost/Outdated and returns Recreate/Skip/Fatal.
pub fn handle_surface_result_for_canvas(
    canvas: &Canvas,
    result: &wgpu::CurrentSurfaceTexture,
) -> SurfaceErrorAction {
    handle_surface_result_for_handle(canvas.window_handle(), result)
}

/// Call after recreating a window-backed wgpu surface so `SurfaceRecreated` is emitted and redraw scheduled.
pub fn notify_surface_recreated_for_window(window: &Window) {
    push_window_event(window.handle(), WindowEvent::SurfaceRecreated);
    FrameScheduler::schedule();
}

pub fn notify_surface_recreated_for_handle(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    push_window_event(handle, WindowEvent::SurfaceRecreated);
    FrameScheduler::schedule();
}

/// Call after recreating a canvas-backed wgpu surface so `SurfaceRecreated` is emitted and redraw scheduled.
pub fn notify_surface_recreated_for_canvas(canvas: &Canvas) {
    let handle = canvas.window_handle();
    if handle.is_null() {
        return;
    }
    push_window_event(handle, WindowEvent::SurfaceRecreated);
    FrameScheduler::schedule();
}
