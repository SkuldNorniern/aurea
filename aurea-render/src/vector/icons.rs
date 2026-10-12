//! A small set of line icons for editor chrome, drawn on a 24-unit grid with 2-unit strokes in
//! whatever colour they're given.

use std::sync::LazyLock;

use aurea_foundation::AureaResult;

use super::svg::VectorImage;
use crate::drawing::DrawingContext;
use crate::types::{Color, Rect};

/// A built-in icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    /// The selection pointer.
    Arrow,
    Hand,
    Zoom,
    ZoomIn,
    ZoomOut,
    Brush,
    Eraser,
    Crop,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    Plus,
    Minus,
    Close,
    Check,
    Folder,
    File,
    Trash,
    Undo,
    Redo,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronDown,
    Move,
    Text,
    Layers,
}

impl Icon {
    pub const ALL: [Icon; 28] = [
        Icon::Arrow,
        Icon::Hand,
        Icon::Zoom,
        Icon::ZoomIn,
        Icon::ZoomOut,
        Icon::Brush,
        Icon::Eraser,
        Icon::Crop,
        Icon::Eye,
        Icon::EyeOff,
        Icon::Lock,
        Icon::Unlock,
        Icon::Plus,
        Icon::Minus,
        Icon::Close,
        Icon::Check,
        Icon::Folder,
        Icon::File,
        Icon::Trash,
        Icon::Undo,
        Icon::Redo,
        Icon::ChevronLeft,
        Icon::ChevronRight,
        Icon::ChevronUp,
        Icon::ChevronDown,
        Icon::Move,
        Icon::Text,
        Icon::Layers,
    ];

    /// The icon's SVG path data, on the 24-unit grid, meant to be stroked.
    pub fn path_data(self) -> &'static str {
        const LENS: &str = "M10.5 3a7.5 7.5 0 1 1 0 15a7.5 7.5 0 1 1 0-15Z M16 16 L21 21";
        const EYE: &str =
            "M2 12 C5 6 19 6 22 12 C19 18 5 18 2 12Z M12 9a3 3 0 1 1 0 6a3 3 0 1 1 0-6Z";
        match self {
            Icon::Arrow => "M5 3 L5 19 L9.5 14.5 L12.5 21 L15 20 L12 13.5 L18 13.5Z",
            Icon::Hand => {
                "M8 13 V6 a1.5 1.5 0 0 1 3 0 V11 M11 10 V4.5 a1.5 1.5 0 0 1 3 0 V11 \
                 M14 10.5 V6 a1.5 1.5 0 0 1 3 0 V12 M17 10 a1.5 1.5 0 0 1 3 0 V14 \
                 a7 7 0 0 1 -7 7 H12 a6 6 0 0 1 -4.8 -2.4 L4.3 14.7 a1.5 1.5 0 0 1 2.4 -1.8 L8 14.5"
            }
            Icon::Zoom => LENS,
            Icon::ZoomIn => {
                "M10.5 3a7.5 7.5 0 1 1 0 15a7.5 7.5 0 1 1 0-15Z M16 16 L21 21 M7.5 10.5 H13.5 M10.5 7.5 V13.5"
            }
            Icon::ZoomOut => {
                "M10.5 3a7.5 7.5 0 1 1 0 15a7.5 7.5 0 1 1 0-15Z M16 16 L21 21 M7.5 10.5 H13.5"
            }
            Icon::Brush => {
                "M18 3 L21 6 L12 15 L9 12Z M9 12 C6 12 4.5 14 4.5 16.5 C4.5 18.5 3.5 20 2.5 21 C7 21 12 19.5 12 15"
            }
            Icon::Eraser => {
                "M15 3.5 L20.5 9 L11 18.5 H6.5 L3.5 15.5Z M8.5 10 L14 15.5 M11 18.5 H21"
            }
            Icon::Crop => "M6 2 V18 H22 M2 6 H18 V22",
            Icon::Eye => EYE,
            Icon::EyeOff => {
                "M2 12 C5 6 19 6 22 12 C19 18 5 18 2 12Z M12 9a3 3 0 1 1 0 6a3 3 0 1 1 0-6Z M3 3 L21 21"
            }
            Icon::Lock => "M5 11 H19 V21 H5Z M8 11 V7.5 a4 4 0 0 1 8 0 V11",
            Icon::Unlock => "M5 11 H19 V21 H5Z M8 11 V7.5 a4 4 0 0 1 7.7 -1.5",
            Icon::Plus => "M12 5 V19 M5 12 H19",
            Icon::Minus => "M5 12 H19",
            Icon::Close => "M6 6 L18 18 M18 6 L6 18",
            Icon::Check => "M4 12.5 L9.5 18 L20 6.5",
            Icon::Folder => {
                "M3 6 a1 1 0 0 1 1 -1 H9 L11 7.5 H20 a1 1 0 0 1 1 1 V18 a1 1 0 0 1 -1 1 H4 a1 1 0 0 1 -1 -1Z"
            }
            Icon::File => "M6 2.5 H14 L19 7.5 V21.5 H6Z M14 2.5 V7.5 H19",
            Icon::Trash => {
                "M4 6.5 H20 M9 6.5 V4 H15 V6.5 M6 6.5 L7 21 H17 L18 6.5 M10 10.5 V17 M14 10.5 V17"
            }
            Icon::Undo => "M9 14 L4 9 L9 4 M4 9 H14.5 a5.5 5.5 0 0 1 0 11 H11",
            Icon::Redo => "M15 14 L20 9 L15 4 M20 9 H9.5 a5.5 5.5 0 0 0 0 11 H13",
            Icon::ChevronLeft => "M15 5 L8 12 L15 19",
            Icon::ChevronRight => "M9 5 L16 12 L9 19",
            Icon::ChevronUp => "M5 15 L12 8 L19 15",
            Icon::ChevronDown => "M5 9 L12 16 L19 9",
            Icon::Move => {
                "M12 2 V22 M2 12 H22 M9 5 L12 2 L15 5 M9 19 L12 22 L15 19 M5 9 L2 12 L5 15 M19 9 L22 12 L19 15"
            }
            Icon::Text => "M5 6 V4 H19 V6 M12 4 V20 M9 20 H15",
            Icon::Layers => {
                "M12 3 L22 8 L12 13 L2 8Z M2 12.5 L12 17.5 L22 12.5 M2 17 L12 22 L22 17"
            }
        }
    }

    /// The icon as a vector image, parsed once.
    pub fn image(self) -> &'static VectorImage {
        static IMAGES: LazyLock<Vec<VectorImage>> = LazyLock::new(|| {
            let grid = Rect::new(0.0, 0.0, 24.0, 24.0);
            Icon::ALL
                .iter()
                .map(|icon| {
                    VectorImage::stroked_path_data(icon.path_data(), grid, 2.0)
                        .unwrap_or_else(|_| VectorImage::empty(grid))
                })
                .collect()
        });
        let i = Icon::ALL.iter().position(|&i| i == self).unwrap_or(0);
        &IMAGES[i]
    }

    /// Draws the icon into `dest`, fitted and centred, in `color`.
    pub fn draw(self, ctx: &mut dyn DrawingContext, dest: Rect, color: Color) -> AureaResult<()> {
        self.image().draw(ctx, dest, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::parse_path_data;

    #[test]
    fn every_icon_parses() {
        for icon in Icon::ALL {
            let path = parse_path_data(icon.path_data());
            assert!(path.is_ok(), "{icon:?}: {:?}", path.err());
            assert!(!icon.image().is_empty(), "{icon:?}");
        }
    }

    #[test]
    fn all_lists_every_icon_once() {
        for (i, a) in Icon::ALL.iter().enumerate() {
            assert!(!Icon::ALL[i + 1..].contains(a), "{a:?} twice");
        }
    }
}
