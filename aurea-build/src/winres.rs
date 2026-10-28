//! The Windows resources of an app's `.exe`: a version block, an application manifest and an icon.
//! They are written straight into a `.res` file, which `link.exe` takes as an input, so no
//! `rc.exe` is needed.

use crate::Identity;
use crate::plist::xml_escape;
use std::fs;
use std::path::Path;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
const RT_MANIFEST: u16 = 24;
const LANG_EN_US: u16 = 0x0409;
const FLAGS_ICON: u16 = 0x1030;
const FLAGS_PLAIN: u16 = 0x0030;
const VS_FIXED_SIGNATURE: u32 = 0xFEEF_04BD;
const VS_FIXED_LEN: usize = 52;
const UNICODE_CODE_PAGE: u16 = 1200;

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="APP_ID" version="1.0.0.0" processorArchitecture="*"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
    </windowsSettings>
  </application>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
"#;

/// Writes `aurea.res` to `out_dir` and links it into the app's binaries. The resources carry
/// `identity`, a manifest when `manifest` is set, and the images of `icon_ico` when given.
pub(crate) fn embed(identity: &Identity, icon_ico: Option<&[u8]>, manifest: bool, out_dir: &Path) {
    let mut entries = vec![Entry {
        kind: RT_VERSION,
        id: 1,
        flags: FLAGS_PLAIN,
        data: version_info(identity),
    }];
    if manifest {
        entries.push(Entry {
            kind: RT_MANIFEST,
            id: 1,
            flags: FLAGS_PLAIN,
            data: MANIFEST
                .replace("APP_ID", &xml_escape(&identity.id))
                .into_bytes(),
        });
    }
    if let Some(ico) = icon_ico {
        entries.extend(icon_entries(ico).expect("icon is a valid .ico file"));
    }
    let path = out_dir.join("aurea.res");
    fs::write(&path, res_file(&entries)).expect("write aurea.res");
    println!("cargo:rustc-link-arg-bins={}", path.display());
}

/// One resource: its type, id, memory flags and data. The language is always `LANG_EN_US`.
struct Entry {
    kind: u16,
    id: u16,
    flags: u16,
    data: Vec<u8>,
}

/// The `.res` file holding `entries`, starting with the empty entry every such file begins with.
fn res_file(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    push_entry(&mut out, 0, 0, 0, 0, &[]);
    for entry in entries {
        push_entry(
            &mut out,
            entry.kind,
            entry.id,
            entry.flags,
            LANG_EN_US,
            &entry.data,
        );
    }
    out
}

/// One `.res` entry with numeric type and name: a 32-byte header, then the data padded to 4.
fn push_entry(out: &mut Vec<u8>, kind: u16, id: u16, flags: u16, lang: u16, data: &[u8]) {
    let size = u32::try_from(data.len()).expect("resource under 4 GiB");
    out.extend(size.to_le_bytes());
    out.extend(32u32.to_le_bytes());
    out.extend(0xFFFFu16.to_le_bytes());
    out.extend(kind.to_le_bytes());
    out.extend(0xFFFFu16.to_le_bytes());
    out.extend(id.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(flags.to_le_bytes());
    out.extend(lang.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(data);
    pad4(out);
}

/// `out` padded with zeros to a multiple of 4 bytes.
fn pad4(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// `text` as UTF-16LE with its terminating zero.
fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// `version` as the four numbers Windows keeps: up to four dot-separated parts, missing parts
/// zero, and anything from a `-` or `+` on dropped.
fn version_numbers(version: &str) -> [u16; 4] {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let mut out = [0u16; 4];
    for (slot, part) in out.iter_mut().zip(core.split('.')) {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        *slot = if digits.is_empty() {
            0
        } else {
            digits.parse().unwrap_or(u16::MAX)
        };
    }
    out
}

/// A `VS_VERSIONINFO`-style block: its length, its value's length, its type, its key, its value,
/// then its children. Every part is padded to 4 bytes, and the length includes the padding.
/// `value_len` counts bytes for a binary value and UTF-16 words for a text one.
fn block(key: &str, text: bool, value_len: usize, value: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0u8; 6];
    out.extend(wide(key));
    pad4(&mut out);
    out.extend(value);
    pad4(&mut out);
    for child in children {
        out.extend(child);
    }
    pad4(&mut out);
    let len = u16::try_from(out.len()).expect("version block under 64 KiB");
    let value_len = u16::try_from(value_len).expect("version value under 64 KiB");
    out[0..2].copy_from_slice(&len.to_le_bytes());
    out[2..4].copy_from_slice(&value_len.to_le_bytes());
    out[4..6].copy_from_slice(&u16::from(text).to_le_bytes());
    out
}

/// The `VS_FIXEDFILEINFO` of a file at `version` on Windows NT.
fn fixed_info(version: [u16; 4]) -> Vec<u8> {
    let most = (u32::from(version[0]) << 16) | u32::from(version[1]);
    let least = (u32::from(version[2]) << 16) | u32::from(version[3]);
    let words = [
        VS_FIXED_SIGNATURE,
        0x0001_0000,
        most,
        least,
        most,
        least,
        0x3F,
        0,
        0x0004_0004,
        1,
        0,
        0,
        0,
    ];
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

/// The `RT_VERSION` data for `identity`.
fn version_info(identity: &Identity) -> Vec<u8> {
    let mut pairs = Vec::new();
    if let Some(company) = identity.organization.as_deref().filter(|c| !c.is_empty()) {
        pairs.push(("CompanyName", company));
    }
    pairs.extend([
        ("FileDescription", identity.name.as_str()),
        ("FileVersion", identity.version.as_str()),
        ("InternalName", identity.id.as_str()),
        ("ProductName", identity.name.as_str()),
        ("ProductVersion", identity.version.as_str()),
    ]);
    let strings: Vec<Vec<u8>> = pairs
        .iter()
        .map(|(key, value)| {
            let value = wide(value);
            block(key, true, value.len() / 2, &value, &[])
        })
        .collect();
    let table = block("040904B0", true, 0, &[], &strings);
    let string_file_info = block("StringFileInfo", true, 0, &[], &[table]);

    let code_page: Vec<u8> = [LANG_EN_US, UNICODE_CODE_PAGE]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    let translation = block("Translation", false, code_page.len(), &code_page, &[]);
    let var_file_info = block("VarFileInfo", true, 0, &[], &[translation]);

    let fixed = fixed_info(version_numbers(&identity.version));
    block(
        "VS_VERSION_INFO",
        false,
        VS_FIXED_LEN,
        &fixed,
        &[string_file_info, var_file_info],
    )
}

/// The `RT_ICON` images of the `.ico` file `ico`, and the `RT_GROUP_ICON` directory that names them
/// by id (1 onwards) in place of file offsets.
fn icon_entries(ico: &[u8]) -> Result<Vec<Entry>, String> {
    let truncated = || "truncated .ico file".to_string();
    let count = le_u16(ico, 4).ok_or_else(truncated)?;
    let mut group = vec![0, 0, 1, 0];
    group.extend(count.to_le_bytes());
    let mut entries = Vec::new();
    for index in 0..usize::from(count) {
        let at = 6 + index * 16;
        let dir = ico.get(at..at + 16).ok_or_else(truncated)?;
        let size =
            usize::try_from(le_u32(dir, 8).ok_or_else(truncated)?).map_err(|_| truncated())?;
        let offset =
            usize::try_from(le_u32(dir, 12).ok_or_else(truncated)?).map_err(|_| truncated())?;
        let image = ico
            .get(offset..offset + size)
            .ok_or_else(truncated)?
            .to_vec();
        let id = u16::try_from(index + 1).map_err(|_| "too many icon images".to_string())?;
        group.extend(&dir[..12]);
        group.extend(id.to_le_bytes());
        entries.push(Entry {
            kind: RT_ICON,
            id,
            flags: FLAGS_ICON,
            data: image,
        });
    }
    if count == 0 {
        return Err("empty .ico file".to_string());
    }
    entries.push(Entry {
        kind: RT_GROUP_ICON,
        id: 1,
        flags: FLAGS_ICON,
        data: group,
    });
    Ok(entries)
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(version: &str) -> Identity {
        Identity {
            name: "Res Test".to_string(),
            id: "dev.test.res".to_string(),
            organization: Some("dev.test".to_string()),
            version: version.to_string(),
            icon: None,
        }
    }

    /// The `(offset, length)` of every entry in the `.res` file `res`, checking each header.
    fn entries(res: &[u8]) -> Vec<(usize, usize)> {
        let mut found = Vec::new();
        let mut at = 0;
        while at < res.len() {
            assert_eq!(at % 4, 0, "entry at {at} is not 4-aligned");
            let size = usize::try_from(le_u32(res, at).expect("data size")).expect("size fits");
            assert_eq!(le_u32(res, at + 4), Some(32), "header size at {at}");
            found.push((at, size));
            at = (at + 32 + size).next_multiple_of(4);
        }
        assert_eq!(at, res.len());
        found
    }

    #[test]
    fn version_numbers_keep_up_to_four_numeric_parts() {
        assert_eq!(version_numbers("1.2.3"), [1, 2, 3, 0]);
        assert_eq!(version_numbers("0.1.0-beta"), [0, 1, 0, 0]);
        assert_eq!(version_numbers("1.2.3.4.5"), [1, 2, 3, 4]);
        assert_eq!(version_numbers("2"), [2, 0, 0, 0]);
        assert_eq!(version_numbers("1.2+build.7"), [1, 2, 0, 0]);
    }

    #[test]
    fn res_starts_with_the_empty_entry() {
        let res = res_file(&[Entry {
            kind: RT_VERSION,
            id: 1,
            flags: FLAGS_PLAIN,
            data: vec![1, 2, 3],
        }]);
        let mut empty = vec![0, 0, 0, 0, 32, 0, 0, 0, 0xFF, 0xFF, 0, 0, 0xFF, 0xFF, 0, 0];
        empty.extend([0u8; 16]);
        assert_eq!(&res[..32], &empty[..]);
        assert_eq!(entries(&res), vec![(0, 0), (32, 3)]);
    }

    #[test]
    fn every_entry_is_four_aligned() {
        let ico = two_image_ico();
        let mut all = vec![Entry {
            kind: RT_VERSION,
            id: 1,
            flags: FLAGS_PLAIN,
            data: version_info(&identity("1.2.3")),
        }];
        all.push(Entry {
            kind: RT_MANIFEST,
            id: 1,
            flags: FLAGS_PLAIN,
            data: MANIFEST.as_bytes().to_vec(),
        });
        all.extend(icon_entries(&ico).expect("valid ico"));
        let res = res_file(&all);
        assert_eq!(entries(&res).len(), all.len() + 1);
    }

    #[test]
    fn version_info_puts_the_fixed_info_at_an_aligned_offset() {
        let info = version_info(&identity("1.2.3"));
        assert_eq!(
            le_u16(&info, 0),
            Some(u16::try_from(info.len()).expect("small"))
        );
        assert_eq!(
            le_u16(&info, 2),
            Some(u16::try_from(VS_FIXED_LEN).expect("small"))
        );
        assert_eq!(le_u32(&info, 40), Some(VS_FIXED_SIGNATURE));
        assert_eq!(le_u32(&info, 44), Some(0x0001_0000));
    }

    #[test]
    fn version_strings_are_utf16_with_the_company_when_given() {
        let info = version_info(&identity("1.2.3"));
        let company = wide("CompanyName");
        assert!(info.windows(company.len()).any(|w| w == company));
        let mut anonymous = identity("1.2.3");
        anonymous.organization = Some(String::new());
        let info = version_info(&anonymous);
        assert!(!info.windows(company.len()).any(|w| w == company));
    }

    #[test]
    fn icon_group_names_images_by_id() -> Result<(), String> {
        let entries = icon_entries(&two_image_ico())?;
        assert_eq!(entries.len(), 3);
        let group = &entries[2];
        assert_eq!((group.kind, group.id), (RT_GROUP_ICON, 1));
        assert_eq!(group.data.len(), 6 + 2 * 14);
        assert_eq!(&group.data[..6], &[0, 0, 1, 0, 2, 0]);
        assert_eq!(&group.data[18..20], &[1, 0]);
        assert_eq!(&group.data[32..34], &[2, 0]);
        assert_eq!((entries[0].kind, entries[0].id), (RT_ICON, 1));
        assert_eq!(entries[1].data, vec![0xBB; 5]);
        Ok(())
    }

    #[test]
    fn a_truncated_icon_is_an_error() {
        let mut ico = two_image_ico();
        ico.truncate(30);
        assert!(icon_entries(&ico).is_err());
    }

    /// An `.ico` with two images: 4 bytes of `0xAA` and 5 bytes of `0xBB`.
    fn two_image_ico() -> Vec<u8> {
        let mut ico = vec![0, 0, 1, 0, 2, 0];
        let images = [(0xAA, 4u32, 38u32), (0xBB, 5, 42)];
        for (_, size, offset) in images {
            ico.extend([16, 16, 0, 0, 1, 0, 32, 0]);
            ico.extend(size.to_le_bytes());
            ico.extend(offset.to_le_bytes());
        }
        ico.extend([0xAAu8; 4]);
        ico.extend([0xBBu8; 5]);
        ico
    }
}
