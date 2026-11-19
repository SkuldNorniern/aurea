//! Installed fonts: the font folders walked once, the faces in them read when a family is first
//! asked for by name, and the generic and fallback families.

#[cfg(not(target_os = "android"))]
use std::env::var;
#[cfg(target_os = "windows")]
use std::env::var_os;
use std::fs::read_dir;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use super::{Located, file_bytes};
use crate::text::fonts::{FaceInfo, normalise};
use crate::text::platform::FontRef;
use crate::types::{FontStyle, FontWeight};

#[cfg(target_os = "android")]
fn font_search_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/system/fonts"),
        PathBuf::from("/product/fonts"),
        PathBuf::from("/system_ext/fonts"),
    ]
}

#[cfg(not(target_os = "android"))]
fn font_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
        dirs.push(PathBuf::from(
            "/System/Library/PrivateFrameworks/FontServices.framework/Versions/A/Resources/Fonts",
        ));
        // Fonts the system downloads, in folders named with a version.
        if let Ok(assets) = read_dir("/System/Library/AssetsV2") {
            dirs.extend(
                assets
                    .flatten()
                    .filter(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("com_apple_MobileAsset_Font")
                    })
                    .map(|entry| entry.path()),
            );
        }
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = var("HOME") {
            dirs.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(root) = var_os("SYSTEMROOT") {
            dirs.push(PathBuf::from(root).join("Fonts"));
        } else {
            dirs.push(PathBuf::from("C:\\Windows\\Fonts"));
        }
        if let Ok(profile) = var("USERPROFILE") {
            let home = PathBuf::from(profile);
            dirs.push(home.join("AppData\\Local\\Microsoft\\Windows\\Fonts"));
            dirs.push(home.join("AppData\\Roaming\\Microsoft\\Windows\\Fonts"));
        }
    }

    #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = var("HOME") {
            let h = PathBuf::from(home);
            dirs.push(h.join(".fonts"));
            dirs.push(h.join(".local/share/fonts"));
        }
    }

    dirs
}

fn fallback_stems() -> &'static [&'static str] {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        &[
            "sfns",
            "sfui",
            "helveticaneue",
            "helvetica",
            "applesdgothicneo",
            "pingfang",
            "hiraginosansgb",
            "sfarabic",
            "geezapro",
            "sfhebrew",
            "arialhb",
            "thonburiui",
            "kohinoor",
            "menlo",
        ]
    }
    #[cfg(target_os = "windows")]
    {
        &["segoeui", "arial", "malgun", "msyh", "yugothr", "consola"]
    }
    #[cfg(target_os = "android")]
    {
        &[
            "robotostaticregular",
            "robotoregular",
            "droidsans",
            "notosanscjkregular",
            "droidsansmono",
        ]
    }
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    {
        &[
            "dejavusans",
            "liberationsansregular",
            "notosansregular",
            "ubunturegular",
            "notosanscjkregular",
            "notosanscjkkrregular",
            "nanumgothic",
            "droidsansfallbackfull",
            "dejavusansmono",
            "liberationmonoregular",
        ]
    }
}

/// What a generic family means here, in order; empty for the system's own face, the first fallback.
fn generic_families(family: &str) -> Option<&'static [&'static str]> {
    match family.to_ascii_lowercase().as_str() {
        "" | "sans" | "sans-serif" | "system-ui" | "ui-sans-serif" => Some(&[]),
        "serif" | "ui-serif" => Some(&[
            "Times New Roman",
            "Times",
            "Noto Serif",
            "DejaVu Serif",
            "Liberation Serif",
        ]),
        "mono" | "monospace" | "ui-monospace" => Some(&[
            "Menlo",
            "Consolas",
            "Droid Sans Mono",
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Noto Sans Mono",
        ]),
        _ => None,
    }
}

fn file_stem(name: &str) -> &str {
    name.trim_end_matches(".ttf")
        .trim_end_matches(".TTF")
        .trim_end_matches(".otf")
        .trim_end_matches(".OTF")
        .trim_end_matches(".ttc")
        .trim_end_matches(".TTC")
}

fn is_font_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc")
}

type FontFile = (String, PathBuf);

/// One face of a font file on disk.
struct SystemFace {
    /// Its place in the file list.
    file: usize,
    index: u32,
    info: FaceInfo,
}

/// Reads what each file's faces call themselves. A mapped file only pages in the tables read.
fn read_system_faces(files: &[FontFile]) -> Vec<SystemFace> {
    let started = Instant::now();
    let mut faces = Vec::new();
    for (file, (_, path)) in files.iter().enumerate() {
        let Some(bytes) = file_bytes(path) else {
            continue;
        };
        let contents: &[u8] = (*bytes).as_ref();
        for index in 0.. {
            let Some(info) = FaceInfo::read(contents, index) else {
                break;
            };
            faces.push(SystemFace { file, index, info });
        }
    }
    log::debug!(
        "font: read {} faces of {} files in {:?}",
        faces.len(),
        files.len(),
        started.elapsed()
    );
    faces
}

/// The face of `family`, by the names faces give themselves, closest to `font`.
fn closest_system_face<'a>(
    family: &str,
    font: FontRef,
    faces: &'a [SystemFace],
) -> Option<&'a SystemFace> {
    let want = normalise(family);
    faces
        .iter()
        .filter(|face| face.info.is_named(&want))
        .min_by(|a, b| a.info.distance(font).total_cmp(&b.info.distance(font)))
}

fn scan_font_files(dirs: &[PathBuf]) -> Vec<FontFile> {
    let mut files = Vec::new();
    let mut pending = dirs.to_vec();
    while let Some(dir) = pending.pop() {
        let Ok(entries) = read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if is_font_file(&name) {
                files.push((normalise(file_stem(&name)), entry.path()));
            }
        }
    }
    files.sort_by(|a, b| a.1.cmp(&b.1));
    // A folder listed inside another is walked twice.
    files.dedup_by(|a, b| a.1 == b.1);
    files
}

fn face_suffixes(weight: FontWeight, style: FontStyle) -> &'static [&'static str] {
    match (weight.number(), style) {
        (1..=149, FontStyle::Normal) => &["thin", "hairline", "light", "", "regular"],
        (150..=249, FontStyle::Normal) => &["extralight", "ultralight", "light", "", "regular"],
        (250..=349, FontStyle::Normal) => &["light", "semilight", "", "regular"],
        (450..=549, FontStyle::Normal) => &["medium", "", "regular", "bold"],
        (550..=649, FontStyle::Normal) => &["semibold", "demibold", "bold", "", "regular"],
        (650..=749, FontStyle::Normal) => &["bold", "", "regular"],
        (750..=849, FontStyle::Normal) => &["extrabold", "ultrabold", "bold", "", "regular"],
        (850..=1000, FontStyle::Normal) => &["black", "heavy", "extrabold", "bold", "", "regular"],
        (_, FontStyle::Normal) => &["", "regular", "book", "roman"],
        (1..=149, FontStyle::Italic) => &[
            "thinitalic",
            "hairlineitalic",
            "lightitalic",
            "italic",
            "",
            "regular",
        ],
        (150..=249, FontStyle::Italic) => &[
            "extralightitalic",
            "ultralightitalic",
            "lightitalic",
            "italic",
            "",
            "regular",
        ],
        (250..=349, FontStyle::Italic) => &["lightitalic", "lightoblique", "italic", "", "regular"],
        (450..=549, FontStyle::Italic) => {
            &["mediumitalic", "mediumoblique", "italic", "", "regular"]
        }
        (550..=649, FontStyle::Italic) => &[
            "semibolditalic",
            "demibolditalic",
            "bolditalic",
            "bold",
            "",
            "regular",
        ],
        (650..=749, FontStyle::Italic) => &["bolditalic", "boldoblique", "bold", "", "regular"],
        (750..=849, FontStyle::Italic) => &[
            "extrabolditalic",
            "ultrabolditalic",
            "bolditalic",
            "bold",
            "",
            "regular",
        ],
        (850..=1000, FontStyle::Italic) => &[
            "blackitalic",
            "heavyitalic",
            "extrabolditalic",
            "bolditalic",
            "bold",
            "",
            "regular",
        ],
        (_, FontStyle::Italic) => &["italic", "oblique", "", "regular"],
    }
}

fn find_font_file<'a>(
    family: &str,
    weight: FontWeight,
    style: FontStyle,
    files: &'a [FontFile],
) -> Option<&'a Path> {
    let want = normalise(family);
    if want.is_empty() {
        return None;
    }

    for suffix in face_suffixes(weight, style) {
        let name = format!("{want}{suffix}");
        if let Some((_, path)) = files.iter().find(|(stem, _)| *stem == name) {
            return Some(path);
        }
    }

    files
        .iter()
        .filter(|(stem, _)| stem.starts_with(&want))
        .min_by_key(|(stem, _)| stem.len())
        .map(|(_, path)| path.as_path())
}

/// The installed fonts. The folders are walked, and the faces read, on first use.
#[derive(Default)]
pub(crate) struct Installed {
    dirs: Vec<PathBuf>,
    files: OnceLock<Vec<FontFile>>,
    system_faces: OnceLock<Vec<SystemFace>>,
    fallbacks: OnceLock<Vec<PathBuf>>,
}

impl Installed {
    pub(crate) fn new() -> Self {
        Self {
            dirs: font_search_dirs(),
            ..Self::default()
        }
    }

    fn files(&self) -> &[FontFile] {
        self.files.get_or_init(|| scan_font_files(&self.dirs))
    }

    fn system_faces(&self) -> &[SystemFace] {
        self.system_faces
            .get_or_init(|| read_system_faces(self.files()))
    }

    /// The installed face of `font`, by the names faces give themselves. A file named like the
    /// family covers one whose names can't be read, or a family asked for by part of its name.
    pub(crate) fn find(&self, font: FontRef) -> Option<Located> {
        let named = [font.family];
        let generic = generic_families(font.family);
        let found = generic
            .unwrap_or(&named)
            .iter()
            .find_map(|family| self.find_family(family, font));
        if found.is_none() && generic.is_none() {
            log::warn!("font: no family {:?}, using the default", font.family);
        }
        found
    }

    fn find_family(&self, family: &str, font: FontRef) -> Option<Located> {
        if let Some(face) = closest_system_face(family, font, self.system_faces()) {
            let (_, path) = self.files().get(face.file)?;
            return Some(Located {
                path: path.clone(),
                index: face.index,
                variations: face.info.variations(font),
            });
        }
        let path = find_font_file(family, font.weight, font.style, self.files())?;
        Some(Located {
            path: path.to_path_buf(),
            index: 0,
            variations: font.variations.to_vec(),
        })
    }

    /// Family names of the installed faces, sorted, without duplicates.
    pub(crate) fn families(&self) -> Vec<String> {
        let mut families: Vec<String> = self
            .system_faces()
            .iter()
            .map(|face| face.info.family.clone())
            // macOS keeps its own interface faces out of menus this way.
            .filter(|family| !family.starts_with('.'))
            .collect();
        families.sort_unstable();
        families.dedup();
        families
    }

    /// The files of the fallback faces, in order, for the ones the system has.
    pub(crate) fn fallbacks(&self) -> &[PathBuf] {
        self.fallbacks.get_or_init(|| {
            fallback_stems()
                .iter()
                .filter_map(|stem| {
                    let (_, path) = self.files().iter().find(|(s, _)| s == stem)?;
                    Some(path.clone())
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Font;
    use std::env::temp_dir;
    use std::fs::{create_dir_all, read, remove_dir_all, write};
    use std::process::id;
    use std::slice::from_ref;

    const TUFFY: &[u8] = include_bytes!("../../../fonts/Tuffy.ttf");

    fn files(stems: &[&str]) -> Vec<FontFile> {
        stems
            .iter()
            .map(|s| ((*s).to_owned(), PathBuf::from(format!("{s}.ttf"))))
            .collect()
    }

    fn pick(family: &str, weight: FontWeight, style: FontStyle, list: &[FontFile]) -> String {
        find_font_file(family, weight, style, list)
            .and_then(Path::to_str)
            .unwrap_or("")
            .to_owned()
    }

    #[test]
    fn picks_numeric_face_weights_without_collapsing_them_to_bold() {
        let list = files(&[
            "notosanslight",
            "notosansregular",
            "notosansmedium",
            "notosanssemibold",
            "notosansbold",
            "notosansblackitalic",
        ]);
        for (weight, expected) in [
            (300, "notosanslight.ttf"),
            (400, "notosansregular.ttf"),
            (500, "notosansmedium.ttf"),
            (600, "notosanssemibold.ttf"),
            (700, "notosansbold.ttf"),
        ] {
            assert_eq!(
                pick(
                    "Noto Sans",
                    FontWeight::from_number(weight),
                    FontStyle::Normal,
                    &list
                ),
                expected
            );
        }
        assert_eq!(
            pick(
                "Noto Sans",
                FontWeight::from_number(900),
                FontStyle::Italic,
                &list
            ),
            "notosansblackitalic.ttf"
        );
        assert_eq!(
            pick(
                "Noto Sans",
                FontWeight::from_number(200),
                FontStyle::Normal,
                &list
            ),
            "notosanslight.ttf"
        );
    }

    #[test]
    fn picks_the_family_not_a_longer_one() {
        let list = files(&["dejavusansmono", "dejavusansbold", "dejavusans"]);
        let normal = (FontWeight::Normal, FontStyle::Normal);
        assert_eq!(
            pick("DejaVu Sans", normal.0, normal.1, &list),
            "dejavusans.ttf"
        );
        assert_eq!(pick("DejaVu", normal.0, normal.1, &list), "dejavusans.ttf");
    }

    #[test]
    fn a_family_never_takes_a_file_with_a_shorter_or_unrelated_name() {
        let list = files(&["arial", "ptsans", "notosanscjkregular", "comicsansms"]);
        let normal = (FontWeight::Normal, FontStyle::Normal);
        for family in [
            "Arial Hebrew",
            "PT Sans Narrow",
            "Noto Sans CJK KR",
            "Sans MS",
        ] {
            assert_eq!(pick(family, normal.0, normal.1, &list), "", "{family}");
        }
    }

    /// A folder with one file whose name says nothing of its font.
    fn tuffy_named(name: &str) -> (Installed, PathBuf) {
        let root = temp_dir().join(format!("aurea-named-{}-{name}", id()));
        create_dir_all(&root).expect("create font dir");
        write(root.join(name), TUFFY).expect("write font");
        let installed = Installed {
            dirs: vec![root.clone()],
            ..Installed::default()
        };
        (installed, root)
    }

    #[test]
    fn a_face_is_found_by_its_own_name_whatever_its_file_is_called() {
        let (r, root) = tuffy_named("zz0.ttf");
        let font = Font::new("Tuffy", 14.0);
        let found = r.find((&font).into());
        let missing = r.find((&Font::new("Tuffy Hebrew", 14.0)).into());
        let families = r.families();
        let bytes = found.as_ref().map(|f| read(&f.path).expect("read"));
        drop(r);
        remove_dir_all(&root).expect("remove");
        assert_eq!(bytes.as_deref(), Some(TUFFY));
        assert!(missing.is_none());
        assert_eq!(families, ["Tuffy"]);
    }

    #[test]
    fn a_file_that_holds_no_font_is_skipped() {
        let root = temp_dir().join(format!("aurea-broken-{}", id()));
        create_dir_all(&root).expect("create font dir");
        write(root.join("broken.ttf"), b"nope").expect("write");
        let files = scan_font_files(from_ref(&root));
        let faces = read_system_faces(&files);
        remove_dir_all(&root).expect("remove");
        assert_eq!(files.len(), 1);
        assert!(faces.is_empty());
    }

    #[test]
    fn a_folder_inside_another_lists_its_files_once() {
        let root = temp_dir().join(format!("aurea-twice-{}", id()));
        let nested = root.join("nested");
        create_dir_all(&nested).expect("create font dirs");
        write(nested.join("a.ttf"), b"").expect("write");
        let found = scan_font_files(&[root.clone(), nested]);
        remove_dir_all(&root).expect("remove");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn picks_the_bold_face_and_falls_back_to_regular() {
        let list = files(&["notosansregular", "notosansbold", "dejavusans"]);
        assert_eq!(
            pick("Noto Sans", FontWeight::Bold, FontStyle::Normal, &list),
            "notosansbold.ttf"
        );
        assert_eq!(
            pick("Noto Sans", FontWeight::Normal, FontStyle::Normal, &list),
            "notosansregular.ttf"
        );
        assert_eq!(
            pick("DejaVu Sans", FontWeight::Bold, FontStyle::Italic, &list),
            "dejavusans.ttf"
        );
    }

    #[test]
    fn generic_families_never_match_a_name_by_accident() {
        let list = files(&[
            "comicsansms",
            "notosansadlamregular",
            "notoserifregular",
            "notoserifbold",
            "dejavusansmono",
        ]);
        let at = |family, weight| {
            let found = |family| find_font_file(family, weight, FontStyle::Normal, &list);
            generic_families(family)
                .map_or_else(
                    || found(family),
                    |families| families.iter().find_map(|family| found(family)),
                )
                .and_then(Path::to_str)
                .unwrap_or("")
                .to_owned()
        };
        assert_eq!(at("Sans", FontWeight::Normal), "");
        assert_eq!(at("", FontWeight::Normal), "");
        assert_eq!(at("serif", FontWeight::Normal), "notoserifregular.ttf");
        assert_eq!(at("serif", FontWeight::Bold), "notoserifbold.ttf");
        assert_eq!(at("monospace", FontWeight::Normal), "dejavusansmono.ttf");
        assert_eq!(at("Comic Sans MS", FontWeight::Normal), "comicsansms.ttf");
    }

    #[test]
    fn scans_nested_font_directories() {
        let root = temp_dir().join(format!("aurea-fonts-{}", id()));
        let nested = root.join("truetype").join("dejavu");
        create_dir_all(&nested).expect("create font dirs");
        write(nested.join("DejaVuSans.ttf"), b"").expect("write font");
        write(nested.join("README"), b"").expect("write readme");

        let found = scan_font_files(from_ref(&root));
        let _ = remove_dir_all(&root);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "dejavusans");
    }
}
