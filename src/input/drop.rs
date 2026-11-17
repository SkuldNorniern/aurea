//! Files dragged over or dropped on a window, as `WindowEvent::FilesHovered` and
//! `WindowEvent::FilesDropped`.

use std::os::raw::c_char;
use std::path::PathBuf;

use super::c_string;
use crate::WindowEvent;

const HOVER: u32 = 0;
const DROP: u32 = 1;

/// The event for a drag of `kind` at (`x`, `y`), with `paths` one per line. Any other kind ends
/// a hover.
pub(crate) fn event(kind: u32, x: f64, y: f64, paths: *const c_char) -> WindowEvent {
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
