//! Open, save and share, one call on every platform. A desktop dialog answers before the call
//! returns; a phone's picker answers later. Either way `done` hears it on the UI thread.
//!
//! What a phone's picker hands over is a copy the app may keep, so no access has to be held open.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;
#[cfg(any(target_os = "ios", target_os = "android"))]
use std::slice;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_os = "android")]
use crate::android as host;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::dialog::FileDialog;
use crate::files::app_cache_dir;
#[cfg(target_os = "ios")]
use crate::ios as host;
use crate::{AureaError, AureaResult, Rect, Window};

#[cfg_attr(
    not(any(test, target_os = "ios", target_os = "android")),
    allow(dead_code)
)]
static STAGED: AtomicU64 = AtomicU64::new(0);

/// A folder of its own in the app's cache holding `bytes` as `name`.
#[cfg_attr(
    not(any(test, target_os = "ios", target_os = "android")),
    allow(dead_code)
)]
fn stage(name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let file = Path::new(name)
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let root = app_cache_dir().unwrap_or_else(env::temp_dir);
    let n = STAGED.fetch_add(1, Ordering::Relaxed);
    let folder = root.join(format!("aurea-out-{}-{n}", process::id()));
    fs::create_dir_all(&folder)?;
    let path = folder.join(file);
    fs::write(&path, bytes)?;
    Ok(path)
}

/// Removes what [`stage`] made.
#[cfg_attr(
    not(any(test, target_os = "ios", target_os = "android")),
    allow(dead_code)
)]
fn unstage(path: &Path) {
    if let Some(folder) = path.parent()
        && let Err(error) = fs::remove_dir_all(folder)
    {
        log::warn!("{}: {error}", folder.display());
    }
}

fn failed(_: io::Error) -> AureaError {
    AureaError::ElementOperationFailed
}

/// Files for the user to pick, of `extensions` (any file when empty), or several with `multiple`.
/// `parent` is the window a desktop dialog sits over. `done` hears none when cancelled.
///
/// # Errors
///
/// What the dialog or picker reports when it cannot show; `done` is then not called.
pub fn open(
    extensions: &[&str],
    multiple: bool,
    parent: Option<&Window>,
    done: impl FnOnce(Vec<PathBuf>) + 'static,
) -> AureaResult<()> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = parent;
        host::pick_files(extensions, multiple, done)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let mut dialog = FileDialog::new().multiple(multiple);
        if !extensions.is_empty() {
            dialog = dialog.filter("Files", extensions);
        }
        done(dialog.open_over(parent)?);
        Ok(())
    }
}

/// Lets the user save `bytes`, suggesting `name`. `done` hears where it went, or `None` when
/// cancelled. On a phone that is a `content:` URI or a file outside the app; the copy made to hand
/// over is cleaned up after `done`.
///
/// # Errors
///
/// [`AureaError::ElementOperationFailed`] when `name` has no file name or the copy can't be
/// written, and what the dialog or picker reports when it cannot show. `done` is then not called.
pub fn save(
    name: &str,
    bytes: &[u8],
    parent: Option<&Window>,
    done: impl FnOnce(Option<PathBuf>) + 'static,
) -> AureaResult<()> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = parent;
        let staged = stage(name, bytes).map_err(failed)?;
        let copy = staged.clone();
        let answered = move |went: Vec<PathBuf>| {
            done(went.into_iter().next());
            unstage(&copy);
        };
        let shown = host::export_files(slice::from_ref(&staged), answered);
        if shown.is_err() {
            unstage(&staged);
        }
        shown
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let Some(path) = FileDialog::new().file_name(name).save_over(parent)? else {
            done(None);
            return Ok(());
        };
        fs::write(&path, bytes).map_err(failed)?;
        done(Some(path));
        Ok(())
    }
}

/// The system's share sheet for `bytes` as a file called `name`. `from` is what an iPad's sheet
/// points at. `done` hears whether it went somewhere, as far as the platform says.
///
/// # Errors
///
/// [`AureaError::Unsupported`] on the desktop, which has no share sheet here, and what the sheet
/// reports when it cannot show. `done` is then not called.
pub fn share(
    name: &str,
    bytes: &[u8],
    from: Option<Rect>,
    done: impl FnOnce(bool) + 'static,
) -> AureaResult<()> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let staged = stage(name, bytes).map_err(failed)?;
        let copy = staged.clone();
        let answered = move |went: bool| {
            done(went);
            // Android answers as the sheet opens, while another app may still read the file.
            #[cfg(target_os = "ios")]
            unstage(&copy);
            #[cfg(target_os = "android")]
            let _ = &copy;
        };
        let shown = host::share_files(slice::from_ref(&staged), from, answered);
        if shown.is_err() {
            unstage(&staged);
        }
        shown
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let _ = (name, bytes, from, done);
        Err(AureaError::unsupported("share"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_staged_copy_keeps_only_the_file_name_and_cleans_up() {
        let path = stage("../../not/here/report.lpaint", b"bytes").expect("stage");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("report.lpaint")
        );
        assert_eq!(fs::read(&path).expect("read"), b"bytes");
        let folder = path.parent().expect("folder").to_path_buf();
        unstage(&path);
        assert!(!folder.exists());
        assert!(stage("..", b"x").is_err(), "no file name");
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[test]
    fn the_desktop_has_no_share_sheet() {
        let refused = share("a.txt", b"x", None, |_| {}).expect_err("no sheet");
        assert!(refused.is_unsupported());
    }
}
