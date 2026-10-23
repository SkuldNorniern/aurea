//! Alerts and the share sheet. Neither blocks, as a desktop message box does: the app keeps
//! drawing, and `done` hears the answer on the main thread.

use std::ffi::c_void;
use std::os::raw::{c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use aurea_ffi::{ng_ios_alert, ng_ios_share_files};

use super::documents::nul_list;
use crate::dialog::{MessageButtons, MessageChoice};
use crate::ffi::c_text;
use crate::{AureaError, AureaResult, Rect};

type Answered = Box<dyn FnOnce(c_int)>;

/// Asks `title` and `message` with `buttons`, worded in the user's language where iOS has the
/// words; `done` hears which was tapped.
///
/// # Errors
///
/// [`AureaError::InvalidText`] when the title or message holds a NUL, and
/// [`AureaError::ElementOperationFailed`] when it cannot show, before the app has a window; `done`
/// is then not called.
pub fn alert(
    title: &str,
    message: &str,
    buttons: MessageButtons,
    done: impl FnOnce(MessageChoice) + 'static,
) -> AureaResult<()> {
    let (title, message) = (c_text(title)?, c_text(message)?);
    let ctx = into_ctx(move |answer| {
        done(MessageChoice::from_code(answer).unwrap_or(MessageChoice::Cancel));
    });
    // SAFETY: the strings live for the call; `ctx` is ours until `answered` takes it back.
    let shown = unsafe {
        ng_ios_alert(
            title.as_ptr(),
            message.as_ptr(),
            buttons.code(),
            answered,
            ctx,
        )
    };
    shown_or_drop(shown, ctx)
}

/// The share sheet for `paths`: to other apps, AirDrop, Photos and the like. On an iPad it points
/// at `from`, in points from the safe area's top left, such as the button that asked; `done` hears
/// whether the files went somewhere.
///
/// # Errors
///
/// [`AureaError::ElementOperationFailed`] when `paths` is empty or the sheet cannot show; `done`
/// is then not called.
pub fn share_files(
    paths: &[PathBuf],
    from: Option<Rect>,
    done: impl FnOnce(bool) + 'static,
) -> AureaResult<()> {
    let list: Vec<c_char> = nul_list(paths.iter().map(|p| p.as_os_str().as_bytes()));
    let at = from.unwrap_or_default();
    let ctx = into_ctx(move |answer| done(answer != 0));
    // SAFETY: `list` is a NUL-separated list ending in an empty entry; `ctx` as in `alert`.
    let shown = unsafe {
        ng_ios_share_files(
            list.as_ptr(),
            at.x,
            at.y,
            at.width,
            at.height,
            answered,
            ctx,
        )
    };
    shown_or_drop(shown, ctx)
}

fn into_ctx(done: impl FnOnce(c_int) + 'static) -> *mut c_void {
    Box::into_raw(Box::new(Box::new(done) as Answered)).cast()
}

fn shown_or_drop(shown: c_int, ctx: *mut c_void) -> AureaResult<()> {
    if shown != 0 {
        return Ok(());
    }
    // SAFETY: nothing showed, so nothing took `ctx`, made by `into_ctx`.
    drop(unsafe { Box::from_raw(ctx.cast::<Answered>()) });
    Err(AureaError::ElementOperationFailed)
}

unsafe extern "C" fn answered(ctx: *mut c_void, answer: c_int) {
    // SAFETY: `ctx` came from `into_ctx`, and each sheet answers once.
    let done = unsafe { Box::from_raw(ctx.cast::<Answered>()) };
    done(answer);
}
