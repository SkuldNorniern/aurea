//! The system document picker: files to open, and copies of files to save elsewhere, such as
//! iCloud Drive. Neither blocks: the app keeps drawing, and `done` hears the paths on the main
//! thread once the user is through, with none when they cancelled.

use std::ffi::c_void;
use std::os::raw::{c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use aurea_ffi::{ng_ios_export_files, ng_ios_pick_files};

use crate::dialog::path_list;
use crate::{AureaError, AureaResult};

type Done = Box<dyn FnOnce(Vec<PathBuf>)>;

/// Lets the user choose files to open, of `extensions` (any file when empty), or several with
/// `multiple`. The chosen files are copies in the app's sandbox, free to read and keep.
///
/// # Errors
///
/// [`AureaError::ElementOperationFailed`] when the picker cannot show, before the app has a
/// window; `done` is then not called.
pub fn pick_files(
    extensions: &[&str],
    multiple: bool,
    done: impl FnOnce(Vec<PathBuf>) + 'static,
) -> AureaResult<()> {
    let list = nul_list(
        extensions
            .iter()
            .map(|e| e.trim_start_matches('.').as_bytes()),
    );
    let ctx = into_ctx(done);
    // SAFETY: `list` is a NUL-separated list ending in an empty entry; `ctx` is ours until
    // `picked` takes it back, or below when nothing shows.
    let shown = unsafe { ng_ios_pick_files(list.as_ptr(), c_int::from(multiple), picked, ctx) };
    shown_or_drop(shown, ctx)
}

/// Lets the user save copies of `paths` somewhere outside the app; `done` hears where they went.
///
/// # Errors
///
/// [`AureaError::ElementOperationFailed`] when `paths` is empty or the picker cannot show; `done`
/// is then not called.
pub fn export_files(
    paths: &[PathBuf],
    done: impl FnOnce(Vec<PathBuf>) + 'static,
) -> AureaResult<()> {
    let list = nul_list(paths.iter().map(|p| p.as_os_str().as_bytes()));
    let ctx = into_ctx(done);
    // SAFETY: as in `pick_files`.
    let shown = unsafe { ng_ios_export_files(list.as_ptr(), picked, ctx) };
    shown_or_drop(shown, ctx)
}

fn into_ctx(done: impl FnOnce(Vec<PathBuf>) + 'static) -> *mut c_void {
    Box::into_raw(Box::new(Box::new(done) as Done)).cast()
}

fn shown_or_drop(shown: c_int, ctx: *mut c_void) -> AureaResult<()> {
    if shown != 0 {
        return Ok(());
    }
    // SAFETY: the picker did not show, so it never took `ctx`, made by `into_ctx`.
    drop(unsafe { Box::from_raw(ctx.cast::<Done>()) });
    Err(AureaError::ElementOperationFailed)
}

/// Entries, each ended by a NUL, then an empty one.
fn nul_list<'a>(entries: impl Iterator<Item = &'a [u8]>) -> Vec<c_char> {
    let mut list = Vec::new();
    for entry in entries.filter(|e| !e.is_empty() && !e.contains(&0)) {
        list.extend(entry.iter().map(|b| b.cast_signed()));
        list.push(0);
    }
    list.push(0);
    list
}

unsafe extern "C" fn picked(ctx: *mut c_void, paths: *const c_char) {
    // SAFETY: `ctx` came from `into_ctx` and the picker calls this once.
    let done = unsafe { Box::from_raw(ctx.cast::<Done>()) };
    // SAFETY: the picker passes a path list as `path_list` reads it, alive for the call.
    let paths = if paths.is_null() {
        Vec::new()
    } else {
        unsafe { path_list(paths) }
    };
    done(paths);
}
