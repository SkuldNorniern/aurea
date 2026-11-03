//! Writes an app's mobile icons from its master icon: `aurea-icons <Cargo.toml> android <res dir>`
//! or `aurea-icons <Cargo.toml> ios <AppIcon.appiconset dir>`, which do nothing without an `icon`
//! key. `linux <root>` writes the app's `share` folder, and `ios-documents <file>` the document
//! types Xcode merges through `INFOPLIST_FILE`.

use aurea_build::{
    Identity, ios_documents_plist, load_master, write_android, write_ios, write_linux,
};
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("aurea-icons: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let [manifest, target, out] = args else {
        return Err("usage: aurea-icons <Cargo.toml> android|ios|linux|ios-documents <out>".into());
    };
    let identity =
        Identity::from_manifest(Path::new(manifest)).map_err(|e| format!("{manifest}: {e}"))?;
    match target.as_str() {
        "linux" => {
            return write_linux(&identity, Path::new(out)).map_err(|e| format!("{out}: {e}"));
        }
        "ios-documents" => {
            return fs::write(out, ios_documents_plist(&identity))
                .map_err(|e| format!("{out}: {e}"));
        }
        _ => {}
    }
    let Some(icon) = identity.icon else {
        return Ok(());
    };
    let master = load_master(&icon).map_err(|e| format!("{}: {e}", icon.display()))?;
    let out = Path::new(out);
    let written = match target.as_str() {
        "android" => write_android(&master, out),
        "ios" => write_ios(&master, out),
        other => return Err(format!("no target {other}")),
    };
    written.map_err(|e| format!("{}: {e}", out.display()))
}
