//! Path tessellation for the CPU rasterizer.
//!
//! Turns path commands into edges (y range, x at y_min, slope) so the scanline
//! filler can find crossings and fill between them.

use crate::types::{Path, PathCommand, Point};

/// One edge for scanline filling: y range, x at the top, and dx/dy slope.
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub y_min: f32,
    pub y_max: f32,
    pub x_at_y_min: f32,
    pub slope: f32,
}

impl Edge {
    pub fn new(p1: Point, p2: Point) -> Option<Self> {
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
/// `scale` converts the path's (logical) coordinates to physical pixels as each point
/// is visited, so callers no longer need to pre-build a separately-scaled `Path`.
pub fn tessellate_path_into(path: &Path, scale: f32, edges: &mut Vec<Edge>) {
    edges.clear();
    let mut current_point = Point::new(0.0, 0.0);
    let mut start_point = Point::new(0.0, 0.0);
    let mut has_start = false;

    let sp = |p: &Point| Point::new(p.x * scale, p.y * scale);

    for command in &path.commands {
        match command {
            PathCommand::MoveTo(p) => {
                let p = sp(p);
                current_point = p;
                start_point = p;
                has_start = true;
            }
            PathCommand::LineTo(p) => {
                let p = sp(p);
                if has_start {
                    if let Some(edge) = Edge::new(current_point, p) {
                        edges.push(edge);
                    }
                    current_point = p;
                }
            }
            PathCommand::QuadTo(p1, p2) => {
                let p1 = sp(p1);
                let p2 = sp(p2);
                tessellate_quad(current_point, p1, p2, edges);
                current_point = p2;
            }
            PathCommand::CubicTo(p1, p2, p3) => {
                let p1 = sp(p1);
                let p2 = sp(p2);
                let p3 = sp(p3);
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

fn tessellate_quad(p0: Point, p1: Point, p2: Point, edges: &mut Vec<Edge>) {
    let steps: u16 = 4;
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
    let steps: u16 = 8;
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
