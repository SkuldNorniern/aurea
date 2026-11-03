//! The macOS `Info.plist` of an app, embedded in its binary's `__TEXT,__info_plist` section, and
//! the document types iOS needs in one too.

use crate::{Document, Identity};
use std::env;
use std::fs;
use std::path::Path;

/// The `Info.plist` for `identity`, whose executable is `executable`.
pub fn info(identity: &Identity, executable: &str) -> String {
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
{documents}</dict>\n</plist>\n",
        documents = document_rows(identity, Icons::Icns),
    )
}

/// A plist with only the document types, as a file Xcode merges through `INFOPLIST_FILE`, since
/// `INFOPLIST_KEY_*` can't hold arrays. Each icon goes in the app's resources as `<ext>.png`.
pub fn ios_documents(identity: &Identity) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n{documents}</dict>\n</plist>\n",
        documents = document_rows(identity, Icons::Png),
    )
}

/// How a platform names a document's icon.
#[derive(Clone, Copy)]
enum Icons {
    Icns,
    Png,
}

/// `CFBundleDocumentTypes` and `UTExportedTypeDeclarations`; nothing without documents.
fn document_rows(identity: &Identity, icons: Icons) -> String {
    if identity.documents.is_empty() {
        return String::new();
    }
    let mut types = String::new();
    let mut exported = String::new();
    for document in &identity.documents {
        let uti = xml_escape(&format!("{}.{}", identity.id, document.ext));
        let (type_icon, uti_icon) = icon_rows(document, icons);
        types.push_str(&format!(
            "\t\t<dict>\n\
\t\t\t<key>CFBundleTypeName</key>\n\t\t\t<string>{name}</string>\n\
\t\t\t<key>CFBundleTypeRole</key>\n\t\t\t<string>{role}</string>\n\
\t\t\t<key>LSHandlerRank</key>\n\t\t\t<string>Owner</string>\n\
\t\t\t<key>LSItemContentTypes</key>\n\t\t\t<array><string>{uti}</string></array>\n\
{type_icon}\t\t</dict>\n",
            name = xml_escape(&document.name),
            role = if document.editor { "Editor" } else { "Viewer" },
        ));
        exported.push_str(&format!(
            "\t\t<dict>\n\
\t\t\t<key>UTTypeIdentifier</key>\n\t\t\t<string>{uti}</string>\n\
\t\t\t<key>UTTypeDescription</key>\n\t\t\t<string>{name}</string>\n\
\t\t\t<key>UTTypeConformsTo</key>\n\t\t\t<array><string>public.data</string><string>public.content</string></array>\n\
\t\t\t<key>UTTypeTagSpecification</key>\n\t\t\t<dict>\n\
\t\t\t\t<key>public.filename-extension</key>\n\t\t\t\t<array><string>{ext}</string></array>\n\
\t\t\t\t<key>public.mime-type</key>\n\t\t\t\t<array><string>{mime}</string></array>\n\
\t\t\t</dict>\n\
{uti_icon}\t\t</dict>\n",
            name = xml_escape(&document.name),
            ext = xml_escape(&document.ext),
            mime = xml_escape(&document.mime),
        ));
    }
    format!(
        "\t<key>CFBundleDocumentTypes</key>\n\t<array>\n{types}\t</array>\n\
\t<key>UTExportedTypeDeclarations</key>\n\t<array>\n{exported}\t</array>\n"
    )
}

/// The rows naming `document`'s icon in the document type and in the exported type.
fn icon_rows(document: &Document, icons: Icons) -> (String, String) {
    if document.icon.is_none() {
        return (String::new(), String::new());
    }
    let ext = xml_escape(&document.ext);
    match icons {
        Icons::Icns => (
            format!("\t\t\t<key>CFBundleTypeIconFile</key>\n\t\t\t<string>{ext}.icns</string>\n"),
            format!("\t\t\t<key>UTTypeIconFile</key>\n\t\t\t<string>{ext}.icns</string>\n"),
        ),
        Icons::Png => (
            format!(
                "\t\t\t<key>CFBundleTypeIconFiles</key>\n\t\t\t<array><string>{ext}.png</string></array>\n"
            ),
            format!(
                "\t\t\t<key>UTTypeIconFiles</key>\n\t\t\t<array><string>{ext}.png</string></array>\n"
            ),
        ),
    }
}

/// Writes `Info.plist` to `out_dir` and links it into the app's binaries.
pub(crate) fn embed(identity: &Identity, out_dir: &Path) {
    let path = out_dir.join("Info.plist");
    let executable = env::var("CARGO_PKG_NAME").unwrap_or_default();
    fs::write(&path, info(identity, &executable)).expect("write Info.plist");
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
            package: "ab".to_string(),
            documents: Vec::new(),
        };
        let plist = info(&identity, "ab");
        assert!(!plist.contains("CFBundleDocumentTypes"));
        assert!(plist.contains("<string>A &amp; B &lt;3</string>"));
        assert!(!plist.contains("A & B"));
        assert!(plist.contains("<key>CFBundleIdentifier</key>\n\t<string>dev.test.ab</string>"));
    }

    fn with_a_document() -> Identity {
        Identity {
            name: "App".to_string(),
            id: "dev.test.app".to_string(),
            organization: None,
            version: "1.0.0".to_string(),
            icon: None,
            package: "app".to_string(),
            documents: vec![Document {
                ext: "lp".to_string(),
                name: "A & B".to_string(),
                mime: "application/vnd.test.lp".to_string(),
                icon: Some("brand/lp".into()),
                editor: true,
            }],
        }
    }

    #[test]
    fn documents_are_declared_and_exported() {
        let plist = info(&with_a_document(), "app");
        assert!(plist.contains("<key>CFBundleDocumentTypes</key>"));
        assert!(plist.contains("<key>UTExportedTypeDeclarations</key>"));
        assert!(plist.contains("<string>A &amp; B</string>"));
        assert!(plist.contains("<string>Editor</string>"));
        assert_eq!(plist.matches("<string>dev.test.app.lp</string>").count(), 2);
        assert!(plist.contains("<array><string>lp</string></array>"));
        assert!(plist.contains("<array><string>application/vnd.test.lp</string></array>"));
        assert_eq!(plist.matches("<string>lp.icns</string>").count(), 2);
    }

    #[test]
    fn ios_gets_only_the_document_types_with_png_icons() {
        let plist = ios_documents(&with_a_document());
        assert!(plist.contains("<key>CFBundleDocumentTypes</key>"));
        assert!(!plist.contains("CFBundleExecutable"));
        assert!(plist.contains("<array><string>lp.png</string></array>"));
        assert!(!plist.contains("icns"));
    }
}
