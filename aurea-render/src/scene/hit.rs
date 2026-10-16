//! Hit testing for shapes and paths.

use crate::cpu::path::{cubic_steps, quad_steps};
use crate::types::{Path, PathCommand, Point, Rect};

/// Returns true if the point is inside the path (odd-even rule: ray to the right, count crossings).
pub fn hit_test_path(path: &Path, point: Point) -> bool {
    let bounds = path_bounds(path);
    if !hit_test_rect(bounds, point) {
        return false;
    }

    let mut intersections: u32 = 0;
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
                    intersections += count_line_intersection(point, current_point, *p);
                    current_point = *p;
                }
            }
            PathCommand::QuadTo(p1, p2) => {
                intersections += count_quad_intersections(point, current_point, *p1, *p2);
                current_point = *p2;
            }
            PathCommand::CubicTo(p1, p2, p3) => {
                intersections += count_cubic_intersections(point, current_point, *p1, *p2, *p3);
                current_point = *p3;
            }
            PathCommand::Close => {
                if has_start {
                    intersections += count_line_intersection(point, current_point, start_point);
                    current_point = start_point;
                }
            }
        }
    }

    intersections % 2 == 1
}

fn count_line_intersection(point: Point, a: Point, b: Point) -> u32 {
    u32::from(ray_intersects_line_segment(point, a, b))
}

fn count_quad_intersections(point: Point, p0: Point, p1: Point, p2: Point) -> u32 {
    let steps = quad_steps(p0, p1, p2);
    let mut prev = p0;
    let mut count = 0;
    for i in 1..=steps {
        let t = f32::from(i) / f32::from(steps);
        let p = quadratic_bezier(p0, p1, p2, t);
        count += u32::from(ray_intersects_line_segment(point, prev, p));
        prev = p;
    }
    count
}

fn count_cubic_intersections(point: Point, p0: Point, p1: Point, p2: Point, p3: Point) -> u32 {
    let steps = cubic_steps(p0, p1, p2, p3);
    let mut prev = p0;
    let mut count = 0;
    for i in 1..=steps {
        let t = f32::from(i) / f32::from(steps);
        let p = cubic_bezier(p0, p1, p2, p3, t);
        count += u32::from(ray_intersects_line_segment(point, prev, p));
        prev = p;
    }
    count
}

/// Returns true if the point is inside the rectangle (inclusive edges).
pub fn hit_test_rect(rect: Rect, point: Point) -> bool {
    point.x >= rect.x
        && point.x <= rect.x + rect.width
        && point.y >= rect.y
        && point.y <= rect.y + rect.height
}

/// Returns true if the point is inside `rect` with corners of `radius`.
pub fn hit_test_round_rect(rect: Rect, radius: f32, point: Point) -> bool {
    if !hit_test_rect(rect, point) {
        return false;
    }
    let r = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
    let cx = point.x.clamp(rect.x + r, rect.x + rect.width - r);
    let cy = point.y.clamp(rect.y + r, rect.y + rect.height - r);
    let (dx, dy) = (point.x - cx, point.y - cy);
    dx * dx + dy * dy <= r * r
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

pub(crate) fn path_bounds(path: &Path) -> Rect {
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
                for p in [current_point, *p1, *p2] {
                    min_x = min_x.min(p.x);
                    min_y = min_y.min(p.y);
                    max_x = max_x.max(p.x);
                    max_y = max_y.max(p.y);
                }
                current_point = *p2;
            }
            PathCommand::CubicTo(p1, p2, p3) => {
                for p in [current_point, *p1, *p2, *p3] {
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

fn quadratic_bezier(p0: Point, p1: Point, p2: Point, t: f32) -> Point {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let t2 = t * t;

    Point::new(
        mt2 * p0.x + 2.0 * mt * t * p1.x + t2 * p2.x,
        mt2 * p0.y + 2.0 * mt * t * p1.y + t2 * p2.y,
    )
}

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

    #[test]
    fn path_bounds_hold_every_point_of_a_curve() {
        let (p0, p1, p2, p3) = (
            Point::new(0.0, 0.0),
            Point::new(7.0, 93.0),
            Point::new(61.0, -71.0),
            Point::new(100.0, 13.0),
        );
        let path = Path {
            commands: vec![PathCommand::MoveTo(p0), PathCommand::CubicTo(p1, p2, p3)],
            ..Path::new()
        };
        let b = path_bounds(&path);
        for i in 0..=1000u16 {
            let p = cubic_bezier(p0, p1, p2, p3, f32::from(i) / 1000.0);
            assert!(
                p.x >= b.x && p.x <= b.x + b.width && p.y >= b.y && p.y <= b.y + b.height,
                "{p:?} outside {b:?}"
            );
        }
    }
}
