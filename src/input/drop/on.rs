//! Files dragged over or dropped on a window.

use std::os::raw::{c_char, c_void};
use std::path::PathBuf;

use crate::WindowEvent;
use crate::input::c_string;
use crate::window::push_window_event;

const HOVER: u32 = 0;
const DROP: u32 = 1;

/// Queues a drag of `kind` at (`x`, `y`) over `window`, with `paths` one per line.
pub(crate) fn deliver(window: *mut c_void, kind: u32, x: f64, y: f64, paths: *const c_char) {
    push_window_event(window, event(kind, x, y, paths));
}

/// The event for a drag of `kind` at (`x`, `y`), with `paths` one per line. Any other kind ends
/// a hover.
fn event(kind: u32, x: f64, y: f64, paths: *const c_char) -> WindowEvent {
    let paths: Vec<PathBuf> = c_string(paths)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect();
    match kind {
        HOVER => WindowEvent::FilesHovered { paths, x, y },
        DROP => WindowEvent::FilesDropped { paths, x, y },
        _ => WindowEvent::FilesHoverEnded,
    }
}

/// The files iOS dropped on the screen, as `FilesDropped`: `paths` is NUL-terminated paths, ended
/// by an empty one.
///
/// # Safety
///
/// `paths` is the host's list, valid for the call.
#[cfg(target_os = "ios")]
pub(crate) unsafe fn dropped(paths: *const c_char, x: f64, y: f64) -> Option<WindowEvent> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    let mut list = Vec::new();
    let mut at = paths;
    // SAFETY: the host gives NUL-terminated paths ended by an empty one.
    while let Some(path) = unsafe { CStr::from_ptr(at) }
        .to_bytes()
        .first()
        .map(|_| unsafe { CStr::from_ptr(at) })
    {
        list.push(PathBuf::from(OsStr::from_bytes(path.to_bytes())));
        // SAFETY: past this path's NUL there is at least the empty one.
        at = unsafe { at.add(path.to_bytes().len() + 1) };
    }
    Some(WindowEvent::FilesDropped { paths: list, x, y })
}
