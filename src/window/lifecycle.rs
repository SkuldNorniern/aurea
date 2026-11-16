//! Turns the platform's lifecycle notices into window events.

use super::close::CloseFlags;
use super::native;
use crate::lifecycle::{LifecycleCallback, LifecycleEvent};
use crate::render::{redraw_canvases_in, release_frames_in};
use crate::trim_memory;
use aurea_foundation::WindowEvent;
use aurea_runtime::{EventQueue, FrameScheduler};
use std::os::raw::c_void;
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
            WindowEvent::Resized {
                width: width.cast_unsigned(),
                height: height.cast_unsigned(),
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
