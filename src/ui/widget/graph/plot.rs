//! A plot's data: series, axes and cursors, and the view the user zoomed to.

use crate::render::{Point, Rect};
use crate::ui::component::PlotView;

use super::axis::{Axis, Cursor};
use super::project::Projection;
use super::scale::{Mapping, Range};
use super::series::Series;

/// Where a plot's legend sits, inside the plotting area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LegendPlacement {
    /// No legend.
    Off,
    /// In the top right, the default.
    #[default]
    TopRight,
    /// In the top left.
    TopLeft,
    /// In the bottom right.
    BottomRight,
    /// In the bottom left.
    BottomLeft,
}

/// A plot with axes and any number of series.
#[derive(Debug, Clone)]
pub struct Graph {
    /// The x axis.
    pub x: Axis,
    /// The y axis.
    pub y: Axis,
    /// The series drawn on the plot.
    pub series: Vec<Series>,
    /// Cursors drawn on the plot.
    pub cursors: Vec<Cursor>,
    /// Where the legend of named series sits.
    pub legend: LegendPlacement,
    /// What the user zoomed and panned to, and what is under the pointer.
    pub view: PlotView,
    plot_area: Rect,
    pub(crate) projection: Projection,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    /// A plot with auto axes.
    pub fn new() -> Self {
        Self {
            x: Axis::auto(),
            y: Axis::auto(),
            series: Vec::new(),
            cursors: Vec::new(),
            legend: LegendPlacement::default(),
            view: PlotView::new(),
            plot_area: Rect::default(),
            projection: Projection::default(),
        }
    }

    /// Sets the x axis.
    pub fn with_x(mut self, axis: Axis) -> Self {
        self.x = axis;
        self
    }

    /// Sets the y axis.
    pub fn with_y(mut self, axis: Axis) -> Self {
        self.y = axis;
        self
    }

    /// Sets where the legend sits; [`LegendPlacement::Off`] hides it.
    pub fn with_legend(mut self, legend: LegendPlacement) -> Self {
        self.legend = legend;
        self
    }

    /// Adds a series and hands back its index, for later lookup by index.
    pub fn add_series(&mut self, series: Series) -> usize {
        self.series.push(series);
        self.series.len() - 1
    }

    /// The series at `index`, for feeding samples in.
    pub fn series_mut(&mut self, index: usize) -> Option<&mut Series> {
        self.series.get_mut(index)
    }

    /// The series named `name`.
    pub fn series_by_name(&mut self, name: &str) -> Option<&mut Series> {
        self.series.iter_mut().find(|s| s.name == name)
    }

    /// Adds a sample to the series at `index`, if there is one.
    pub fn push(&mut self, index: usize, value: f64) {
        if let Some(series) = self.series.get_mut(index) {
            series.push(value);
        }
    }

    /// Drops the points of every series, keeping the series themselves.
    pub fn clear(&mut self) {
        for series in &mut self.series {
            series.clear();
        }
    }

    /// The plotting area from the last frame, without the axis margins.
    pub fn plot_area(&self) -> Rect {
        self.plot_area
    }

    /// The data value under a pixel.
    pub fn value_at(&self, point: Point) -> Option<(f64, f64)> {
        if self.plot_area.width <= 0.0 || !self.plot_area.contains(point) {
            return None;
        }
        let (x_map, y_map) = self.mappings();
        Some((x_map.value_at(point.x)?, y_map.value_at(point.y)?))
    }

    /// The visible, named series' point nearest `at` in pixels, within `reach`, with its name
    /// and its data values.
    pub(crate) fn nearest_named(&self, at: Point, reach: f32) -> Option<(&str, f64, f64)> {
        let (x_map, y_map) = self.mappings();
        let mut best: Option<(f32, &str, f64, f64)> = None;
        for series in self
            .series
            .iter()
            .filter(|s| s.visible && !s.name.is_empty())
        {
            for (x, y) in series.points.iter() {
                let (Some(px), Some(py)) = (x_map.place(x).pixel(), y_map.place(y).pixel()) else {
                    continue;
                };
                let d = (px - at.x).hypot(py - at.y);
                if d <= reach && best.is_none_or(|(b, ..)| d < b) {
                    best = Some((d, &series.name, x, y));
                }
            }
        }
        best.map(|(_, name, x, y)| (name, x, y))
    }

    pub(crate) fn resolve(&mut self, area: Rect) {
        let (data_x, data_y) = if self.x.bounds.needs_data() || self.y.bounds.needs_data() {
            self.data_extent()
        } else {
            (None, None)
        };
        self.x.resolve(data_x, self.view.x());
        self.y.resolve(data_y, self.view.y());
        self.plot_area = area;
    }

    pub(crate) fn mappings(&self) -> (Mapping, Mapping) {
        let area = self.plot_area;
        (
            Mapping::new(self.x.range(), self.x.scale, area.x, area.right()),
            Mapping::new(self.y.range(), self.y.scale, area.bottom(), area.y),
        )
    }

    fn data_extent(&self) -> (Option<Range>, Option<Range>) {
        let mut x: Option<Range> = None;
        let mut y: Option<Range> = None;
        for (sx, sy) in self
            .series
            .iter()
            .filter(|s| s.visible)
            .filter_map(|s| s.points.extent())
        {
            x = Some(x.map_or(sx, |r| r.union(sx)));
            y = Some(y.map_or(sy, |r| r.union(sy)));
        }
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_extent_covers_every_visible_series() {
        let mut graph = Graph::new();
        graph.add_series(Series::xy("a", vec![(0.0, 0.0), (5.0, 5.0)]));
        graph.add_series(Series::xy("b", vec![(-2.0, 9.0)]));
        graph.add_series(Series::xy("c", vec![(100.0, 100.0)]).with_visible(false));
        let (x, y) = graph.data_extent();
        assert_eq!(x, Some(Range::new(-2.0, 5.0)));
        assert_eq!(y, Some(Range::new(0.0, 9.0)));
    }

    #[test]
    fn value_at_inverts_the_mapping() {
        let mut graph = Graph::new()
            .with_x(Axis::fixed(0.0, 10.0))
            .with_y(Axis::fixed(0.0, 100.0));
        assert_eq!(
            graph.value_at(Point::new(10.0, 10.0)),
            None,
            "not drawn yet"
        );
        graph.resolve(Rect::new(20.0, 10.0, 200.0, 100.0));
        let (x, y) = graph.value_at(Point::new(120.0, 60.0)).expect("inside");
        assert!(
            (x - 5.0).abs() < 1e-6 && (y - 50.0).abs() < 1e-6,
            "{x}, {y}"
        );
        assert_eq!(graph.value_at(Point::new(1.0, 1.0)), None, "in the margin");
    }

    #[test]
    fn a_fixed_plot_keeps_the_range_it_was_given() {
        let mut graph = Graph::new()
            .with_x(Axis::fixed(0.0, 10.0))
            .with_y(Axis::fixed(-1.0, 1.0));
        graph.add_series(Series::xy("s", vec![(0.0, 0.0), (1000.0, 500.0)]));
        graph.resolve(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(graph.x.range(), Range::new(0.0, 10.0));
        assert_eq!(graph.y.range(), Range::new(-1.0, 1.0));
    }

    #[test]
    fn the_legend_is_top_right_unless_asked_otherwise() {
        assert_eq!(Graph::new().legend, LegendPlacement::TopRight);
        let off = Graph::new().with_legend(LegendPlacement::Off);
        assert_eq!(off.legend, LegendPlacement::Off);
    }

    #[test]
    fn the_nearest_named_point_is_found_within_reach() {
        let mut graph = Graph::new()
            .with_x(Axis::fixed(0.0, 100.0))
            .with_y(Axis::fixed(0.0, 100.0));
        graph.add_series(Series::xy("near", vec![(50.0, 50.0)]));
        graph.add_series(Series::xy("", vec![(10.0, 10.0)]));
        graph.resolve(Rect::new(0.0, 0.0, 100.0, 100.0));
        let at = Point::new(52.0, 49.0);
        assert_eq!(graph.nearest_named(at, 5.0), Some(("near", 50.0, 50.0)));
        assert_eq!(
            graph.nearest_named(Point::new(10.0, 90.0), 5.0),
            None,
            "unnamed"
        );
        assert_eq!(
            graph.nearest_named(Point::new(80.0, 20.0), 5.0),
            None,
            "too far"
        );
    }

    #[test]
    fn series_are_found_by_name_and_index() {
        let mut graph = Graph::new();
        let index = graph.add_series(Series::rolling("temperature", 4));
        graph.push(index, 1.0);
        graph.push(7, 1.0);
        assert!(graph.series_by_name("temperature").is_some());
        assert!(graph.series_by_name("missing").is_none());
        assert_eq!(graph.series[index].points.len(), 1);
    }
}
