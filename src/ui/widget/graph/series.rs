//! What a set of points looks like once it is drawn.

use std::iter::Chain;
use std::slice;

use crate::render::Color;

use super::buffer::SampleBuffer;

/// How the points are joined up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Plot {
    /// Straight lines between points.
    #[default]
    Line,
    /// Held flat, then a jump: right for digital signals and counters.
    Step,
    /// No lines, a mark at each point.
    Points,
    /// A line with the area under it filled down to the baseline.
    Area,
    /// A vertical bar from the baseline to each point.
    Bars,
}

/// Where the points come from.
#[derive(Debug, Clone)]
pub enum Points {
    /// Evenly spaced y values.
    Rolling {
        /// The y values, oldest first.
        samples: SampleBuffer,
        /// X of the first sample.
        x_start: f64,
        /// X distance between samples.
        x_step: f64,
    },
    /// Explicit pairs, in the order they are drawn.
    Xy(Vec<(f64, f64)>),
}

impl Points {
    /// A rolling series holding `capacity` samples, one per x unit.
    pub fn rolling(capacity: usize) -> Self {
        Self::Rolling {
            samples: SampleBuffer::with_capacity(capacity),
            x_start: 0.0,
            x_step: 1.0,
        }
    }

    /// How many points there are.
    pub fn len(&self) -> usize {
        match self {
            Self::Rolling { samples, .. } => samples.len(),
            Self::Xy(points) => points.len(),
        }
    }

    /// Whether there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The points, in drawing order.
    pub fn iter(&self) -> PointIter<'_> {
        match self {
            Self::Rolling {
                samples,
                x_start,
                x_step,
            } => {
                let (head, tail) = samples.slices();
                PointIter::Rolling {
                    samples: head.iter().chain(tail),
                    index: 0,
                    x_start: *x_start,
                    x_step: *x_step,
                }
            }
            Self::Xy(points) => PointIter::Xy(points.iter()),
        }
    }

    /// Smallest and largest x and y, skipping points that are not finite.
    pub fn extent(&self) -> Option<(super::Range, super::Range)> {
        let mut x_min = f64::INFINITY;
        let mut x_max = f64::NEG_INFINITY;
        let mut y_min = f64::INFINITY;
        let mut y_max = f64::NEG_INFINITY;

        for (x, y) in self.iter().filter(|(x, y)| x.is_finite() && y.is_finite()) {
            x_min = x_min.min(x);
            x_max = x_max.max(x);
            y_min = y_min.min(y);
            y_max = y_max.max(y);
        }

        if x_min.is_finite() {
            Some((
                super::Range::new(x_min, x_max),
                super::Range::new(y_min, y_max),
            ))
        } else {
            None
        }
    }
}

/// The points of a [`Points`], oldest first.
pub enum PointIter<'p> {
    /// A rolling series, walked in order.
    Rolling {
        /// The sample halves, oldest first.
        samples: Chain<slice::Iter<'p, f64>, slice::Iter<'p, f64>>,
        /// Index of the next sample.
        index: usize,
        /// X of the first sample.
        x_start: f64,
        /// X distance between samples.
        x_step: f64,
    },
    /// An explicit series.
    Xy(slice::Iter<'p, (f64, f64)>),
}

impl Iterator for PointIter<'_> {
    type Item = (f64, f64);

    fn next(&mut self) -> Option<(f64, f64)> {
        match self {
            Self::Rolling {
                samples,
                index,
                x_start,
                x_step,
            } => {
                let y = *samples.next()?;
                let x = x_step.mul_add(super::numeric::count_to_f64(*index), *x_start);
                *index += 1;
                Some((x, y))
            }
            Self::Xy(points) => points.next().copied(),
        }
    }
}

/// One plotted set of points.
#[derive(Debug, Clone)]
pub struct Series {
    /// Shown in a legend and in cursor readouts.
    pub name: String,
    /// The points to draw.
    pub points: Points,
    /// How the points are joined.
    pub plot: Plot,
    /// `None` takes the next colour from the style palette.
    pub color: Option<Color>,
    /// Line width in pixels.
    pub width: f32,
    /// Radius of the mark for [`Plot::Points`], and of the point markers drawn on a line when
    /// [`Self::show_points`] is set.
    pub point_radius: f32,
    /// Draw a mark at every point as well as the line.
    pub show_points: bool,
    /// Where [`Plot::Area`] and [`Plot::Bars`] fill down to.
    pub baseline: f64,
    /// Fill opacity for [`Plot::Area`], 0 to 1.
    pub fill_alpha: f32,
    /// A hidden series keeps its data and its palette slot.
    pub visible: bool,
}

impl Series {
    /// A named series with no points yet.
    pub fn new(name: impl Into<String>, points: Points) -> Self {
        Self {
            name: name.into(),
            points,
            plot: Plot::Line,
            color: None,
            width: 1.5,
            point_radius: 2.0,
            show_points: false,
            baseline: 0.0,
            fill_alpha: 0.25,
            visible: true,
        }
    }

    /// A rolling series fed one sample at a time.
    pub fn rolling(name: impl Into<String>, capacity: usize) -> Self {
        Self::new(name, Points::rolling(capacity))
    }

    /// A series over explicit pairs.
    pub fn xy(name: impl Into<String>, points: Vec<(f64, f64)>) -> Self {
        Self::new(name, Points::Xy(points))
    }

    /// Sets how the points are drawn.
    pub fn with_plot(mut self, plot: Plot) -> Self {
        self.plot = plot;
        self
    }

    /// Sets an explicit colour instead of the palette's next one.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the line width in pixels.
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Sets whether a mark is drawn at each point.
    pub fn with_points_shown(mut self, shown: bool) -> Self {
        self.show_points = shown;
        self
    }

    /// Sets the level that areas and bars fill down to.
    pub fn with_baseline(mut self, baseline: f64) -> Self {
        self.baseline = baseline;
        self
    }

    /// Sets the area's fill opacity, clamped to 0 to 1.
    pub fn with_fill_alpha(mut self, alpha: f32) -> Self {
        self.fill_alpha = alpha.clamp(0.0, 1.0);
        self
    }

    /// Sets whether the series is drawn.
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Adds a sample to a rolling series.
    pub fn push(&mut self, value: f64) {
        if let Points::Rolling { samples, .. } = &mut self.points {
            samples.push(value);
        }
    }

    /// Replaces the points of an explicit series.
    pub fn set_points(&mut self, points: Vec<(f64, f64)>) {
        if let Points::Xy(existing) = &mut self.points {
            *existing = points;
        }
    }

    /// Drops every point, keeping the capacity of a rolling series.
    pub fn clear(&mut self) {
        match &mut self.points {
            Points::Rolling { samples, .. } => samples.clear(),
            Points::Xy(points) => points.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rolling_series_gets_x_from_the_sample_index() {
        let mut series = Series::rolling("s", 8);
        series.push(10.0);
        series.push(20.0);
        series.push(30.0);

        let points: Vec<(f64, f64)> = series.points.iter().collect();
        assert_eq!(points, vec![(0.0, 10.0), (1.0, 20.0), (2.0, 30.0)]);
    }

    #[test]
    fn a_rolling_series_keeps_x_spacing_after_it_wraps() {
        let mut series = Series::rolling("s", 3);
        for v in 0..5 {
            series.push(f64::from(v));
        }

        let points: Vec<(f64, f64)> = series.points.iter().collect();
        assert_eq!(points, vec![(0.0, 2.0), (1.0, 3.0), (2.0, 4.0)]);
    }

    #[test]
    fn an_explicit_series_keeps_its_pairs() {
        let series = Series::xy("s", vec![(0.5, 1.0), (2.5, -1.0)]);
        assert_eq!(
            series.points.iter().collect::<Vec<_>>(),
            vec![(0.5, 1.0), (2.5, -1.0)]
        );
    }

    #[test]
    fn extent_covers_both_axes() {
        let series = Series::xy("s", vec![(0.0, 5.0), (10.0, -5.0), (4.0, 1.0)]);
        let (x, y) = series.points.extent().expect("has points");
        assert_eq!(x, super::super::Range::new(0.0, 10.0));
        assert_eq!(y, super::super::Range::new(-5.0, 5.0));
    }

    #[test]
    fn extent_skips_points_that_are_not_finite() {
        let series = Series::xy("s", vec![(0.0, 1.0), (f64::NAN, 5.0), (2.0, f64::NAN)]);
        let (x, y) = series.points.extent().expect("has one good point");
        assert_eq!(x, super::super::Range::new(0.0, 0.0));
        assert_eq!(y, super::super::Range::new(1.0, 1.0));
    }

    #[test]
    fn an_empty_series_has_no_extent() {
        assert!(Series::rolling("s", 4).points.extent().is_none());
        assert!(Series::xy("s", Vec::new()).points.extent().is_none());
    }

    #[test]
    fn pushing_to_an_explicit_series_does_nothing() {
        let mut series = Series::xy("s", vec![(0.0, 0.0)]);
        series.push(9.0);
        assert_eq!(series.points.len(), 1);
    }

    #[test]
    fn clear_keeps_rolling_capacity() {
        let mut series = Series::rolling("s", 16);
        series.push(1.0);
        series.clear();

        assert!(series.points.is_empty());
        series.push(2.0);
        assert_eq!(series.points.len(), 1);
    }

    #[test]
    fn fill_alpha_is_clamped() {
        assert_eq!(Series::rolling("s", 1).with_fill_alpha(5.0).fill_alpha, 1.0);
        assert_eq!(
            Series::rolling("s", 1).with_fill_alpha(-1.0).fill_alpha,
            0.0
        );
    }
}
