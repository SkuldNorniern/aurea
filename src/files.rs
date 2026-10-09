//! Handing files to the desktop: open one with its default application, or
//! show it in the file manager.
//!
//! Both start the system tool and return without waiting for it.

use std::path::Path;
use std::process::Command;

use crate::{AureaError, AureaResult, Platform};

/// Opens `path` the way double-clicking it in the file manager would.
pub fn open_path(path: &Path) -> AureaResult<()> {
    if cfg!(windows) {
        spawn(Command::new("explorer.exe").arg(path))
    } else if cfg!(target_os = "macos") {
        spawn(Command::new("open").arg(path))
    } else if cfg!(all(unix, not(target_os = "android"), not(target_os = "ios"))) {
        spawn(Command::new("xdg-open").arg(path))
    } else {
        unsupported("open_path")
    }
}

/// Shows the folder holding `path` in the file manager, with `path`
/// selected where the file manager can do that.
pub fn reveal_path(path: &Path) -> AureaResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Explorer reads `/select,` and the path as one argument.
        let arg = format!("/select,\"{}\"", path.display());
        return spawn(Command::new("explorer.exe").raw_arg(arg));
    }
    #[cfg(target_os = "macos")]
    {
        return spawn(Command::new("open").arg("-R").arg(path));
    }
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android"), not(target_os = "ios")))]
    {
        // The freedesktop file manager interface selects the file; without
        // one running, open the folder.
        let uri = format!("file://{}", path.display());
        let shown = Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.freedesktop.FileManager1",
                "--object-path",
                "/org/freedesktop/FileManager1",
                "--method",
                "org.freedesktop.FileManager1.ShowItems",
            ])
            .arg(format!("['{uri}']"))
            .arg("")
            .status()
            .is_ok_and(|status| status.success());
        if shown {
            return Ok(());
        }
        let folder = path.parent().unwrap_or(path);
        return spawn(Command::new("xdg-open").arg(folder));
    }
    #[allow(unreachable_code)]
    {
        let _ = path;
        unsupported("reveal_path")
    }
}

fn spawn(command: &mut Command) -> AureaResult<()> {
    command
        .spawn()
        .map(drop)
        .map_err(|e| AureaError::PlatformError(e.raw_os_error().unwrap_or(-1)))
}

fn unsupported(operation: &'static str) -> AureaResult<()> {
    Err(AureaError::Unsupported {
        operation,
        platform: Platform::current(),
    })
}
