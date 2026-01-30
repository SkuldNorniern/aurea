//! Hit testing for shapes and paths.
//!
//! Answers whether a point lies inside a path (odd-even ray cast), rect, or circle.

use super::super::types::{Path, PathCommand, Point, Rect};
use super::path::Edge;

/// Returns true if the point is inside the path (odd-even rule: ray to the right, count crossings).
pub fn hit_test_path(path: &Path, point: Point) -> bool {
    let bounds = path_bounds(path);
    if !hit_test_rect(bounds, point) {
        return false;
    }

    let mut intersections = 0;
    let mut current_point = Point::new(0.0, 0.0);
    let mut start_point = Point::new(0.0, 0.0);
    let mut has_start = false;

    for command in &path.commands {
        match command {
            PathCommand::MoveTo(p) => {
                current_point = *p;
                start_point = *p;
                has_start = true;
            }
            PathCommand::LineTo(p) => {
                if has_start {
                    if ray_intersects_line_segment(point, current_point, *p) {
                        intersections += 1;
                    }
                    current_point = *p;
                }
            }
            PathCommand::QuadTo(p1, p2) => {
                let steps = 8;
                let mut prev = current_point;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let p = quadratic_bezier(current_point, *p1, *p2, t);
                    if ray_intersects_line_segment(point, prev, p) {
                        intersections += 1;
                    }
                    prev = p;
                }
                current_point = *p2;
            }
            PathCommand::CubicTo(p1, p2, p3) => {
                let steps = 16;
                let mut prev = current_point;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let p = cubic_bezier(current_point, *p1, *p2, *p3, t);
                    if ray_intersects_line_segment(point, prev, p) {
                        intersections += 1;
                    }
                    prev = p;
                }
                current_point = *p3;
            }
            PathCommand::Close => {
                if has_start {
                    if ray_intersects_line_segment(point, current_point, start_point) {
                        intersections += 1;
                    }
                    current_point = start_point;
                }
            }
        }
    }

    intersections % 2 == 1
}

/// Returns true if the point is inside the rectangle (inclusive edges).
pub fn hit_test_rect(rect: Rect, point: Point) -> bool {
    point.x >= rect.x
        && point.x <= rect.x + rect.width
        && point.y >= rect.y
        && point.y <= rect.y + rect.height
}

/// Returns true if the point is inside the circle (distance <= radius).
pub fn hit_test_circle(center: Point, radius: f32, point: Point) -> bool {
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    let dist_squared = dx * dx + dy * dy;
    dist_squared <= radius * radius
}

fn ray_intersects_line_segment(ray_origin: Point, seg_start: Point, seg_end: Point) -> bool {
    let y = ray_origin.y;
    let y1 = seg_start.y;
    let y2 = seg_end.y;

    if (y1 > y && y2 > y) || (y1 < y && y2 < y) || (y1 == y2) {
        return false;
    }

    let t = if (y2 - y1).abs() < 0.001 {
        0.0
    } else {
        (y - y1) / (y2 - y1)
    };

    let x_intersect = seg_start.x + t * (seg_end.x - seg_start.x);
    x_intersect > ray_origin.x
}

fn path_bounds(path: &Path) -> Rect {
    if path.commands.is_empty() {
        return Rect::new(0.0, 0.0, 0.0, 0.0);
    }

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    let mut current_point = Point::new(0.0, 0.0);
    let mut has_point = false;

    for command in &path.commands {
        match command {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
                current_point = *p;
                has_point = true;
            }
            PathCommand::QuadTo(p1, p2) => {
                let steps = 8;
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    let p = quadratic_bezier(current_point, *p1, *p2, t);
                    min_x = min_x.min(p.x);
                    min_y = min_y.min(p.y);
                    max_x = max_x.max(p.x);
                    max_y = max_y.max(p.y);
                }
                current_point = *p2;
            }
            PathCommand::CubicTo(p1, p2, p3) => {
                let steps = 16;
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    let p = cubic_bezier(current_point, *p1, *p2, *p3, t);
                    min_x = min_x.min(p.x);
                    min_y = min_y.min(p.y);
                    max_x = max_x.max(p.x);
                    max_y = max_y.max(p.y);
                }
                current_point = *p3;
            }
            PathCommand::Close => {}
        }
    }

    if !has_point {
        return Rect::new(0.0, 0.0, 0.0, 0.0);
    }

    Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
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
