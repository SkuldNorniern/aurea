//! Handing files to the desktop: open one with its default application, or show it in the file
//! manager, and find the folders an app keeps its own files in.

use std::env;
use std::path::{self, Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "android")]
use crate::android;
#[cfg(target_os = "ios")]
use crate::ios;
use crate::{AureaError, AureaResult};

/// Opens `path` the way double-clicking it in the file manager would. On iOS, Quick Look shows it
/// over the app.
pub fn open_path(path: &Path) -> AureaResult<()> {
    let path = &launch_path(path);
    #[cfg(target_os = "ios")]
    {
        return ios::preview_path(path);
    }
    #[allow(unreachable_code)]
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
        Err(AureaError::unsupported("open_path"))
    }
}

/// Shows the folder holding `path` in the file manager, with `path` selected where the file manager
/// can do that. On iOS the Files app opens at the folder, if it is in the app's Documents.
pub fn reveal_path(path: &Path) -> AureaResult<()> {
    let path = &launch_path(path);
    #[cfg(target_os = "ios")]
    {
        return ios::reveal_path(path);
    }
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
        Err(AureaError::unsupported("reveal_path"))
    }
}

/// Opens `url` in the app that handles it: a web page in the browser, a `mailto:` link in the mail
/// app. Only `http`, `https` and `mailto` URLs are opened, so a link from a document or a terminal
/// cannot start a program through a scheme of its own.
///
/// # Errors
///
/// [`AureaError::InvalidUrl`] for any other scheme, or a URL with spaces or control characters,
/// and [`AureaError::ElementOperationFailed`] when nothing opens it.
pub fn open_url(url: &str) -> AureaResult<()> {
    if !openable_url(url) {
        return Err(AureaError::InvalidUrl);
    }
    #[cfg(target_os = "ios")]
    {
        return ios::open_url(url);
    }
    #[cfg(target_os = "android")]
    {
        return if android::open_url(url) {
            Ok(())
        } else {
            Err(AureaError::ElementOperationFailed)
        };
    }
    #[allow(unreachable_code)]
    if cfg!(windows) {
        // the URL protocol handler, not the shell: no command line to break out of.
        spawn(
            Command::new("rundll32.exe")
                .arg("url.dll,FileProtocolHandler")
                .arg(url),
        )
    } else if cfg!(target_os = "macos") {
        spawn(Command::new("open").arg(url))
    } else if cfg!(unix) {
        spawn(Command::new("xdg-open").arg(url))
    } else {
        Err(AureaError::unsupported("open_url"))
    }
}

/// Whether `url` is one [`open_url`] opens: `http`, `https` or `mailto`, then something, with no
/// spaces or control characters (which the URL spec has percent-escaped).
fn openable_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    let web = ["http", "https"]
        .iter()
        .any(|s| scheme.eq_ignore_ascii_case(s));
    let allowed = if web {
        rest.strip_prefix("//")
            .is_some_and(|host| !host.is_empty() && !host.starts_with('/'))
    } else {
        scheme.eq_ignore_ascii_case("mailto") && !rest.is_empty()
    };
    allowed && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The folder `app` keeps its settings in, for the current user: `%APPDATA%\app` on Windows,
/// `~/Library/Application Support/app` on macOS and iOS (inside the app's sandbox there) and
/// `$XDG_CONFIG_HOME/app` (or `~/.config/app`) elsewhere.
pub fn config_dir(app: &str) -> Option<PathBuf> {
    user_dir(app, "XDG_CONFIG_HOME", ".config")
}

/// The folder `app` keeps its data in, for the current user: the same as [`config_dir`] on
/// Windows, macOS and iOS, and `$XDG_DATA_HOME/app` (or `~/.local/share/app`) elsewhere.
pub fn data_dir(app: &str) -> Option<PathBuf> {
    user_dir(app, "XDG_DATA_HOME", ".local/share")
}

/// The folder `app` keeps large per-machine data in, such as checkpoints or thumbnails.
pub fn local_data_dir(app: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        env_path("LOCALAPPDATA").map(|base| base.join(app))
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        data_dir(app)
    } else {
        user_dir(app, "XDG_STATE_HOME", ".local/state")
    }
}

/// The folder `app` keeps caches in.
pub fn cache_dir(app: &str) -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        return android::cache_dir().map(|cache| cache.join(app));
    }
    #[allow(unreachable_code)]
    if cfg!(windows) {
        env_path("LOCALAPPDATA").map(|base| base.join(app).join("Cache"))
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        env_path("HOME").map(|home| home.join("Library").join("Caches").join(app))
    } else {
        user_dir(app, "XDG_CACHE_HOME", ".cache")
    }
}

/// The folder the user's own files go in: `~/Documents`. On iOS it is the app's Documents, which
/// the Files app shows when the app's Info.plist sets `UIFileSharingEnabled` and
/// `LSSupportsOpeningDocumentsInPlace`.
pub fn documents_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        return android::files_dir().map(|files| files.join("Documents"));
    }
    #[allow(unreachable_code)]
    if cfg!(windows) {
        env_path("USERPROFILE").map(|home| home.join("Documents"))
    } else {
        env_path("HOME").map(|home| home.join("Documents"))
    }
}

fn user_dir(app: &str, xdg: &str, home_relative: &str) -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        // the app's own private storage; `xdg` places mean nothing there
        let _ = (xdg, home_relative);
        return android::files_dir().map(|files| files.join(app));
    }
    #[allow(unreachable_code)]
    let base = if cfg!(windows) {
        env_path("APPDATA")
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        // an iOS app's HOME is its sandbox; only Library, Documents and tmp in it can be written.
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

/// `path` made absolute, since file managers do not resolve against this process's directory and
/// a leading `-` reads as an option to `open` or `xdg-open`.
fn launch_path(path: &Path) -> PathBuf {
    let path = path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    if path.as_os_str().as_encoded_bytes().first() == Some(&b'-') {
        Path::new(".").join(path)
    } else {
        path
    }
}

fn spawn(command: &mut Command) -> AureaResult<()> {
    command
        .spawn()
        .map(drop)
        .map_err(|e| AureaError::PlatformError(e.raw_os_error().unwrap_or(-1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_and_mail_urls_open() {
        for url in [
            "https://example.com/a?b=c#d",
            "HTTP://example.com",
            "mailto:me@example.com",
        ] {
            assert!(openable_url(url), "{url}");
        }
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ms-settings:",
            r"C:\Windows\notepad.exe",
            "https://",
            "https:///path",
            "https:example.com",
            "https://example.com/a b",
            "https://example.com/\n--flag",
            "mailto:",
            "-https://x",
        ] {
            assert!(!openable_url(url), "{url}");
        }
    }

    #[test]
    fn launched_paths_are_absolute() {
        let cwd = env::current_dir().expect("cwd");
        assert_eq!(launch_path(Path::new("-n")), cwd.join("-n"));
        assert_eq!(launch_path(Path::new("a-b")), cwd.join("a-b"));
        let root = cwd.ancestors().last().expect("root").join("-n");
        assert_eq!(launch_path(&root), root);
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
