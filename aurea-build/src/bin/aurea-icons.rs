//! Writes an app's mobile icons from its master icon: `aurea-icons <Cargo.toml> android <res dir>`
//! or `aurea-icons <Cargo.toml> ios <AppIcon.appiconset dir>`. Does nothing without an `icon` key.

use aurea_build::{Identity, load_master, write_android, write_ios};
use std::env;
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
        return Err("usage: aurea-icons <Cargo.toml> android|ios <dir>".into());
    };
    let identity =
        Identity::from_manifest(Path::new(manifest)).map_err(|e| format!("{manifest}: {e}"))?;
    let Some(icon) = identity.icon else {
        return Ok(());
    };
    let master = load_master(&icon).map_err(|e| format!("{}: {e}", icon.display()))?;
    let out = Path::new(out);
    let written = match target.as_str() {
        "android" => write_android(&master, out),
        "ios" => write_ios(&master, out),
        other => return Err(format!("no target {other}, expected android or ios")),
    };
    written.map_err(|e| format!("{}: {e}", out.display()))
}
