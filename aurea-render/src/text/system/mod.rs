//! Installed fonts: the font folders and the faces in them, the generic and fallback families, the
//! platform's own rasterizer, and the bytes of font files. Without the `system-fonts` feature there
//! are none, and only registered fonts and the built-in one draw.

#[cfg(all(windows, feature = "system-fonts"))]
mod directwrite;
#[cfg(all(windows, feature = "system-fonts"))]
mod dwrite;
#[cfg(feature = "system-fonts")]
mod lookup;
#[cfg(unix)]
mod mapped;
#[cfg(not(feature = "system-fonts"))]
mod off;

#[cfg(feature = "system-fonts")]
pub(crate) use lookup::Installed;
#[cfg(not(feature = "system-fonts"))]
pub(crate) use off::Installed;

#[cfg(all(windows, feature = "system-fonts"))]
use directwrite::DirectWriteRasterizer;
#[cfg(unix)]
use mapped::Mapped;

use crate::text::glyph::FontBytes;
use crate::text::platform::PlatformTextRasterizer;
use crate::types::FontVariation;
use std::fs::read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

/// A font found by name: its file, which face of the file, and the variations to set.
pub(crate) struct Located {
    pub(crate) path: PathBuf,
    pub(crate) index: u32,
    pub(crate) variations: Vec<FontVariation>,
}

/// File bytes held weakly by the faces that use them.
pub(crate) type WeakBytes = Weak<dyn AsRef<[u8]> + Send + Sync>;

/// The platform's own rasterizer, where it has one that draws the system's fonts.
#[cfg(all(windows, feature = "system-fonts"))]
pub(crate) fn native_rasterizer() -> Option<Box<dyn PlatformTextRasterizer>> {
    DirectWriteRasterizer::new()
        .ok()
        .map(|dw| Box::new(dw) as Box<dyn PlatformTextRasterizer>)
}

/// The platform's own rasterizer, where it has one that draws the system's fonts.
#[cfg(not(all(windows, feature = "system-fonts")))]
pub(crate) fn native_rasterizer() -> Option<Box<dyn PlatformTextRasterizer>> {
    None
}

/// Folders only the system writes to, and it replaces files rather than rewriting them, so a mapped
/// file there can't change. A CJK face is over 20 MB and most of it is never touched.
#[cfg(unix)]
fn fixed_folders() -> &'static [&'static str] {
    #[cfg(target_os = "android")]
    {
        &["/system/", "/product/", "/system_ext/"]
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        &["/System/Library/"]
    }
    #[cfg(not(any(target_os = "android", target_os = "macos", target_os = "ios")))]
    {
        &["/usr/share/fonts/"]
    }
}

/// The bytes of a font file: mapped where it is a fixed system file, else read.
pub(crate) fn file_bytes(path: &Path) -> Option<FontBytes> {
    #[cfg(unix)]
    if fixed_folders().iter().any(|root| path.starts_with(root))
        && let Some(mapped) = Mapped::open(path)
    {
        return Some(Arc::new(mapped));
    }
    Some(Arc::new(read(path).ok()?))
}
