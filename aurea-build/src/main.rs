//! Writes what an app's packaging needs from its `Cargo.toml`, for scripts that run outside cargo's
//! build:
//!
//! ```text
//! aurea-build <Cargo.toml> icons android <res dir>
//! aurea-build <Cargo.toml> icons ios <AppIcon.appiconset dir>
//! aurea-build <Cargo.toml> documents ios <file>
//! aurea-build <Cargo.toml> share linux <root>
//! ```
//!
//! `icons` does nothing without an `icon` key.

use aurea_build::{Identity, icons, linux, plist};
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: aurea-build <Cargo.toml> icons android|ios <out>\n       \
                     aurea-build <Cargo.toml> documents ios <out>\n       \
                     aurea-build <Cargo.toml> share linux <out>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("aurea-build: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let [manifest, what, platform, out] = args else {
        return Err(USAGE.into());
    };
    let identity =
        Identity::from_manifest(Path::new(manifest)).map_err(|e| format!("{manifest}: {e}"))?;
    let path = Path::new(out);
    let written = match (what.as_str(), platform.as_str()) {
        ("icons", "android") => write_icons(&identity, |icon| icons::android(icon, path)),
        ("icons", "ios") => write_icons(&identity, |icon| icons::ios(icon, path)),
        ("documents", "ios") => fs::write(path, plist::ios_documents(&identity)),
        ("share", "linux") => linux::share(&identity, path),
        _ => return Err(USAGE.into()),
    };
    written.map_err(|e| format!("{out}: {e}"))
}

fn write_icons(
    identity: &Identity,
    write: impl FnOnce(&icons::Image) -> io::Result<()>,
) -> io::Result<()> {
    match &identity.icon {
        Some(icon) => write(&icons::load_master(icon)?),
        None => Ok(()),
    }
}
