//! SVG path data (`d="M 2 2 L 22 22 ..."`) into a [`Path`].
//!
//! Every command is read, absolute and relative: `M L H V C S Q T A Z`.
//! Arcs become cubic curves, at most a quarter turn each.

use std::error::Error;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::from_utf8;

use crate::numeric::f32_to_usize_clamped;
use crate::types::{Path, PathCommand, Point};

/// Why some SVG text didn't parse, and the byte it stopped at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgError {
    pub at: usize,
    pub reason: &'static str,
}

impl Display for SvgError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "svg: {} at byte {}", self.reason, self.at)
    }
}

impl Error for SvgError {}

/// Reads SVG path data into a path in the same coordinates.
pub fn parse_path_data(d: &str) -> Result<Path, SvgError> {
    let mut path = Path::new();
    append_path_data(d, &mut path)?;
    Ok(path)
}

/// Like [`parse_path_data`], adding to `path`.
pub fn append_path_data(d: &str, path: &mut Path) -> Result<(), SvgError> {
    let mut r = Reader {
        s: d.as_bytes(),
        i: 0,
    };
    let mut pen = Pen::default();
    let mut command = None;
    loop {
        r.skip_separators();
        let Some(c) = r.peek() else {
            return Ok(());
        };
        if c.is_ascii_alphabetic() {
            r.i += 1;
            command = Some(c);
            if c == b'Z' || c == b'z' {
                pen.close(path);
                continue;
            }
        } else if command.is_none() {
            return Err(r.error("path data must start with a command"));
        }
        let Some(c) = command else {
            return Err(r.error("missing command"));
        };
        pen.step(c, &mut r, path)?;
        // Coordinates after a moveto are linetos of the same case.
        command = match c {
            b'M' => Some(b'L'),
            b'm' => Some(b'l'),
            other => Some(other),
        };
    }
}

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn error(&self, reason: &'static str) -> SvgError {
        SvgError { at: self.i, reason }
    }

    fn skip_separators(&mut self) {
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_whitespace() || c == b',')
        {
            self.i += 1;
        }
    }

    /// One number, the way SVG writes them: `-1.5e3`, `.5`, and `1.5.5`
    /// for two numbers.
    fn number(&mut self) -> Result<f32, SvgError> {
        self.skip_separators();
        let start = self.i;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.i += 1;
        }
        let mut dot = false;
        while let Some(c) = self.peek() {
            match c {
                b'0'..=b'9' => self.i += 1,
                b'.' if !dot => {
                    dot = true;
                    self.i += 1;
                }
                b'e' | b'E' if self.i > start => {
                    self.i += 1;
                    if matches!(self.peek(), Some(b'+' | b'-')) {
                        self.i += 1;
                    }
                }
                _ => break,
            }
        }
        from_utf8(&self.s[start..self.i])
            .ok()
            .and_then(|t| t.parse().ok())
            .ok_or(SvgError {
                at: start,
                reason: "expected a number",
            })
    }

    /// An arc flag: one `0` or `1`, which may run into the next number.
    fn flag(&mut self) -> Result<bool, SvgError> {
        self.skip_separators();
        match self.peek() {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(self.error("expected an arc flag")),
        }
    }

    fn point(&mut self) -> Result<Point, SvgError> {
        Ok(Point::new(self.number()?, self.number()?))
    }
}

/// Where drawing has got to.
#[derive(Default)]
struct Pen {
    at: Point,
    start: Point,
    /// The last curve's second control point, for `S`/`T` reflections.
    last_cubic: Option<Point>,
    last_quad: Option<Point>,
    open: bool,
}

impl Pen {
    fn close(&mut self, path: &mut Path) {
        if self.open {
            path.commands.push(PathCommand::Close);
        }
        self.at = self.start;
        self.open = false;
        self.last_cubic = None;
        self.last_quad = None;
    }

    fn step(&mut self, c: u8, r: &mut Reader<'_>, path: &mut Path) -> Result<(), SvgError> {
        let relative = c.is_ascii_lowercase();
        let origin = if relative {
            self.at
        } else {
            Point::new(0.0, 0.0)
        };
        let abs = |p: Point| Point::new(p.x + origin.x, p.y + origin.y);
        let (mut cubic, mut quad) = (None, None);
        match c.to_ascii_uppercase() {
            b'M' => {
                let p = abs(r.point()?);
                path.commands.push(PathCommand::MoveTo(p));
                self.at = p;
                self.start = p;
                self.open = true;
            }
            b'L' => self.line(abs(r.point()?), path),
            b'H' => {
                let x = r.number()? + origin.x;
                self.line(Point::new(x, self.at.y), path);
            }
            b'V' => {
                let y = r.number()? + origin.y;
                self.line(Point::new(self.at.x, y), path);
            }
            b'C' => {
                let (c1, c2, p) = (abs(r.point()?), abs(r.point()?), abs(r.point()?));
                self.cubic(c1, c2, p, path);
                cubic = Some(c2);
            }
            b'S' => {
                let c1 = reflect(self.last_cubic, self.at);
                let (c2, p) = (abs(r.point()?), abs(r.point()?));
                self.cubic(c1, c2, p, path);
                cubic = Some(c2);
            }
            b'Q' => {
                let (q, p) = (abs(r.point()?), abs(r.point()?));
                self.quad(q, p, path);
                quad = Some(q);
            }
            b'T' => {
                let q = reflect(self.last_quad, self.at);
                let p = abs(r.point()?);
                self.quad(q, p, path);
                quad = Some(q);
            }
            b'A' => {
                let (rx, ry, rotation) = (r.number()?, r.number()?, r.number()?);
                let (large, sweep) = (r.flag()?, r.flag()?);
                let p = abs(r.point()?);
                let arc = Arc {
                    radii: (rx, ry),
                    rotation: rotation.to_radians(),
                    large,
                    sweep,
                };
                for (c1, c2, end) in arc.cubics(self.at, p) {
                    self.cubic(c1, c2, end, path);
                }
                self.at = p;
            }
            _ => return Err(r.error("unknown path command")),
        }
        self.last_cubic = cubic;
        self.last_quad = quad;
        Ok(())
    }

    fn begin(&mut self, path: &mut Path) {
        if !self.open {
            path.commands.push(PathCommand::MoveTo(self.at));
            self.start = self.at;
            self.open = true;
        }
    }

    fn line(&mut self, p: Point, path: &mut Path) {
        self.begin(path);
        path.commands.push(PathCommand::LineTo(p));
        self.at = p;
    }

    fn cubic(&mut self, c1: Point, c2: Point, p: Point, path: &mut Path) {
        self.begin(path);
        path.commands.push(PathCommand::CubicTo(c1, c2, p));
        self.at = p;
    }

    fn quad(&mut self, q: Point, p: Point, path: &mut Path) {
        self.begin(path);
        path.commands.push(PathCommand::QuadTo(q, p));
        self.at = p;
    }
}

/// The last control point mirrored through `at`, or `at` with none.
fn reflect(control: Option<Point>, at: Point) -> Point {
    control.map_or(at, |c| Point::new(2.0 * at.x - c.x, 2.0 * at.y - c.y))
}

/// An SVG elliptical arc, as its endpoint parameters give it.
pub(crate) struct Arc {
    pub radii: (f32, f32),
    /// Radians.
    pub rotation: f32,
    pub large: bool,
    pub sweep: bool,
}

impl Arc {
    /// Cubic segments from `from` to `to`, as (control, control, end).
    /// Zero radii make a straight line, as SVG says.
    pub fn cubics(&self, from: Point, to: Point) -> Vec<(Point, Point, Point)> {
        let (mut rx, mut ry) = (self.radii.0.abs(), self.radii.1.abs());
        if rx == 0.0 || ry == 0.0 || (from.x == to.x && from.y == to.y) {
            return vec![(from, to, to)];
        }
        let (sin, cos) = self.rotation.sin_cos();
        // The endpoints in the ellipse's own frame, centred between them.
        let (dx, dy) = ((from.x - to.x) / 2.0, (from.y - to.y) / 2.0);
        let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
        // Radii too small to reach scale up until they just do.
        let grow = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
        if grow > 1.0 {
            rx *= grow.sqrt();
            ry *= grow.sqrt();
        }
        let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
        let mut k = (num / den).max(0.0).sqrt();
        if self.large == self.sweep {
            k = -k;
        }
        let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
        let cx = cos * cx1 - sin * cy1 + (from.x + to.x) / 2.0;
        let cy = sin * cx1 + cos * cy1 + (from.y + to.y) / 2.0;
        let angle = |ux: f32, uy: f32| uy.atan2(ux);
        let theta = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
        let mut delta = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry) - theta;
        if self.sweep && delta < 0.0 {
            delta += TAU;
        } else if !self.sweep && delta > 0.0 {
            delta -= TAU;
        }
        // The slack keeps float error on an exact quarter-turn multiple from
        // costing a whole extra piece (a half circle is two, not three).
        let pieces = (delta.abs() / FRAC_PI_2 - 1e-3).ceil().max(1.0);
        let step = delta / pieces;
        // Control arm length for a cubic matching `step` of a circle.
        let arm = 4.0 / 3.0 * (step / 4.0).tan();
        let on = |t: f32| {
            let (s, c) = t.sin_cos();
            (rx * c, ry * s)
        };
        let place = |(x, y): (f32, f32)| Point::new(cos * x - sin * y + cx, sin * x + cos * y + cy);
        let count = f32_to_usize_clamped(pieces);
        let mut out = Vec::with_capacity(count);
        let mut t = theta;
        for i in 0..count {
            let t2 = t + step;
            let (a, b) = (on(t), on(t2));
            let (sa, ca) = t.sin_cos();
            let (sb, cb) = t2.sin_cos();
            let c1 = (a.0 - arm * rx * sa, a.1 + arm * ry * ca);
            let c2 = (b.0 + arm * rx * sb, b.1 - arm * ry * cb);
            let end = if i + 1 == count { to } else { place(b) };
            out.push((place(c1), place(c2), end));
            t = t2;
        }
        debug_assert!(delta.abs() <= TAU + PI);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    #[test]
    fn absolute_and_relative_lines() {
        let p = parse_path_data("M2 2 L 10,2 l0 8 H2 v-8 z").unwrap();
        assert_eq!(p.commands.len(), 6);
        assert!(
            matches!(p.commands[2], PathCommand::LineTo(q) if close(q, Point::new(10.0, 10.0)))
        );
        assert!(matches!(p.commands[4], PathCommand::LineTo(q) if close(q, Point::new(2.0, 2.0))));
        assert!(matches!(p.commands[5], PathCommand::Close));
    }

    #[test]
    fn numbers_run_together() {
        // "1-2.5.5" is 1, -2.5, 0.5; pairs after M are lines.
        let p = parse_path_data("M1-2.5.5 3e1,4 5").unwrap();
        assert!(matches!(p.commands[0], PathCommand::MoveTo(q) if close(q, Point::new(1.0, -2.5))));
        assert!(matches!(p.commands[1], PathCommand::LineTo(q) if close(q, Point::new(0.5, 30.0))));
        assert!(matches!(p.commands[2], PathCommand::LineTo(q) if close(q, Point::new(4.0, 5.0))));
    }

    #[test]
    fn smooth_curves_reflect_the_last_control() {
        let p = parse_path_data("M0 0 C 0 10 10 10 10 0 S 20 -10 20 0").unwrap();
        let PathCommand::CubicTo(c1, _, _) = p.commands[2] else {
            panic!("expected a cubic");
        };
        assert!(close(c1, Point::new(10.0, -10.0)));
    }

    #[test]
    fn a_half_circle_arc_passes_through_its_far_point() {
        // From (0,0) to (20,0) on radius 10, sweeping clockwise in y-down: via (10,-10).
        let p = parse_path_data("M0 0 A10 10 0 0 1 20 0").unwrap();
        let ends: Vec<Point> = p
            .commands
            .iter()
            .filter_map(|c| match c {
                PathCommand::CubicTo(_, _, e) => Some(*e),
                _ => None,
            })
            .collect();
        assert_eq!(ends.len(), 2);
        assert!(close(ends[0], Point::new(10.0, -10.0)), "{:?}", ends[0]);
        assert!(close(ends[1], Point::new(20.0, 0.0)));
    }

    #[test]
    fn arc_flags_may_touch() {
        assert!(parse_path_data("M0 0a5 5 0 1120 0").is_ok());
    }

    #[test]
    fn errors_say_where() {
        let e = parse_path_data("M0 0 L x").unwrap_err();
        assert_eq!(e.at, 7);
        assert!(parse_path_data("10 10").is_err());
    }
}
