use super::Point;

/// 2D transformation matrix (3x3 homogeneous coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Row 1, column 1.
    pub m11: f32,
    /// Row 1, column 2.
    pub m12: f32,
    /// Row 1, column 3.
    pub m13: f32,
    /// Row 2, column 1.
    pub m21: f32,
    /// Row 2, column 2.
    pub m22: f32,
    /// Row 2, column 3.
    pub m23: f32,
    /// Row 3, column 1.
    pub m31: f32,
    /// Row 3, column 2.
    pub m32: f32,
    /// Row 3, column 3.
    pub m33: f32,
}

impl Transform {
    /// Identity transform.
    pub fn identity() -> Self {
        Self {
            m11: 1.0,
            m12: 0.0,
            m13: 0.0,
            m21: 0.0,
            m22: 1.0,
            m23: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
        }
    }

    /// Translation by `(x, y)`.
    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            m11: 1.0,
            m12: 0.0,
            m13: 0.0,
            m21: 0.0,
            m22: 1.0,
            m23: 0.0,
            m31: x,
            m32: y,
            m33: 1.0,
        }
    }

    /// Scale by `(sx, sy)`.
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            m11: sx,
            m12: 0.0,
            m13: 0.0,
            m21: 0.0,
            m22: sy,
            m23: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
        }
    }

    /// Rotation by `angle` radians.
    pub fn rotate(angle: f32) -> Self {
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        Self {
            m11: cos_a,
            m12: sin_a,
            m13: 0.0,
            m21: -sin_a,
            m22: cos_a,
            m23: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
        }
    }

    /// Composes two transforms; `self` is applied first.
    pub fn multiply(self, other: Self) -> Self {
        Self {
            m11: self.m11 * other.m11 + self.m12 * other.m21 + self.m13 * other.m31,
            m12: self.m11 * other.m12 + self.m12 * other.m22 + self.m13 * other.m32,
            m13: self.m11 * other.m13 + self.m12 * other.m23 + self.m13 * other.m33,
            m21: self.m21 * other.m11 + self.m22 * other.m21 + self.m23 * other.m31,
            m22: self.m21 * other.m12 + self.m22 * other.m22 + self.m23 * other.m32,
            m23: self.m21 * other.m13 + self.m22 * other.m23 + self.m23 * other.m33,
            m31: self.m31 * other.m11 + self.m32 * other.m21 + self.m33 * other.m31,
            m32: self.m31 * other.m12 + self.m32 * other.m22 + self.m33 * other.m32,
            m33: self.m31 * other.m13 + self.m32 * other.m23 + self.m33 * other.m33,
        }
    }

    /// Inverse, or identity if the matrix is singular.
    pub fn inverse(self) -> Self {
        let det = self.m11 * (self.m22 * self.m33 - self.m23 * self.m32)
            - self.m12 * (self.m21 * self.m33 - self.m23 * self.m31)
            + self.m13 * (self.m21 * self.m32 - self.m22 * self.m31);

        if det.abs() < 1e-10 {
            return Self::identity();
        }

        let inv_det = 1.0 / det;
        Self {
            m11: (self.m22 * self.m33 - self.m23 * self.m32) * inv_det,
            m12: (self.m13 * self.m32 - self.m12 * self.m33) * inv_det,
            m13: (self.m12 * self.m23 - self.m13 * self.m22) * inv_det,
            m21: (self.m23 * self.m31 - self.m21 * self.m33) * inv_det,
            m22: (self.m11 * self.m33 - self.m13 * self.m31) * inv_det,
            m23: (self.m13 * self.m21 - self.m11 * self.m23) * inv_det,
            m31: (self.m21 * self.m32 - self.m22 * self.m31) * inv_det,
            m32: (self.m12 * self.m31 - self.m11 * self.m32) * inv_det,
            m33: (self.m11 * self.m22 - self.m12 * self.m21) * inv_det,
        }
    }

    /// Maps a point through the matrix using the row-vector convention documented on [`Transform`].
    pub fn map_point(self, point: Point) -> Point {
        Point {
            x: self.m11 * point.x + self.m21 * point.y + self.m31,
            y: self.m12 * point.x + self.m22 * point.y + self.m32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn assert_pt(actual: Point, x: f32, y: f32) {
        assert!(
            (actual.x - x).abs() < 1e-4 && (actual.y - y).abs() < 1e-4,
            "expected ({x}, {y}), got ({}, {})",
            actual.x,
            actual.y
        );
    }

    #[test]
    fn identity_maps_point_unchanged() {
        assert_pt(
            Transform::identity().map_point(Point::new(3.0, -7.0)),
            3.0,
            -7.0,
        );
    }

    #[test]
    fn translate_maps_origin_to_offset() {
        let t = Transform::translate(100.0, 50.0);
        assert_pt(t.map_point(Point::new(0.0, 0.0)), 100.0, 50.0);
        assert_pt(t.map_point(Point::new(5.0, 5.0)), 105.0, 55.0);
    }

    #[test]
    fn scale_maps_point() {
        assert_pt(
            Transform::scale(2.0, 3.0).map_point(Point::new(4.0, 5.0)),
            8.0,
            15.0,
        );
    }

    #[test]
    fn rotate_quarter_turn_maps_unit_x_to_unit_y() {
        let t = Transform::rotate(FRAC_PI_2);
        assert_pt(t.map_point(Point::new(1.0, 0.0)), 0.0, 1.0);
        assert_pt(t.map_point(Point::new(0.0, 1.0)), -1.0, 0.0);
    }

    #[test]
    fn inverse_round_trips() {
        let t = Transform::scale(2.0, 4.0)
            .multiply(Transform::rotate(0.7))
            .multiply(Transform::translate(30.0, -10.0));
        let p = Point::new(11.0, -3.0);
        assert_pt(t.inverse().map_point(t.map_point(p)), p.x, p.y);
    }

    #[test]
    fn multiply_applies_left_operand_first() {
        let t = Transform::scale(2.0, 2.0).multiply(Transform::translate(10.0, 0.0));
        assert_pt(t.map_point(Point::new(1.0, 0.0)), 12.0, 0.0);
    }
}
