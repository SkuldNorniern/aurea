//! Handing files to the desktop: open one with its default application, or show it in the file
//! manager, and find the folders an app keeps its own files in.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{AureaError, AureaResult, Platform};

/// Opens `path` the way double-clicking it in the file manager would.
pub fn open_path(path: &Path) -> AureaResult<()> {
    let path = &not_an_option(path);
    if cfg!(windows) {
        spawn(Command::new("explorer.exe").arg(path))
    } else if cfg!(target_os = "macos") {
        spawn(Command::new("open").arg(path))
    } else if cfg!(all(
        unix,
        not(target_os = "android"),
        not(target_os = "ios")
    )) {
        spawn(Command::new("xdg-open").arg(path))
    } else {
        unsupported("open_path")
    }
}

/// Shows the folder holding `path` in the file manager, with `path` selected where the file manager
/// can do that.
pub fn reveal_path(path: &Path) -> AureaResult<()> {
    let path = &not_an_option(path);
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
    #[cfg(all(
        unix,
        not(target_os = "macos"),
        not(target_os = "android"),
        not(target_os = "ios")
    ))]
    {
        let uri = file_uri(path);
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

/// The folder `app` keeps large per-machine data in, such as checkpoints or thumbnails.
pub fn local_data_dir(app: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        env_path("LOCALAPPDATA").map(|base| base.join(app))
    } else if cfg!(target_os = "macos") {
        data_dir(app)
    } else {
        user_dir(app, "XDG_STATE_HOME", ".local/state")
    }
}

/// The folder `app` keeps caches in.
pub fn cache_dir(app: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        env_path("LOCALAPPDATA").map(|base| base.join(app).join("Cache"))
    } else if cfg!(target_os = "macos") {
        env_path("HOME").map(|home| home.join("Library").join("Caches").join(app))
    } else {
        user_dir(app, "XDG_CACHE_HOME", ".cache")
    }
}

fn user_dir(app: &str, xdg: &str, home_relative: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        env_path("APPDATA")
    } else if cfg!(target_os = "macos") {
        env_path("HOME").map(|home| home.join("Library").join("Application Support"))
    } else {
        env_path(xdg).or_else(|| env_path("HOME").map(|home| home.join(home_relative)))
    };
    base.map(|base| base.join(app))
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(any(
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "android"),
        not(target_os = "ios")
    ),
    test
))]
fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// A relative path starting with `-` reads as an option to `open` or `xdg-open`; `./-x` names the
/// same file and does not.
fn not_an_option(path: &Path) -> PathBuf {
    if path.as_os_str().as_encoded_bytes().first() == Some(&b'-') {
        Path::new(".").join(path)
    } else {
        path.to_path_buf()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_starting_with_a_dash_is_not_an_option() {
        assert_eq!(not_an_option(Path::new("-n")), PathBuf::from("./-n"));
        assert_eq!(
            not_an_option(Path::new("/tmp/-n")),
            PathBuf::from("/tmp/-n")
        );
        assert_eq!(not_an_option(Path::new("a-b")), PathBuf::from("a-b"));
    }

    #[test]
    fn file_uri_escapes_what_a_uri_or_gvariant_string_cannot_hold() {
        assert_eq!(
            file_uri(Path::new("/home/me/My Files/it's #1.txt")),
            "file:///home/me/My%20Files/it%27s%20%231.txt"
        );
        assert_eq!(
            file_uri(Path::new("/tmp/caf\u{e9}")),
            "file:///tmp/caf%C3%A9"
        );
    }

    #[test]
    fn app_folders_end_in_the_app_name() {
        for dir in [config_dir("Aurea Test"), data_dir("Aurea Test")]
            .into_iter()
            .flatten()
        {
            assert!(dir.ends_with("Aurea Test"));
        }
    }

    #[test]
    fn local_and_cache_folders_end_in_the_app_name() {
        if let Some(dir) = local_data_dir("Aurea Test") {
            assert!(dir.ends_with("Aurea Test"));
        }
        if let Some(dir) = cache_dir("Aurea Test") {
            let app = if cfg!(windows) {
                dir.parent().unwrap_or(dir.as_path())
            } else {
                dir.as_path()
            };
            assert!(app.ends_with("Aurea Test"));
        }
    }

    #[test]
    fn windows_local_folders_sit_under_local_app_data() {
        if !cfg!(windows) {
            return;
        }
        let Some(local) = env_path("LOCALAPPDATA") else {
            return;
        };
        if let Some(dir) = local_data_dir("Aurea Test") {
            assert!(dir.starts_with(&local));
        }
        if let Some(dir) = cache_dir("Aurea Test") {
            assert!(dir.starts_with(&local));
            assert!(dir.ends_with(Path::new("Aurea Test").join("Cache")));
        }
    }
}
