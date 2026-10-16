//! Path tessellation for the CPU rasterizer.
//!
//! Turns path commands into edges (y range, x at y_min, slope) so the scanline
//! filler can find crossings and fill between them.

use crate::numeric::f32_to_u32_clamped;
use crate::types::{Path, PathCommand, Point};

/// One edge for scanline filling: y range, x at the top, and dx/dy slope.
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub y_min: f32,
    pub y_max: f32,
    pub x_at_y_min: f32,
    pub slope: f32,
    /// 1 where the outline runs down, -1 where it runs up.
    pub dir: i32,
}

impl Edge {
    pub fn new(p1: Point, p2: Point) -> Option<Self> {
        let dir = if p1.y <= p2.y { 1 } else { -1 };
        let (p1, p2) = if p1.y <= p2.y { (p1, p2) } else { (p2, p1) };

        if (p2.y - p1.y).abs() < 0.001 {
            return None;
        }

        let slope = (p2.x - p1.x) / (p2.y - p1.y);

        Some(Self {
            y_min: p1.y,
            y_max: p2.y,
            x_at_y_min: p1.x,
            slope,
            dir,
        })
    }

    pub fn x_at_y(&self, y: f32) -> f32 {
        if y < self.y_min {
            self.x_at_y_min
        } else if y > self.y_max {
            self.x_at_y_min + self.slope * (self.y_max - self.y_min)
        } else {
            self.x_at_y_min + self.slope * (y - self.y_min)
        }
    }
}

/// Converts a path into a list of edges for the scanline filler (lines, quads and cubics
/// subdivided), reusing `out`'s existing allocation instead of allocating a fresh `Vec`.
///
/// Points are taken as they are. The recorder puts paths in physical pixels
/// like every other command, so there is nothing left to scale here.
pub fn tessellate_path_into(path: &Path, edges: &mut Vec<Edge>) {
    edges.clear();
    let mut current_point = Point::new(0.0, 0.0);
    let mut start_point = Point::new(0.0, 0.0);
    let mut has_start = false;

    for command in &path.commands {
        match command {
            PathCommand::MoveTo(p) => {
                let p = *p;
                current_point = p;
                start_point = p;
                has_start = true;
            }
            PathCommand::LineTo(p) => {
                let p = *p;
                if has_start {
                    if let Some(edge) = Edge::new(current_point, p) {
                        edges.push(edge);
                    }
                    current_point = p;
                }
            }
            PathCommand::QuadTo(p1, p2) => {
                let p1 = *p1;
                let p2 = *p2;
                tessellate_quad(current_point, p1, p2, edges);
                current_point = p2;
            }
            PathCommand::CubicTo(p1, p2, p3) => {
                let p1 = *p1;
                let p2 = *p2;
                let p3 = *p3;
                tessellate_cubic(current_point, p1, p2, p3, edges);
                current_point = p3;
            }
            PathCommand::Close => {
                if has_start {
                    if let Some(edge) = Edge::new(current_point, start_point) {
                        edges.push(edge);
                    }
                    current_point = start_point;
                }
            }
        }
    }
}

/// How far, in pixels, a flattened curve may stray from the true one.
const FLATNESS: f32 = 0.25;

/// Chords that keep a curve of degree `degree` within [`FLATNESS`] of
/// itself, given the largest second difference `dd` of its control points
/// (Wang's formula). A fixed count left large curves visibly faceted and
/// spent chords on small ones.
fn curve_steps(degree: f32, dd: f32) -> u16 {
    let n = (degree * (degree - 1.0) / 8.0 * dd / FLATNESS)
        .sqrt()
        .ceil();
    // NaN, from a non-finite control point, takes one chord.
    u16::try_from(f32_to_u32_clamped(n).clamp(1, 256)).unwrap_or(256)
}

fn second_difference(a: Point, b: Point, c: Point) -> f32 {
    (a.x - 2.0 * b.x + c.x).hypot(a.y - 2.0 * b.y + c.y)
}

/// Chords for the quadratic `p0, p1, p2`.
pub fn quad_steps(p0: Point, p1: Point, p2: Point) -> u16 {
    curve_steps(2.0, second_difference(p0, p1, p2))
}

/// Chords for the cubic `p0, p1, p2, p3`.
pub fn cubic_steps(p0: Point, p1: Point, p2: Point, p3: Point) -> u16 {
    let dd = second_difference(p0, p1, p2).max(second_difference(p1, p2, p3));
    curve_steps(3.0, dd)
}

fn tessellate_quad(p0: Point, p1: Point, p2: Point, edges: &mut Vec<Edge>) {
    let steps = quad_steps(p0, p1, p2);
    let mut prev = p0;
    for i in 1..=steps {
        let t = f32::from(i) / f32::from(steps);
        let p = quadratic_bezier(p0, p1, p2, t);
        if let Some(edge) = Edge::new(prev, p) {
            edges.push(edge);
        }
        prev = p;
    }
}

fn tessellate_cubic(p0: Point, p1: Point, p2: Point, p3: Point, edges: &mut Vec<Edge>) {
    let steps = cubic_steps(p0, p1, p2, p3);
    let mut prev = p0;
    for i in 1..=steps {
        let t = f32::from(i) / f32::from(steps);
        let p = cubic_bezier(p0, p1, p2, p3, t);
        if let Some(edge) = Edge::new(prev, p) {
            edges.push(edge);
        }
        prev = p;
    }
}

/// Evaluate quadratic Bezier curve at parameter t
fn quadratic_bezier(p0: Point, p1: Point, p2: Point, t: f32) -> Point {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let t2 = t * t;

    Point::new(
        mt2 * p0.x + 2.0 * mt * t * p1.x + t2 * p2.x,
        mt2 * p0.y + 2.0 * mt * t * p1.y + t2 * p2.y,
    )
}

/// Evaluate cubic Bezier curve at parameter t
fn cubic_bezier(p0: Point, p1: Point, p2: Point, p3: Point, t: f32) -> Point {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;
    let t2 = t * t;
    let t3 = t2 * t;

    Point::new(
        mt3 * p0.x + 3.0 * mt2 * t * p1.x + 3.0 * mt * t2 * p2.x + t3 * p3.x,
        mt3 * p0.y + 3.0 * mt2 * t * p1.y + 3.0 * mt * t2 * p2.y + t3 * p3.y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each chord's midpoint stays within the flatness of the curve's own
    /// midpoint, on a wide curve and a tight one alike.
    #[test]
    fn curves_flatten_within_the_tolerance() {
        let wide = [
            Point::new(0.0, 0.0),
            Point::new(400.0, 260.0),
            Point::new(800.0, -200.0),
            Point::new(1280.0, 0.0),
        ];
        let tight = [
            Point::new(10.0, 10.0),
            Point::new(14.0, 2.0),
            Point::new(18.0, 18.0),
            Point::new(12.0, 11.0),
        ];
        for [p0, p1, p2, p3] in [wide, tight] {
            let n = cubic_steps(p0, p1, p2, p3);
            let at = |t: f32| cubic_bezier(p0, p1, p2, p3, t);
            for i in 0..n {
                let (a, b) = (f32::from(i), f32::from(n));
                let (s, e, m) = (at(a / b), at((a + 1.0) / b), at((a + 0.5) / b));
                let chord = Point::new((s.x + e.x) / 2.0, (s.y + e.y) / 2.0);
                assert!(
                    (chord.x - m.x).hypot(chord.y - m.y) <= FLATNESS + 1e-3,
                    "{n} chords"
                );
            }
        }
        assert!(cubic_steps(wide[0], wide[1], wide[2], wide[3]) > 8);
        assert!(
            quad_steps(
                Point::new(0.0, 0.0),
                Point::new(1.0, 1.0),
                Point::new(2.0, 0.0)
            ) <= 2
        );
        assert_eq!(
            quad_steps(
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(2.0, 0.0)
            ),
            1
        );
    }
}
