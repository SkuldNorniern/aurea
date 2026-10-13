//! Glyph outlines as [`Path`]s, for apps that draw text as vector shapes.

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::geometry::{Path, PathCommand, Point};
use crate::text::glyph::FaceData;

/// Collects an outline into a [`Path`], flipping y: fonts measure up, the
/// canvas down.
struct PathPen(Path);

impl OutlinePen for PathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.commands.push(PathCommand::MoveTo(Point::new(x, -y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.commands.push(PathCommand::LineTo(Point::new(x, -y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0
            .commands
            .push(PathCommand::QuadTo(Point::new(cx0, -cy0), Point::new(x, -y)));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.commands.push(PathCommand::CubicTo(
            Point::new(cx0, -cy0),
            Point::new(cx1, -cy1),
            Point::new(x, -y),
        ));
    }

    fn close(&mut self) {
        self.0.commands.push(PathCommand::Close);
    }
}

/// `glyph`'s outline at `size` pixels, with the pen at the origin on the
/// baseline; `None` for a glyph with no outline (a space, or a bitmap-only
/// emoji). Unhinted, so it scales and rotates cleanly.
pub fn glyph_outline(face: &FaceData, glyph: u16, size: f32) -> Option<Path> {
    let font = FontRef::from_index(face.bytes(), face.index()).ok()?;
    let outline = font.outline_glyphs().get(GlyphId::new(u32::from(glyph)))?;
    let mut pen = PathPen(Path::new());
    outline
        .draw(
            DrawSettings::unhinted(Size::new(size), LocationRef::default()),
            &mut pen,
        )
        .ok()?;
    (!pen.0.commands.is_empty()).then_some(pen.0)
}
