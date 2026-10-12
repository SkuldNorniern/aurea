//! Handing files to the desktop: open one with its default application, or show it in the file
//! manager, and find the folders an app keeps its own files in.

use std::env;
use std::path::{Path, PathBuf};
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

/// Shows the folder holding `path` in the file manager, with `path` selected where the file manager
/// can do that.
pub fn reveal_path(path: &Path) -> AureaResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let arg = format!("/select,\"{}\"", path.display());
        return spawn(Command::new("explorer.exe").raw_arg(arg));
    }
    #[cfg(target_os = "macos")]
    {
        return spawn(Command::new("open").arg("-R").arg(path));
    }
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android"), not(target_os = "ios")))]
    {
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

/// The folder `app` keeps its settings in, for the current user: `%APPDATA%\app` on Windows,
/// `~/Library/Application Support/app` on macOS and `$XDG_CONFIG_HOME/app` (or `~/.config/app`)
/// elsewhere.
pub fn config_dir(app: &str) -> Option<PathBuf> {
    user_dir(app, "XDG_CONFIG_HOME", ".config")
}

/// The folder `app` keeps its data in, for the current user: the same as [`config_dir`] on Windows
/// and macOS, and `$XDG_DATA_HOME/app` (or `~/.local/share/app`) elsewhere.
pub fn data_dir(app: &str) -> Option<PathBuf> {
    user_dir(app, "XDG_DATA_HOME", ".local/share")
}

fn user_dir(app: &str, xdg: &str, home_relative: &str) -> Option<PathBuf> {
    let var = |name: &str| {
        env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        var(xdg).or_else(|| var("HOME").map(|home| home.join(home_relative)))
    };
    base.map(|base| base.join(app))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_folders_end_in_the_app_name() {
        for dir in [config_dir("Aurea Test"), data_dir("Aurea Test")]
            .into_iter()
            .flatten()
        {
            assert!(dir.ends_with("Aurea Test"));
        }
    }
}
