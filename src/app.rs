//! The app's identity: its name, id, organization and version, from the app crate's
//! `[package.metadata.aurea]` table. The app's `build.rs` calls `aurea_build::identity()`, and
//! its `main` hands [`identity!`](crate::identity) to [`set_identity`] before anything reads it:
//!
//! ```rust,no_run
//! aurea::app::set_identity(aurea::identity!());
//! let folder = aurea::files::app_data_dir();
//! ```

use std::env;
use std::path::Path;
use std::sync::OnceLock;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_macro_falls_back_to_the_package() {
        let identity = crate::identity!();
        assert_eq!(identity.name, "aurea");
        assert_eq!(identity.id, "aurea");
        assert_eq!(identity.version, env!("CARGO_PKG_VERSION"));
    }
}
