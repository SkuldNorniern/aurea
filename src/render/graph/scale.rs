//! Turning data values into pixels and back.
//!
//! Every view in this module goes through [`Mapping`], including hit testing
//! and cursor readout, so there is one definition of where a value sits and no
//! second copy to drift out of step with the drawing.

use super::numeric::narrow;

/// How values are spaced along an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scale {
    /// Even spacing.
    #[default]
    Linear,
    /// Even spacing per decade. Values at or below zero have no place on a log
    /// axis and are reported as off-scale rather than drawn somewhere made up.
    Log10,
}

/// A closed interval of data values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl Range {
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    /// Width of the interval. Negative if it runs backwards.
    pub fn span(self) -> f64 {
        self.max - self.min
    }

    /// Whether the interval covers no ground, which would make a mapping
    /// divide by zero.
    pub fn is_degenerate(self) -> bool {
        !self.span().is_finite() || self.span().abs() < f64::EPSILON
    }

    /// Grows the interval by `fraction` of its span on both sides.
    ///
    /// Keeps a trace off the edge of the frame when the range came from the
    /// data itself.
    pub fn padded(self, fraction: f64) -> Self {
        let pad = self.span().abs() * fraction;
        Self::new(self.min - pad, self.max + pad)
    }

    /// Widens a flat interval so a constant signal still has somewhere to sit.
    pub fn or_widened(self, minimum_span: f64) -> Self {
        if !self.is_degenerate() {
            return self;
        }
        let mid = if self.min.is_finite() { self.min } else { 0.0 };
        let half = minimum_span.abs().max(f64::EPSILON) / 2.0;
        Self::new(mid - half, mid + half)
    }

    /// Whether `value` falls inside, ends included.
    pub fn contains(self, value: f64) -> bool {
        let (lo, hi) = if self.min <= self.max {
            (self.min, self.max)
        } else {
            (self.max, self.min)
        };
        value >= lo && value <= hi
    }
}

/// Where a value ended up relative to the drawn area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placed {
    /// Inside the axis, at this pixel.
    At(f32),
    /// Outside the axis, at this pixel. Still positioned, so a caller can clip
    /// a line to the edge instead of dropping the segment.
    Outside(f32),
    /// No position at all: NaN, or a non-positive value on a log axis.
    Nowhere,
}

impl Placed {
    /// The pixel, whether or not it landed inside.
    pub fn pixel(self) -> Option<f32> {
        match self {
            Self::At(p) | Self::Outside(p) => Some(p),
            Self::Nowhere => None,
        }
    }

    /// Whether the value landed inside the axis.
    pub fn is_inside(self) -> bool {
        matches!(self, Self::At(_))
    }
}

/// Maps a data range onto a pixel span.
#[derive(Debug, Clone, Copy)]
pub struct Mapping {
    range: Range,
    scale: Scale,
    pixel_min: f32,
    pixel_max: f32,
}

impl Mapping {
    /// Maps `range` onto `pixel_min..pixel_max`.
    ///
    /// For a vertical axis pass the pixel bounds the way the screen runs, with
    /// `pixel_min` at the bottom, and larger values come out higher up.
    pub fn new(range: Range, scale: Scale, pixel_min: f32, pixel_max: f32) -> Self {
        Self {
            range,
            scale,
            pixel_min,
            pixel_max,
        }
    }

    pub fn range(&self) -> Range {
        self.range
    }

    pub fn scale(&self) -> Scale {
        self.scale
    }

    /// The pixel span, as it was given.
    pub fn pixels(&self) -> (f32, f32) {
        (self.pixel_min, self.pixel_max)
    }

    /// Position of `value` along the axis, 0 at the range minimum and 1 at the
    /// maximum.
    fn normalise(&self, value: f64) -> Option<f64> {
        if !value.is_finite() {
            return None;
        }
        match self.scale {
            Scale::Linear => {
                if self.range.is_degenerate() {
                    return None;
                }
                Some((value - self.range.min) / self.range.span())
            }
            Scale::Log10 => {
                if value <= 0.0 || self.range.min <= 0.0 || self.range.max <= 0.0 {
                    return None;
                }
                let lo = self.range.min.log10();
                let hi = self.range.max.log10();
                if (hi - lo).abs() < f64::EPSILON {
                    return None;
                }
                Some((value.log10() - lo) / (hi - lo))
            }
        }
    }

    /// Where `value` sits in pixels.
    pub fn place(&self, value: f64) -> Placed {
        let Some(t) = self.normalise(value) else {
            return Placed::Nowhere;
        };
        let lo = f64::from(self.pixel_min);
        let hi = f64::from(self.pixel_max);
        let pixel = narrow(lo + t * (hi - lo));
        if (0.0..=1.0).contains(&t) {
            Placed::At(pixel)
        } else {
            Placed::Outside(pixel)
        }
    }

    /// The data value at `pixel`, the inverse of [`Self::place`]. This is what
    /// a cursor readout uses.
    pub fn value_at(&self, pixel: f32) -> Option<f64> {
        let lo = f64::from(self.pixel_min);
        let hi = f64::from(self.pixel_max);
        if (hi - lo).abs() < f64::EPSILON {
            return None;
        }
        let t = (f64::from(pixel) - lo) / (hi - lo);
        match self.scale {
            Scale::Linear => {
                if self.range.is_degenerate() {
                    return None;
                }
                Some(self.range.min + t * self.range.span())
            }
            Scale::Log10 => {
                if self.range.min <= 0.0 || self.range.max <= 0.0 {
                    return None;
                }
                let low = self.range.min.log10();
                let high = self.range.max.log10();
                Some(10f64.powf(low + t * (high - low)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear() -> Mapping {
        Mapping::new(Range::new(0.0, 10.0), Scale::Linear, 0.0, 100.0)
    }

    #[test]
    fn linear_maps_the_ends_and_the_middle() {
        let m = linear();
        assert_eq!(m.place(0.0), Placed::At(0.0));
        assert_eq!(m.place(10.0), Placed::At(100.0));
        assert_eq!(m.place(5.0), Placed::At(50.0));
    }

    #[test]
    fn a_reversed_pixel_span_puts_larger_values_higher() {
        // Screen y grows downward, so a vertical axis maps min to the bottom.
        let m = Mapping::new(Range::new(0.0, 10.0), Scale::Linear, 100.0, 0.0);
        assert_eq!(m.place(0.0), Placed::At(100.0));
        assert_eq!(m.place(10.0), Placed::At(0.0));
    }

    #[test]
    fn values_outside_the_range_keep_a_position() {
        let m = linear();
        match m.place(20.0) {
            Placed::Outside(p) => assert!((p - 200.0).abs() < 1e-4),
            other => panic!("expected Outside, got {other:?}"),
        }
        assert!(!m.place(20.0).is_inside());
    }

    #[test]
    fn nan_has_no_position() {
        assert_eq!(linear().place(f64::NAN), Placed::Nowhere);
        assert_eq!(linear().place(f64::INFINITY), Placed::Nowhere);
    }

    #[test]
    fn a_flat_range_has_no_position() {
        let m = Mapping::new(Range::new(5.0, 5.0), Scale::Linear, 0.0, 100.0);
        assert_eq!(m.place(5.0), Placed::Nowhere);
    }

    #[test]
    fn value_at_inverts_place() {
        let m = linear();
        for value in [0.0, 2.5, 7.5, 10.0] {
            let pixel = m.place(value).pixel().expect("placed");
            let back = m.value_at(pixel).expect("inverted");
            assert!((back - value).abs() < 1e-6, "{value} -> {pixel} -> {back}");
        }
    }

    #[test]
    fn log_maps_decades_evenly() {
        let m = Mapping::new(Range::new(1.0, 1000.0), Scale::Log10, 0.0, 300.0);
        assert_eq!(m.place(1.0), Placed::At(0.0));
        assert_eq!(m.place(1000.0), Placed::At(300.0));
        match m.place(10.0) {
            Placed::At(p) => assert!((p - 100.0).abs() < 1e-3, "got {p}"),
            other => panic!("expected At, got {other:?}"),
        }
    }

    #[test]
    fn log_has_no_place_for_zero_or_negative() {
        let m = Mapping::new(Range::new(1.0, 1000.0), Scale::Log10, 0.0, 300.0);
        assert_eq!(m.place(0.0), Placed::Nowhere);
        assert_eq!(m.place(-5.0), Placed::Nowhere);
    }

    #[test]
    fn log_value_at_inverts_place() {
        let m = Mapping::new(Range::new(1.0, 1000.0), Scale::Log10, 0.0, 300.0);
        for value in [1.0, 10.0, 100.0, 1000.0] {
            let pixel = m.place(value).pixel().expect("placed");
            let back = m.value_at(pixel).expect("inverted");
            assert!((back - value).abs() < 1e-6, "{value} -> {pixel} -> {back}");
        }
    }

    #[test]
    fn a_flat_range_widens_so_a_constant_signal_still_fits() {
        let widened = Range::new(3.0, 3.0).or_widened(2.0);
        assert!(!widened.is_degenerate());
        assert_eq!(widened, Range::new(2.0, 4.0));
    }

    #[test]
    fn a_real_range_is_left_alone_by_widening() {
        let range = Range::new(0.0, 10.0);
        assert_eq!(range.or_widened(2.0), range);
    }

    #[test]
    fn padding_grows_both_ends() {
        assert_eq!(Range::new(0.0, 10.0).padded(0.1), Range::new(-1.0, 11.0));
    }

    #[test]
    fn contains_works_on_a_backwards_range() {
        assert!(Range::new(10.0, 0.0).contains(5.0));
        assert!(!Range::new(10.0, 0.0).contains(11.0));
    }
}
