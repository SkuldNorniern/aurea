//! Fonts an app brings itself, for canvas text on every platform. Native widgets keep the system's.
//!
//! ```no_run
//! # use aurea_render::text::fonts;
//! let families = fonts::register(include_bytes!("../../fonts/Tuffy.ttf"))?;
//! fonts::set_default_family(&families[0]);
//! # Ok::<(), aurea_foundation::AureaError>(())
//! ```
//!
//! After that `Font::new("Tuffy", 14.0)` draws it, and any text a font has no glyph for looks here
//! before the system's fallbacks. A variable font with a `wght` axis gives every weight.

use std::borrow::Cow;
use std::path::Path;
#[cfg(test)]
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use aurea_foundation::{AureaError, AureaResult, lock};
use skrifa::attribute::Style;
use skrifa::raw::types::NameId;
use skrifa::{FontRef as SkrifaFont, MetadataProvider, Tag};

use crate::text::file_backend::file_bytes;
use crate::text::glyph::{FaceData, FontBytes};
use crate::text::platform::{FontRef, get_platform_rasterizer};
use crate::types::{FontStyle, FontVariation};

const WGHT: [u8; 4] = *b"wght";

/// What a face's own tables say about it: what it is called and how it looks.
pub(crate) struct FaceInfo {
    pub(crate) family: String,
    /// Normalised family, full and PostScript names, in every language the font has them.
    names: Vec<String>,
    weight: f32,
    width: f32,
    italic: bool,
    weights: Option<(f32, f32)>,
}

impl FaceInfo {
    /// The face at `index` of a font file, or `None` past its last face.
    pub(crate) fn read(contents: &[u8], index: u32) -> Option<Self> {
        let font = SkrifaFont::from_index(contents, index).ok()?;
        let family = [NameId::TYPOGRAPHIC_FAMILY_NAME, NameId::FAMILY_NAME]
            .into_iter()
            .find_map(|name| font.localized_strings(name).english_or_first())?
            .chars()
            .collect();
        let mut names: Vec<String> = [
            NameId::TYPOGRAPHIC_FAMILY_NAME,
            NameId::FAMILY_NAME,
            NameId::FULL_NAME,
            NameId::POSTSCRIPT_NAME,
        ]
        .into_iter()
        .flat_map(|name| font.localized_strings(name))
        .map(|name| normalise(&name.chars().collect::<String>()))
        .collect();
        names.sort_unstable();
        names.dedup();
        let attributes = font.attributes();
        let weights = font
            .axes()
            .get_by_tag(Tag::new(&WGHT))
            .map(|axis| (axis.min_value(), axis.max_value()));
        Some(Self {
            family,
            names,
            weight: attributes.weight.value(),
            width: attributes.stretch.ratio(),
            italic: !matches!(attributes.style, Style::Normal),
            weights,
        })
    }

    /// Whether the face goes by `name`, normalised.
    pub(crate) fn is_named(&self, name: &str) -> bool {
        self.names.iter().any(|own| own == name)
    }

    /// How far the face is from what `font` asks for; lower is closer. Style comes first, then
    /// normal width, then weight.
    pub(crate) fn distance(&self, font: FontRef) -> f32 {
        let want = f32::from(font.weight.number());
        let weight = match self.weights {
            Some((min, max)) => want.clamp(min, max),
            None => self.weight,
        };
        let style = if self.italic == (font.style == FontStyle::Italic) {
            0.0
        } else {
            1000.0
        };
        style + (self.width - 1.0).abs() * 500.0 + (weight - want).abs()
    }

    /// The axis positions that draw `font` in this face: its own, and its weight when the face
    /// has that axis and it did not pick a place on it.
    pub(crate) fn variations(&self, font: FontRef) -> Vec<FontVariation> {
        let mut out = font.variations.to_vec();
        if let Some((min, max)) = self.weights
            && !out.iter().any(|v| v.tag == WGHT)
        {
            out.push(FontVariation {
                tag: WGHT,
                value: f32::from(font.weight.number()).clamp(min, max),
            });
        }
        out
    }
}

/// One face of a registered font.
pub(crate) struct Face {
    pub(crate) id: u32,
    pub(crate) bytes: FontBytes,
    pub(crate) index: u32,
    pub(crate) info: FaceInfo,
    data: FaceData,
}

impl Face {
    fn read(id: u32, bytes: &FontBytes, index: u32) -> Option<Self> {
        let contents: &[u8] = (**bytes).as_ref();
        Some(Self {
            id,
            bytes: bytes.clone(),
            index,
            info: FaceInfo::read(contents, index)?,
            data: FaceData::new(bytes.clone(), index, &[])?,
        })
    }
}

#[derive(Default)]
struct Registry {
    faces: Vec<Arc<Face>>,
    default: Option<String>,
}

impl Registry {
    fn add(&mut self, bytes: &FontBytes) -> AureaResult<Vec<String>> {
        let mut families = Vec::new();
        let mut added = Vec::new();
        for index in 0.. {
            let id = u32::try_from(self.faces.len() + added.len())
                .map_err(|_| AureaError::InvalidFont)?;
            let Some(face) = Face::read(id, bytes, index) else {
                break;
            };
            if !families.contains(&face.info.family) {
                families.push(face.info.family.clone());
            }
            added.push(Arc::new(face));
        }
        if added.is_empty() {
            return Err(AureaError::InvalidFont);
        }
        self.faces.append(&mut added);
        Ok(families)
    }

    fn find(&self, font: FontRef) -> Option<Arc<Face>> {
        let family = match (is_default(font.family), &self.default) {
            (true, Some(default)) => default.as_str(),
            (true, None) => return None,
            (false, _) => font.family,
        };
        let want = normalise(family);
        closest(self.faces.iter().filter(|f| f.info.is_named(&want)), font)
    }

    fn covering(&self, font: FontRef, ch: char) -> Option<Arc<Face>> {
        closest(self.faces.iter().filter(|f| f.data.has_char(ch)), font)
    }
}

fn closest<'a>(faces: impl Iterator<Item = &'a Arc<Face>>, font: FontRef) -> Option<Arc<Face>> {
    faces
        .min_by(|a, b| a.info.distance(font).total_cmp(&b.info.distance(font)))
        .cloned()
}

fn is_default(family: &str) -> bool {
    family.is_empty()
        || ["sans", "sans-serif", "system-ui", "ui-sans-serif"]
            .iter()
            .any(|name| family.eq_ignore_ascii_case(name))
}

pub(crate) fn normalise(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// System fonts only promise letters and common punctuation. A symbol like `☷` or `≡` falls back
/// to whatever the device has, and may draw as a box or nothing on a phone, so draw menu, grip and
/// close marks with [`crate::vector::Icon`], or register a font holding what the app needs.
///
/// Adds a font from its bytes, `.ttf`, `.otf` or `.ttc`: `include_bytes!` is not copied. Gives the
/// family names it holds, for [`Font::new`](crate::types::Font::new). From any thread; registered
/// fonts stay until the app exits.
///
/// # Errors
///
/// [`AureaError::InvalidFont`] when the bytes hold no font.
pub fn register(data: impl Into<Cow<'static, [u8]>>) -> AureaResult<Vec<String>> {
    add(&(Arc::new(data.into()) as FontBytes))
}

/// Adds a font from a file; see [`register`]. A file in a folder the system keeps read-only is
/// mapped, not read.
///
/// # Errors
///
/// [`AureaError::InvalidFont`] when the file cannot be read or holds no font.
pub fn register_file(path: impl AsRef<Path>) -> AureaResult<Vec<String>> {
    add(&file_bytes(path.as_ref()).ok_or(AureaError::InvalidFont)?)
}

fn add(bytes: &FontBytes) -> AureaResult<Vec<String>> {
    let families = lock(&REGISTRY).add(bytes)?;
    GENERATION.fetch_add(1, Ordering::Relaxed);
    Ok(families)
}

/// Draws text that names no family, or a generic sans one, in the registered `family`.
pub fn set_default_family(family: &str) {
    lock(&REGISTRY).default = Some(family.to_owned());
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// The family names registered so far.
pub fn families() -> Vec<String> {
    let mut families: Vec<String> = lock(&REGISTRY)
        .faces
        .iter()
        .map(|f| f.info.family.clone())
        .collect();
    families.sort_unstable();
    families.dedup();
    families
}

/// The family names installed on the system, sorted, for a font menu. Reads the font folders each
/// call where the system has no list of its own, so keep the answer.
pub fn system_families() -> Vec<String> {
    get_platform_rasterizer().system_families()
}

/// Changes when text may draw differently, for a cache of measured text to key on.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

pub(crate) fn get(id: u32) -> Option<Arc<Face>> {
    lock(&REGISTRY).faces.get(id as usize).cloned()
}

pub(crate) fn find(font: FontRef) -> Option<Arc<Face>> {
    lock(&REGISTRY).find(font)
}

pub(crate) fn covering(font: FontRef, ch: char) -> Option<Arc<Face>> {
    lock(&REGISTRY).covering(font, ch)
}

/// Taken shared by tests that count cache hits, and exclusively by one that registers.
#[cfg(test)]
pub(crate) static TEST_GUARD: RwLock<()> = RwLock::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Font, FontWeight};

    const TUFFY: &[u8] = include_bytes!("../../fonts/Tuffy.ttf");

    fn tuffy() -> FontBytes {
        Arc::new(TUFFY)
    }

    #[test]
    fn rejects_what_is_not_a_font() {
        let mut registry = Registry::default();
        assert!(matches!(
            registry.add(&(Arc::new(b"nope".as_slice()) as FontBytes)),
            Err(AureaError::InvalidFont)
        ));
        assert!(registry.faces.is_empty());
    }

    #[test]
    fn finds_by_family_ignoring_case_and_spaces() {
        let mut registry = Registry::default();
        let families = registry.add(&tuffy()).expect("a font");
        assert_eq!(families, ["Tuffy"]);
        let find = |name: &str| registry.find((&Font::new(name, 14.0)).into()).is_some();
        assert!(find("tuffy") && find(" T U F F Y "));
        assert!(registry.faces[0].info.names.len() >= 2, "full names too");
        assert!(!find("Tuf") && !find(""));
    }

    #[test]
    fn the_default_family_takes_generic_names_only() {
        let mut registry = Registry::default();
        registry.add(&tuffy()).expect("a font");
        registry.default = Some("Tuffy".to_owned());
        let find = |name: &str| registry.find((&Font::new(name, 14.0)).into()).is_some();
        assert!(find("") && find("Sans") && find("system-ui"));
        assert!(!find("serif") && !find("Consolas"));
    }

    #[test]
    fn covering_finds_only_a_face_with_the_glyph() {
        let mut registry = Registry::default();
        registry.add(&tuffy()).expect("a font");
        let font = Font::new("", 14.0);
        assert!(registry.covering((&font).into(), 'A').is_some());
        assert!(registry.covering((&font).into(), '가').is_none());
    }

    #[test]
    fn closest_face_wins_on_style_then_width_then_weight() {
        let mut registry = Registry::default();
        registry.add(&tuffy()).expect("a font");
        let mut bold = Face::read(1, &registry.faces[0].bytes, 0).expect("face");
        bold.info.weight = 700.0;
        let mut italic = Face::read(2, &registry.faces[0].bytes, 0).expect("face");
        italic.info.italic = true;
        let mut condensed = Face::read(3, &registry.faces[0].bytes, 0).expect("face");
        condensed.info.weight = 700.0;
        condensed.info.width = 0.75;
        registry.faces.push(Arc::new(condensed));
        registry.faces.push(Arc::new(bold));
        registry.faces.push(Arc::new(italic));
        let pick = |font: Font| registry.find((&font).into()).expect("a face").id;
        let tuffy = Font::new("Tuffy", 14.0);
        assert_eq!(pick(tuffy.clone()), 0);
        assert_eq!(pick(tuffy.clone().with_weight(FontWeight::Bold)), 1);
        assert_eq!(pick(tuffy.with_style(FontStyle::Italic)), 2);
    }

    #[test]
    fn a_weight_axis_takes_the_weight_unless_one_was_picked() {
        let registry = {
            let mut r = Registry::default();
            r.add(&tuffy()).expect("a font");
            r
        };
        let mut face = Face::read(0, &registry.faces[0].bytes, 0).expect("face");
        face.info.weights = Some((100.0, 800.0));
        let bold = Font::new("Tuffy", 14.0).with_weight(FontWeight::Number(900));
        assert_eq!(face.info.variations((&bold).into())[0].value, 800.0);
        assert_eq!(face.info.distance((&bold).into()), 100.0);
        let picked = bold.with_variation("wght", 300.0);
        let variations = face.info.variations((&picked).into());
        assert_eq!(variations.len(), 1);
        assert_eq!(variations[0].value, 300.0);
    }

    // The registry is global, so everything that goes through it is here.
    #[test]
    fn a_registered_font_draws_on_the_platform_backend() {
        let _only_one = TEST_GUARD.write();
        let name = "Tuffy";
        let raster = get_platform_rasterizer();
        let font = Font::new(name, 24.0);
        let before = generation();
        let fallback = raster.face((&font).into()).expect("a face");
        assert_eq!(register(TUFFY).expect("a font"), [name]);
        assert_ne!(generation(), before);
        assert!(families().contains(&name.to_owned()));

        let face = raster.face((&font).into()).expect("the registered face");
        assert_ne!(face, fallback, "registering takes over a resolved name");
        let data = raster.face_data(face).expect("data");
        assert_eq!(data.bytes(), TUFFY);
        let glyph = data.glyph_index('H');
        let drawn = raster.rasterize_subpixel(face, glyph, 24.0).expect("glyph");
        assert!(drawn.coverage.iter().any(|&c| c > 200));
        let (ascent, descent) = raster.face_metrics(face, 24.0);
        assert!(ascent > 0.0 && descent > 0.0);
        let bold = font.clone().with_weight(FontWeight::Bold);
        assert_eq!(
            raster.face_for_char((&bold).into(), face, 'H'),
            face,
            "the face has it"
        );
    }

    #[test]
    #[ignore = "changes the font of every test measuring beside it; run with --ignored"]
    fn the_default_family_takes_over_the_unnamed_font() {
        let _only_one = TEST_GUARD.write();
        let name = "Tuffy";
        assert_eq!(register(TUFFY).expect("a font"), [name]);
        set_default_family(name);
        let raster = get_platform_rasterizer();
        let plain = Font::new("", 24.0);
        let default = raster.face((&plain).into()).expect("the default");
        let bytes = raster.face_data(default).expect("data").bytes().to_vec();
        lock(&REGISTRY).default = None;
        GENERATION.fetch_add(1, Ordering::Relaxed);
        assert_eq!(bytes, TUFFY);
    }

    #[test]
    fn the_system_lists_its_families() {
        let families = system_families();
        assert!(families.windows(2).all(|pair| pair[0] < pair[1]));
        #[cfg(windows)]
        assert!(families.iter().any(|family| family == "Segoe UI"));
    }

    #[test]
    fn a_missing_file_is_an_invalid_font() {
        assert!(matches!(
            register_file("__no_such_font__.ttf"),
            Err(AureaError::InvalidFont)
        ));
    }
}
