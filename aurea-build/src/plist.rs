//! The macOS `Info.plist` of an app, embedded in its binary's `__TEXT,__info_plist` section.

use crate::Identity;
use std::env;
use std::fs;
use std::path::Path;

/// The `Info.plist` for `identity`, whose executable is `executable`.
pub fn info_plist(identity: &Identity, executable: &str) -> String {
    let rows = [
        ("CFBundleDisplayName", xml_escape(&identity.name)),
        ("CFBundleExecutable", xml_escape(executable)),
        ("CFBundleIdentifier", xml_escape(&identity.id)),
        ("CFBundleName", xml_escape(&identity.name)),
        ("CFBundleShortVersionString", xml_escape(&identity.version)),
        ("CFBundleVersion", xml_escape(&identity.version)),
    ];
    let strings: String = rows
        .iter()
        .map(|(key, value)| format!("\t<key>{key}</key>\n\t<string>{value}</string>\n"))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n{strings}\
\t<key>NSHighResolutionCapable</key>\n\t<true/>\n\
</dict>\n</plist>\n"
    )
}

/// Writes `Info.plist` to `out_dir` and links it into the app's binaries.
pub(crate) fn embed(identity: &Identity, out_dir: &Path) {
    let path = out_dir.join("Info.plist");
    let executable = env::var("CARGO_PKG_NAME").unwrap_or_default();
    fs::write(&path, info_plist(identity, &executable)).expect("write Info.plist");
    println!(
        "cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        path.display()
    );
}

/// `text` with the characters XML reserves replaced by their entities.
pub(crate) fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_xml_escaped() {
        let identity = Identity {
            name: "A & B <3".to_string(),
            id: "dev.test.ab".to_string(),
            organization: None,
            version: "1.0.0".to_string(),
            icon: None,
        };
        let plist = info_plist(&identity, "ab");
        assert!(plist.contains("<string>A &amp; B &lt;3</string>"));
        assert!(!plist.contains("A & B"));
        assert!(plist.contains("<key>CFBundleIdentifier</key>\n\t<string>dev.test.ab</string>"));
    }
}
