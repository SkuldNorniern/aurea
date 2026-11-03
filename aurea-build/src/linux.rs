//! A Linux app's desktop entry, MIME types and icons, under a `share` folder for packaging.

use crate::Identity;
use crate::icons::{load_master, png, resize};
use crate::plist::xml_escape;
use std::fs;
use std::io;
use std::path::Path;

const ICON_SIZES: [u32; 4] = [48, 64, 128, 256];

/// Writes `share/applications/<id>.desktop`, `share/mime/packages/<id>.xml` and the icons under
/// `share/icons/hicolor` into `root`. A package then runs `update-mime-database` and
/// `update-desktop-database` on them.
pub fn share(identity: &Identity, root: &Path) -> io::Result<()> {
    let share = root.join("share");
    let applications = share.join("applications");
    fs::create_dir_all(&applications)?;
    fs::write(
        applications.join(format!("{}.desktop", identity.id)),
        desktop_entry(identity),
    )?;
    if !identity.documents.is_empty() {
        let packages = share.join("mime/packages");
        fs::create_dir_all(&packages)?;
        fs::write(
            packages.join(format!("{}.xml", identity.id)),
            mime_info(identity),
        )?;
    }
    if let Some(icon) = &identity.icon {
        write_icons(icon, &share, "apps", &identity.id)?;
    }
    for document in &identity.documents {
        if let Some(icon) = &document.icon {
            write_icons(icon, &share, "mimetypes", &icon_name(&document.mime))?;
        }
    }
    Ok(())
}

/// The name icon themes look a MIME type's icon up by.
fn icon_name(mime: &str) -> String {
    mime.replace('/', "-")
}

fn write_icons(master: &Path, share: &Path, context: &str, name: &str) -> io::Result<()> {
    let master = load_master(master)?;
    for size in ICON_SIZES {
        let dir = share.join(format!("icons/hicolor/{size}x{size}/{context}"));
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(format!("{name}.png")), png(&resize(&master, size)))?;
    }
    Ok(())
}

fn desktop_entry(identity: &Identity) -> String {
    let mut entry = format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec={} %f\nTerminal=false\n",
        one_value(&identity.name),
        one_value(&identity.package),
    );
    if identity.icon.is_some() {
        entry.push_str(&format!("Icon={}\n", one_value(&identity.id)));
    }
    if !identity.documents.is_empty() {
        let types: String = identity
            .documents
            .iter()
            .map(|document| format!("{};", one_value(&document.mime)))
            .collect();
        entry.push_str(&format!("MimeType={types}\n"));
    }
    entry
}

/// `value` as a desktop entry value holds it, on one line.
fn one_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace(['\n', '\r'], " ")
}

fn mime_info(identity: &Identity) -> String {
    let types: String = identity
        .documents
        .iter()
        .map(|document| {
            let icon = if document.icon.is_some() {
                format!("    <icon name=\"{}\"/>\n", xml_escape(&icon_name(&document.mime)))
            } else {
                String::new()
            };
            format!(
                "  <mime-type type=\"{mime}\">\n    <comment>{name}</comment>\n{icon}    <glob pattern=\"*.{ext}\"/>\n  </mime-type>\n",
                mime = xml_escape(&document.mime),
                name = xml_escape(&document.name),
                ext = xml_escape(&document.ext),
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mime-info xmlns=\"http://www.freedesktop.org/standards/shared-mime-info\">\n{types}</mime-info>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use std::{env, process};

    fn app() -> Identity {
        Identity {
            name: "Lunia Paint".to_string(),
            id: "dev.test.paint".to_string(),
            organization: None,
            version: "1.0.0".to_string(),
            icon: None,
            package: "lunia-paint".to_string(),
            documents: vec![Document {
                ext: "lpaint".to_string(),
                name: "Paint & Co".to_string(),
                mime: "application/vnd.test.paint".to_string(),
                icon: Some("brand/lpaint".into()),
                editor: true,
            }],
        }
    }

    #[test]
    fn the_entry_runs_the_package_on_a_file_of_its_types() {
        let entry = desktop_entry(&app());
        assert!(entry.contains("Name=Lunia Paint\n"));
        assert!(entry.contains("Exec=lunia-paint %f\n"));
        assert!(entry.contains("MimeType=application/vnd.test.paint;\n"));
        assert!(!entry.contains("Icon="), "no app icon");
    }

    #[test]
    fn mime_info_globs_and_names_the_icon() {
        let info = mime_info(&app());
        assert!(info.contains("<mime-type type=\"application/vnd.test.paint\">"));
        assert!(info.contains("<glob pattern=\"*.lpaint\"/>"));
        assert!(info.contains("<comment>Paint &amp; Co</comment>"));
        assert!(info.contains("<icon name=\"application-vnd.test.paint\"/>"));
    }

    #[test]
    fn it_writes_the_tree() {
        let root = env::temp_dir().join(format!("aurea-linux-{}", process::id()));
        let mut identity = app();
        identity.documents[0].icon = None;
        share(&identity, &root).expect("write");
        let entry = root.join("share/applications/dev.test.paint.desktop");
        assert!(entry.is_file());
        assert!(
            root.join("share/mime/packages/dev.test.paint.xml")
                .is_file()
        );
        fs::remove_dir_all(&root).expect("clean");
    }
}
