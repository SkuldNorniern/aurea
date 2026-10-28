//! Reads an Aurea app's identity from its `Cargo.toml` and hands it to the app's code. Call
//! [`identity`] from the app's `build.rs`, and `aurea::identity!()` in the app picks it up.
//!
//! ```toml
//! [package.metadata.aurea]
//! name = "Lunia Paint"           # display name on every platform
//! id = "dev.skuld.lunia.paint"   # bundle id, application id, app data folder
//! organization = "dev.skuld"     # shared folders across a suite of apps
//! icon = "../brand/lunia-paint"  # square master PNG, relative to Cargo.toml
//! version = "auto"               # "auto" or missing: the package version
//! ```
//!
//! Missing keys fall back to the package: its name for `name` and `id`, its version for
//! `version`.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod icons;
mod plist;
mod winres;

pub use icons::{Image, icns, ico, load_master, png, resize, write_android, write_ios};
pub use plist::info_plist;

/// An app's identity, as `[package.metadata.aurea]` gives it, with the package's own name and
/// version standing in for what it leaves out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Display name.
    pub name: String,
    /// Reverse-DNS id: bundle id, application id, and the app's data folder.
    pub id: String,
    /// The organization's own reverse-DNS id, if given.
    pub organization: Option<String>,
    /// Version shown to people, such as `0.1.0`.
    pub version: String,
    /// The master icon, made absolute, if given.
    pub icon: Option<PathBuf>,
}

impl Identity {
    /// The identity in `manifest`, the text of the `Cargo.toml` in `dir`, for a package named
    /// `package` at `version`.
    pub fn parse(manifest: &str, dir: &Path, package: &str, version: &str) -> Self {
        let table = aurea_table(manifest, "[package.metadata.aurea]");
        let get = |key: &str| {
            table
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .filter(|v| !v.is_empty())
        };
        Self {
            name: get("name").unwrap_or_else(|| package.to_string()),
            id: get("id").unwrap_or_else(|| package.to_string()),
            organization: get("organization"),
            version: get("version")
                .filter(|v| v != "auto")
                .unwrap_or_else(|| version.to_string()),
            icon: get("icon").map(|icon| dir.join(icon)),
        }
    }

    /// The identity of the package whose manifest is at `path`. Its icon is relative to `path`.
    /// `version` is empty if the package inherits it from the workspace.
    pub fn from_manifest(path: &Path) -> io::Result<Self> {
        let manifest = fs::read_to_string(path)?;
        Ok(Self::from_text(
            &manifest,
            path.parent().unwrap_or(Path::new(".")),
        ))
    }

    fn from_text(manifest: &str, dir: &Path) -> Self {
        let package = aurea_table(manifest, "[package]");
        let field = |key: &str| {
            package
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        Self::parse(manifest, dir, &field("name"), &field("version"))
    }

    /// The identity of the package whose build script is running.
    ///
    /// # Panics
    ///
    /// Outside a build script, where Cargo's `CARGO_MANIFEST_DIR` is not set.
    pub fn of_package() -> Self {
        let dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("run from build.rs"));
        let manifest = fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
        let package = env::var("CARGO_PKG_NAME").unwrap_or_default();
        let version = env::var("CARGO_PKG_VERSION").unwrap_or_default();
        Self::parse(&manifest, &dir, &package, &version)
    }
}

/// From `build.rs`: reads `[package.metadata.aurea]` and sets `AUREA_APP_NAME`, `AUREA_APP_ID`,
/// `AUREA_APP_ORGANIZATION`, `AUREA_APP_VERSION` and `AUREA_APP_ICON` for the package's code,
/// which `aurea::identity!()` reads.
///
/// ```no_run
/// // in build.rs's `main`
/// aurea_build::identity();
/// ```
pub fn identity() -> Identity {
    let identity = Identity::of_package();
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!(
        "cargo:rustc-env=AUREA_APP_NAME={}",
        one_line(&identity.name)
    );
    println!("cargo:rustc-env=AUREA_APP_ID={}", one_line(&identity.id));
    println!(
        "cargo:rustc-env=AUREA_APP_VERSION={}",
        one_line(&identity.version)
    );
    if let Some(organization) = &identity.organization {
        println!(
            "cargo:rustc-env=AUREA_APP_ORGANIZATION={}",
            one_line(organization)
        );
    }
    if let Some(icon) = &identity.icon {
        println!(
            "cargo:rustc-env=AUREA_APP_ICON={}",
            one_line(&icon.to_string_lossy())
        );
        println!("cargo:rerun-if-changed={}", icon.display());
    }
    embed(&identity);
    identity
}

/// Embeds `identity` in the binary: a version resource and manifest on Windows MSVC, an
/// `Info.plist` section on macOS.
fn embed(identity: &Identity) {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let master = identity
        .icon
        .as_deref()
        .and_then(|path| match load_master(path) {
            Ok(image) => Some(image),
            Err(err) => {
                println!("cargo:warning=icon {}: {err}", path.display());
                None
            }
        });
    match (target_os.as_str(), target_env.as_str()) {
        ("windows", "msvc") => {
            let ico = master.as_ref().map(ico);
            winres::embed(identity, ico.as_deref(), true, &out);
        }
        ("macos", _) => {
            plist::embed(identity, &out);
            if let Some(master) = &master {
                let _ = fs::write(out.join("AppIcon.icns"), icns(master));
            }
        }
        _ => {}
    }
}

/// `value` with line breaks turned to spaces: Cargo reads one instruction per line.
fn one_line(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

/// The keys and values of the `section` table, such as `[package]`, in `manifest`. Strings lose
/// their quotes; anything else is kept as written.
fn aurea_table(manifest: &str, section: &str) -> Vec<(String, String)> {
    let mut inside = false;
    let mut table = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            let header: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            inside = header == section;
            continue;
        }
        if !inside {
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches('"').to_string();
        if key.is_empty() || key.starts_with('#') {
            continue;
        }
        if let Some(value) = value_of(rest.trim()) {
            table.push((key, value));
        }
    }
    table
}

/// A TOML value at the start of `text`: a basic or literal string, or a bare word up to a comment.
fn value_of(text: &str) -> Option<String> {
    let mut chars = text.chars();
    match chars.next()? {
        '"' => {
            let mut out = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '"' => return Some(out),
                    '\\' => match chars.next()? {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        other => out.push(other),
                    },
                    c => out.push(c),
                }
            }
            None
        }
        '\'' => text[1..]
            .split_once('\'')
            .map(|(value, _)| value.to_string()),
        _ => Some(text.split('#').next().unwrap_or("").trim().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
[package]
name = "lunia-paint"
version = "0.1.0"

[package.metadata.aurea]
name = "Lunia Paint"            # display name
id = "dev.skuld.lunia.paint"
organization = 'dev.skuld'
icon = "../brand/lunia-paint"
version = "auto"

[dependencies]
name = "not this"
"#;

    #[test]
    fn the_table_names_the_app_and_the_package_fills_the_rest() {
        let dir = Path::new("/apps/paint");
        let id = Identity::parse(MANIFEST, dir, "lunia-paint", "0.1.0");
        assert_eq!(id.name, "Lunia Paint");
        assert_eq!(id.id, "dev.skuld.lunia.paint");
        assert_eq!(id.organization.as_deref(), Some("dev.skuld"));
        assert_eq!(id.version, "0.1.0");
        assert_eq!(id.icon, Some(dir.join("../brand/lunia-paint")));
    }

    #[test]
    fn without_a_table_the_package_is_the_app() {
        let id = Identity::parse("[package]\nname = \"x\"\n", Path::new("."), "tool", "2.3.4");
        assert_eq!((id.name.as_str(), id.id.as_str()), ("tool", "tool"));
        assert_eq!((id.version.as_str(), id.organization), ("2.3.4", None));
    }

    #[test]
    fn the_package_table_gives_name_and_version() {
        let id = Identity::from_text(MANIFEST, Path::new("/apps/paint"));
        assert_eq!(id.name, "Lunia Paint");
        assert_eq!(id.version, "0.1.0");
        let inherited = Identity::from_text(
            "[package]\nname = \"p\"\nversion.workspace = true\n",
            Path::new("."),
        );
        assert_eq!(
            (inherited.name.as_str(), inherited.version.as_str()),
            ("p", "")
        );
    }

    #[test]
    fn values_keep_escapes_hashes_and_explicit_versions() {
        let manifest =
            "[ package.metadata.aurea ]\nname = \"A \\\"B\\\" # C\"\nversion = \"1.2\"\n";
        let id = Identity::parse(manifest, Path::new("."), "p", "0.0.1");
        assert_eq!(id.name, "A \"B\" # C");
        assert_eq!(id.version, "1.2");
    }
}
