//! Glyph outlines as [`Path`]s, for apps that draw text as vector shapes.

use skrifa::instance::Size;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::cpu::clip::ClipBox;
use crate::cpu::raster::fill::{PathScratch, fill_path};
use crate::geometry::{Path, PathCommand, Point};
use crate::numeric::{f32_to_i32_clamped, f32_to_u32_clamped};
use crate::text::glyph::{FaceData, GlyphCoverage};
use crate::types::{BlendMode, Color};

struct PathPen(Path);

impl OutlinePen for PathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.commands.push(PathCommand::MoveTo(Point::new(x, -y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.commands.push(PathCommand::LineTo(Point::new(x, -y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.commands.push(PathCommand::QuadTo(
            Point::new(cx0, -cy0),
            Point::new(x, -y),
        ));
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

/// `glyph`'s outline at `size` pixels, with the pen at the origin on the baseline; `None` for a
/// glyph with no outline (a space, or a bitmap-only emoji).
pub fn glyph_outline(face: &FaceData, glyph: u16, size: f32) -> Option<Path> {
    let font = FontRef::from_index(face.bytes(), face.index()).ok()?;
    let outline = font.outline_glyphs().get(GlyphId::new(u32::from(glyph)))?;
    let mut pen = PathPen(Path::new());
    outline
        .draw(
            DrawSettings::unhinted(Size::new(size), face.coords()),
            &mut pen,
        )
        .ok()?;
    (!pen.0.commands.is_empty()).then_some(pen.0)
}

fn each_point(path: &mut Path, mut f: impl FnMut(&mut Point)) {
    for c in &mut path.commands {
        match c {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => f(p),
            PathCommand::QuadTo(a, b) => {
                f(a);
                f(b);
            }
            PathCommand::CubicTo(a, b, d) => {
                f(a);
                f(b);
                f(d);
            }
            PathCommand::Close => {}
        }
    }
}

pub(crate) fn outline_coverage(
    face: &FaceData,
    glyph: u16,
    size: f32,
    x_scale: f32,
) -> GlyphCoverage {
    let Some(mut path) = glyph_outline(face, glyph, size) else {
        return GlyphCoverage::default();
    };
    let (mut lo, mut hi) = (
        Point::new(f32::MAX, f32::MAX),
        Point::new(f32::MIN, f32::MIN),
    );
    each_point(&mut path, |p| {
        p.x *= x_scale;
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    });
    let (left, top) = (lo.x.floor(), lo.y.floor());
    each_point(&mut path, |p| *p = Point::new(p.x - left, p.y - top));
    let width = f32_to_u32_clamped(hi.x.ceil() - left).max(1);
    let height = f32_to_u32_clamped(hi.y.ceil() - top).max(1);
    let mut buf = vec![0u32; (width * height) as usize];
    fill_path(
        &path,
        Color::rgb(255, 255, 255),
        BlendMode::Normal,
        &mut buf,
        width,
        ClipBox::surface(width, height),
        &mut PathScratch::default(),
    );
    GlyphCoverage {
        width,
        height,
        left: f32_to_i32_clamped(left),
        top: f32_to_i32_clamped(top),
        coverage: buf.iter().map(|p| p.to_be_bytes()[0]).collect(),
    }
}
