//! What a set of points looks like once it is drawn.
//!
//! A series holds its data and how to draw it, and nothing about where it goes
//! on screen. Placement is the plot's job, so the same series can be drawn into
//! two plots at different scales.

use aurea_render::Color;

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
///
/// A rolling series carries only y values and gets its x from the sample index,
/// which is what a live feed produces. An explicit series carries both, for
/// data that is not evenly spaced.
#[derive(Debug, Clone)]
pub enum Points {
    /// Evenly spaced y values. `x_start` is the x of the oldest sample and
    /// `x_step` the distance to the next.
    Rolling {
        samples: SampleBuffer,
        x_start: f64,
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
    pub fn iter(&self) -> Box<dyn Iterator<Item = (f64, f64)> + '_> {
        match self {
            Self::Rolling {
                samples,
                x_start,
                x_step,
            } => {
                Box::new(samples.iter().enumerate().map(move |(i, y)| {
                    (x_step.mul_add(super::numeric::count_to_f64(i), *x_start), y)
                }))
            }
            Self::Xy(points) => Box::new(points.iter().copied()),
        }
    }

    /// Smallest and largest x and y, skipping points that are not finite.
    ///
    /// `None` when nothing is plottable, which is what tells a plot to keep
    /// whatever range it had rather than collapsing to nothing.
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

/// One plotted set of points.
#[derive(Debug, Clone)]
pub struct Series {
    /// Shown in a legend and in cursor readouts.
    pub name: String,
    pub points: Points,
    pub plot: Plot,
    /// `None` takes the next colour from the style palette.
    pub color: Option<Color>,
    pub width: f32,
    /// Radius of the mark for [`Plot::Points`], and of the point markers drawn
    /// on a line when [`Self::show_points`] is set.
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

    pub fn with_plot(mut self, plot: Plot) -> Self {
        self.plot = plot;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn with_points_shown(mut self, shown: bool) -> Self {
        self.show_points = shown;
        self
    }

    pub fn with_baseline(mut self, baseline: f64) -> Self {
        self.baseline = baseline;
        self
    }

    pub fn with_fill_alpha(mut self, alpha: f32) -> Self {
        self.fill_alpha = alpha.clamp(0.0, 1.0);
        self
    }

    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Adds a sample to a rolling series. Does nothing to an explicit one.
    pub fn push(&mut self, value: f64) {
        if let Points::Rolling { samples, .. } = &mut self.points {
            samples.push(value);
        }
    }

    /// Replaces the points of an explicit series. Does nothing to a rolling one.
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

        // The window slid, so x still starts at x_start and steps evenly.
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
