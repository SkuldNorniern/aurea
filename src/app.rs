//! The app's identity: its name, id, organization and version, from the app crate's
//! `[package.metadata.aurea]` table. The app's `build.rs` calls `aurea_build::identity()`, and
//! its `main` hands [`identity!`](crate::identity) to [`set_identity`] before anything reads it:
//!
//! ```rust,no_run
//! aurea::app::set_identity(aurea::identity!());
//! let folder = aurea::files::app_data_dir();
//! ```
//!
//! Documents the app is asked to open, from any platform, come through [`take_opened`].

use std::env;
use std::ffi::OsString;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod instance;
mod run;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use instance::single_instance;
pub use run::run;

#[cfg(target_os = "android")]
use crate::android::open_requested;
#[cfg(target_os = "ios")]
use crate::ios::{self, open_requested};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::{WindowEvent, registry::window::push_event_to_all_windows};

/// Who the app is, on every platform: the same name, id and version the iOS and Android
/// projects are built with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Display name, such as "Lunia Paint".
    pub name: &'static str,
    /// Reverse-DNS id, such as "dev.skuld.lunia.paint": bundle id, application id, and the name
    /// of the app's folders.
    pub id: &'static str,
    /// The organization's reverse-DNS id, shared by a suite of apps, if given.
    pub organization: Option<&'static str>,
    /// Version shown to people, such as "0.1.0"; empty when unknown.
    pub version: &'static str,
}

/// Says the work asked for by [`crate::WindowEvent::Suspending`] is done, so an iOS app needn't use
/// up its time. Nothing to do elsewhere.
pub fn suspend_done() {
    #[cfg(target_os = "ios")]
    ios::end_background();
}

/// The identity `aurea_build::identity()` read from the calling crate's `Cargo.toml`, or that
/// crate's own name and version when its build script did not run it.
#[macro_export]
macro_rules! identity {
    () => {
        $crate::app::Identity {
            name: match option_env!("AUREA_APP_NAME") {
                Some(name) => name,
                None => env!("CARGO_PKG_NAME"),
            },
            id: match option_env!("AUREA_APP_ID") {
                Some(id) => id,
                None => env!("CARGO_PKG_NAME"),
            },
            organization: option_env!("AUREA_APP_ORGANIZATION"),
            version: match option_env!("AUREA_APP_VERSION") {
                Some(version) => version,
                None => env!("CARGO_PKG_VERSION"),
            },
        }
    };
}

static IDENTITY: OnceLock<Identity> = OnceLock::new();

/// Sets the app's identity, once, early in `main`; false when it was set already.
pub fn set_identity(identity: Identity) -> bool {
    IDENTITY.set(identity).is_ok()
}

/// The app's identity: what [`set_identity`] was given, else the executable's name for both name
/// and id.
pub fn identity() -> Identity {
    *IDENTITY.get_or_init(|| {
        let exe = env::current_exe().ok();
        let stem = exe
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|stem| stem.to_str())
            .unwrap_or("app");
        let stem: &'static str = Box::leak(stem.to_owned().into_boxed_str());
        Identity {
            name: stem,
            id: stem,
            organization: None,
            version: "",
        }
    })
}

/// What the app was asked to open and has not taken yet; `None` until the arguments are read.
static OPENED: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);

/// The files the app was asked to open since the last call, whichever way they came: the command
/// line, a double click in Finder, or an iOS or Android open or share. Those two copy each file in
/// first, so the path is the app's own. Call it at start and on
/// [`crate::WindowEvent::OpenRequested`].
pub fn take_opened() -> Vec<PathBuf> {
    let mut opened = OPENED.lock().unwrap_or_else(PoisonError::into_inner);
    match opened.as_mut() {
        Some(paths) => mem::take(paths),
        None => {
            *opened = Some(Vec::new());
            files_in(env::args_os().skip(1))
        }
    }
}

/// The arguments that name files; anything else, such as a flag, is the app's to read.
fn files_in(args: impl Iterator<Item = OsString>) -> Vec<PathBuf> {
    args.map(PathBuf::from)
        .filter(|path| path.is_file())
        .collect()
}

/// Queues `paths` for [`take_opened`] and says so.
pub(crate) fn opened(paths: Vec<PathBuf>) {
    OPENED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(|| files_in(env::args_os().skip(1)))
        .extend(paths);
    #[cfg(target_os = "android")]
    open_requested();
    #[cfg(target_os = "ios")]
    open_requested();
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    push_event_to_all_windows(WindowEvent::OpenRequested);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_arguments_naming_files_are_opened() {
        let here = env::current_exe().expect("this test");
        let args = [
            "--flag".into(),
            here.clone().into_os_string(),
            "no such file".into(),
        ];
        assert_eq!(files_in(args.into_iter()), [here]);
    }

    #[test]
    fn the_macro_falls_back_to_the_package() {
        let identity = crate::identity!();
        assert_eq!(identity.name, "aurea");
        assert_eq!(identity.id, "aurea");
        assert_eq!(identity.version, env!("CARGO_PKG_VERSION"));
    }
}
