//! A path's coverage as bytes, drawn white off to the side.

use crate::cpu::clip::ClipBox;
use crate::cpu::raster::band::Band;
use crate::cpu::raster::fill::PathScratch;
use crate::cpu::raster::item;
use crate::types::{BlendMode, Color, Paint, Path, PathCommand, Point};
use aurea_foundation::AureaResult;
use std::sync::Arc;
use std::{iter, mem};

/// Draws `path` white into `buf` and keeps only each pixel's alpha, in all four bytes.
fn draw_all(
    path: &Path,
    paint: &Paint,
    origin: Point,
    size: (u32, u32),
    buf: &mut [u32],
) -> AureaResult<()> {
    draw_alpha(path, paint, origin, size, buf, &mut PathScratch::default())?;
    for px in buf {
        let a = (*px >> 24) as u8;
        *px = u32::from_ne_bytes([a; 4]);
    }
    Ok(())
}

/// Draws `path` white into `buf`, leaving each pixel's alpha in its top byte.
pub(crate) fn draw_alpha(
    path: &Path,
    paint: &Paint,
    origin: Point,
    (width, height): (u32, u32),
    buf: &mut [u32],
    scratch: &mut PathScratch,
) -> AureaResult<()> {
    let shift = |p: Point| Point::new(p.x - origin.x, p.y - origin.y);
    // The shifted commands reuse the buffer from the last call, so only the first one allocates.
    let mut moved = Path {
        commands: mem::take(&mut scratch.shifted),
        fill_rule: path.fill_rule,
    };
    moved.commands.clear();
    moved.commands.reserve(path.commands.len());
    for command in &path.commands {
        moved.commands.push(match *command {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(shift(p)),
            PathCommand::LineTo(p) => PathCommand::LineTo(shift(p)),
            PathCommand::QuadTo(c, p) => PathCommand::QuadTo(shift(c), shift(p)),
            PathCommand::CubicTo(a, b, p) => PathCommand::CubicTo(shift(a), shift(b), shift(p)),
            PathCommand::Close => PathCommand::Close,
        });
    }
    let white = Paint {
        color: Color::rgb(255, 255, 255),
        ..paint.clone()
    };
    let drawn = item::draw_path(
        &moved,
        &white,
        BlendMode::Normal,
        &mut Band::whole(&mut *buf),
        scratch,
        width,
        ClipBox::surface(width, height),
    );
    scratch.shifted = moved.commands;
    drawn
}

/// The path's coverage over `width` by `height` pixels from `origin`, as four bytes per
/// pixel, each the alpha of the pixel's white fill.
pub(crate) fn of_path(
    path: &Path,
    paint: &Paint,
    origin: Point,
    (width, height): (u32, u32),
) -> AureaResult<Arc<[u8]>> {
    let len = width as usize * height as usize;
    // The bytes are allocated once and drawn into directly, as words when aligned.
    let mut out: Arc<[u8]> = Arc::from_iter(iter::repeat_n(0u8, len * 4));
    let bytes = Arc::get_mut(&mut out).expect("a new Arc is unique");
    // SAFETY: every bit pattern is a valid `u32`, and only the aligned middle is used.
    let (head, words, tail) = unsafe { bytes.align_to_mut::<u32>() };
    if head.is_empty() && tail.is_empty() {
        draw_all(path, paint, origin, (width, height), words)?;
        return Ok(out);
    }
    let mut buf = vec![0u32; len];
    draw_all(path, paint, origin, (width, height), &mut buf)?;
    Ok(buf.iter().flat_map(|px| px.to_ne_bytes()).collect())
}

#[cfg(test)]
mod tests {

    use crate::types::{Paint, Path, PathCommand, Point};

    /// The copy-based result `path_coverage` replaced: draw into words, then copy the bytes.
    fn reference(path: &Path, origin: Point, (w, h): (u32, u32)) -> Vec<u8> {
        let mut buf = vec![0u32; w as usize * h as usize];
        super::draw_all(path, &Paint::new(), origin, (w, h), &mut buf).unwrap();
        buf.iter().flat_map(|px| px.to_ne_bytes()).collect()
    }

    #[test]
    fn path_coverage_matches_copy_based_reference() {
        let mut path = Path::new();
        for (i, (x, y)) in [(1.5, 1.0), (6.0, 1.0), (3.0, 5.25)]
            .into_iter()
            .enumerate()
        {
            let p = Point::new(x, y);
            path.commands.push(if i == 0 {
                PathCommand::MoveTo(p)
            } else {
                PathCommand::LineTo(p)
            });
        }
        path.commands.push(PathCommand::Close);
        let (origin, size) = (Point::new(0.0, 0.0), (8, 6));
        let got = super::of_path(&path, &Paint::new(), origin, size).unwrap();
        assert_eq!(&got[..], &reference(&path, origin, size)[..]);
        let (pixels, _) = got.as_chunks::<4>();
        assert!(pixels.iter().any(|px| px[0] == 255));
        assert!(pixels.iter().all(|px| px.iter().all(|&b| b == px[0])));
    }
}
