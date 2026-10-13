//! A plot's data: series, axes and cursors, and the view the user zoomed to.

use crate::render::{Point, Rect};
use crate::ui::component::PlotView;

use super::axis::{Axis, Cursor};
use super::project::Projection;
use super::scale::{Mapping, Range};
use super::series::Series;

/// A plot with axes and any number of series.
#[derive(Debug, Clone)]
pub struct Graph {
    pub x: Axis,
    pub y: Axis,
    pub series: Vec<Series>,
    pub cursors: Vec<Cursor>,
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
            view: PlotView::new(),
            plot_area: Rect::default(),
            projection: Projection::default(),
        }
    }

    pub fn with_x(mut self, axis: Axis) -> Self {
        self.x = axis;
        self
    }

    pub fn with_y(mut self, axis: Axis) -> Self {
        self.y = axis;
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
