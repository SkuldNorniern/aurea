//! Turns the platform's lifecycle notices into window events.

use super::close::CloseFlags;
use super::native;
use crate::lifecycle::{LifecycleCallback, LifecycleEvent};
use crate::render::{redraw_canvases_in, release_frames_in};
use crate::trim_memory;
use aurea_foundation::WindowEvent;
use aurea_runtime::{EventQueue, FrameScheduler};
use std::os::raw::c_void;

/// Physical pixels for `logical` at `scale`, rounded as the canvases size their buffers.
fn canvas_pixels(logical: u32, scale: f32) -> u32 {
    let pixels = (f64::from(logical) * f64::from(scale)).round();
    // logical sizes are window sized; the product stays far inside u32
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pixels = pixels.max(0.0) as u32;
    pixels
}
use std::sync::Arc;

/// The listener a window registers for its own lifecycle: converts each notice and queues it.
pub(super) fn listener(
    handle: *mut c_void,
    queue: Arc<EventQueue>,
    close: Arc<CloseFlags>,
) -> LifecycleCallback {
    // Raw pointers are not Send; the handle is only used on the UI thread.
    let handle = handle as usize;
    Arc::new(move |event| {
        if let Some(event) = convert(handle as *mut c_void, &close, event) {
            queue.push(event);
            FrameScheduler::wake();
        }
    })
}

fn convert(handle: *mut c_void, close: &CloseFlags, event: LifecycleEvent) -> Option<WindowEvent> {
    Some(match event {
        LifecycleEvent::WindowWillClose => {
            if close.take_forced() {
                WindowEvent::Closed
            } else {
                WindowEvent::CloseRequested
            }
        }
        LifecycleEvent::WindowMoved => {
            let (x, y) = native::position(handle);
            WindowEvent::Moved { x, y }
        }
        LifecycleEvent::WindowResized => {
            let (width, height) = native::size(handle);
            let (width, height) = (width.cast_unsigned(), height.cast_unsigned());
            let scale = native::scale(handle);
            WindowEvent::Resized {
                width,
                height,
                scale,
                canvas_width: canvas_pixels(width, scale),
                canvas_height: canvas_pixels(height, scale),
            }
        }
        LifecycleEvent::WindowMinimized => {
            release_frames_in(handle);
            WindowEvent::Minimized
        }
        LifecycleEvent::WindowRestored => {
            redraw_canvases_in(handle);
            WindowEvent::Restored
        }
        LifecycleEvent::MemoryWarning => {
            trim_memory();
            WindowEvent::MemoryWarning
        }
        LifecycleEvent::Occlusion(level) => WindowEvent::OcclusionChanged(level),
        LifecycleEvent::SurfaceLost => WindowEvent::SurfaceLost,
        LifecycleEvent::SurfaceRecreated => WindowEvent::SurfaceRecreated,
        _ => return None,
    })
}

#[cfg(test)]
mod canvas_tests {
    use super::canvas_pixels;

    #[test]
    fn canvas_pixels_round_like_the_canvases() {
        assert_eq!(canvas_pixels(800, 2.0), 1600);
        assert_eq!(canvas_pixels(801, 1.25), 1001);
        assert_eq!(canvas_pixels(3, 1.5), 5, "4.5 rounds up");
        assert_eq!(canvas_pixels(0, 2.0), 0);
    }
}
