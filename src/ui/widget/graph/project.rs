//! Data turned into screen points, thinned to what the screen can show.

use crate::render::{Point, Rect};

use super::numeric::{f32_to_i32, f64_to_count};
use super::scale::Mapping;
use super::series::Plot;

/// One series on screen, and the buffers that made it, kept between frames so a warm plot allocates
/// nothing.
#[derive(Debug, Clone, Default)]
pub struct Projection {
    points: Vec<Point>,
    envelope: bool,
    lo: Vec<f32>,
    hi: Vec<f32>,
    seen: Vec<u64>,
}

impl Projection {
    /// The points to draw, in drawing order.
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Whether [`Self::points`] are column spans: bottom then top, one pair per pixel column, or
    /// one point where the column is flat.
    pub fn envelope(&self) -> bool {
        self.envelope
    }

    /// Projects `data` (`len` points of it) for a `plot` drawn into `area`.
    pub fn project(
        &mut self,
        data: impl Iterator<Item = (f64, f64)>,
        len: usize,
        (plot, radius): (Plot, f32),
        (x_map, y_map): (Mapping, Mapping),
        area: Rect,
    ) {
        self.points.clear();
        self.envelope = false;
        let placed = data.filter_map(|(x, y)| {
            Some(Point::new(x_map.place(x).pixel()?, y_map.place(y).pixel()?))
        });
        let columns = area.width.max(1.0);
        if len <= decimation_threshold(columns) {
            self.points.extend(placed);
            return;
        }
        match plot {
            Plot::Line | Plot::Area | Plot::Step => {
                self.envelope = true;
                self.decimate(placed, area.x, columns);
            }
            Plot::Points => self.thin_to_cells(placed, area, radius),
            Plot::Bars => self.tallest_per_column(placed, area.x, columns),
        }
    }

    fn decimate(&mut self, points: impl Iterator<Item = Point>, left: f32, columns: f32) {
        let count = f64_to_count(f64::from(columns)).max(1);
        reset(&mut self.lo, count, f32::INFINITY);
        reset(&mut self.hi, count, f32::NEG_INFINITY);
        for point in points {
            let Some(column) = column_of(point, left, count) else {
                continue;
            };
            self.lo[column] = self.lo[column].min(point.y);
            self.hi[column] = self.hi[column].max(point.y);
        }
        for column in 0..count {
            let (lo, hi) = (self.lo[column], self.hi[column]);
            if lo > hi {
                continue;
            }
            let x = left + column as f32;
            self.points.push(Point::new(x, lo));
            if (hi - lo).abs() > f32::EPSILON {
                self.points.push(Point::new(x, hi));
            }
        }
    }

    fn thin_to_cells(&mut self, points: impl Iterator<Item = Point>, area: Rect, radius: f32) {
        let cell = radius.max(1.0);
        let width = f64_to_count(f64::from((area.width / cell).max(1.0)));
        let height = f64_to_count(f64::from((area.height / cell).max(1.0)));
        reset(&mut self.seen, width.saturating_mul(height).div_ceil(64), 0);
        for point in points {
            let x = f32_to_i32((point.x - area.x) / cell);
            let y = f32_to_i32((point.y - area.y) / cell);
            let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
                continue;
            };
            if x >= width || y >= height {
                continue;
            }
            let index = y * width + x;
            let (word, bit) = (index / 64, 1u64 << (index % 64));
            if self.seen[word] & bit == 0 {
                self.seen[word] |= bit;
                self.points.push(point);
            }
        }
    }

    fn tallest_per_column(&mut self, points: impl Iterator<Item = Point>, left: f32, columns: f32) {
        let count = f64_to_count(f64::from(columns)).max(1);
        reset(&mut self.lo, count, f32::INFINITY);
        for point in points {
            if let Some(column) = column_of(point, left, count) {
                self.lo[column] = self.lo[column].min(point.y);
            }
        }
        for (column, top) in self.lo.iter().enumerate() {
            if top.is_finite() {
                self.points.push(Point::new(left + column as f32, *top));
            }
        }
    }
}

fn reset<T: Copy>(buffer: &mut Vec<T>, len: usize, value: T) {
    buffer.clear();
    buffer.resize(len, value);
}

fn decimation_threshold(columns: f32) -> usize {
    f64_to_count(f64::from(columns * 2.0)).max(64)
}

fn column_of(point: Point, left: f32, count: usize) -> Option<usize> {
    let offset = point.x - left;
    if offset < 0.0 || offset > count as f32 {
        return None;
    }
    let column = usize::try_from(f32_to_i32(offset)).ok()?;
    Some(column.min(count.saturating_sub(1)))
}

pub(crate) fn bar_width(screen: &[Point]) -> f32 {
    let smallest = screen
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).abs())
        .filter(|gap| *gap > 0.0)
        .fold(f32::INFINITY, f32::min);
    if smallest.is_finite() {
        (smallest * 0.8).max(1.0)
    } else {
        6.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::graph::{Range, Scale};

    const AREA: Rect = Rect::new(0.0, 0.0, 800.0, 400.0);

    fn maps(x: (f64, f64), y: (f64, f64)) -> (Mapping, Mapping) {
        (
            Mapping::new(Range::new(x.0, x.1), Scale::Linear, 0.0, 800.0),
            Mapping::new(Range::new(y.0, y.1), Scale::Linear, 400.0, 0.0),
        )
    }

    fn project(points: &[(f64, f64)], plot: Plot, x: (f64, f64), y: (f64, f64)) -> Projection {
        let mut p = Projection::default();
        p.project(
            points.iter().copied(),
            points.len(),
            (plot, 2.0),
            maps(x, y),
            AREA,
        );
        p
    }

    fn square(count: u32) -> Vec<(f64, f64)> {
        (0..count)
            .map(|i| (f64::from(i), if i % 2 == 0 { -1.0 } else { 1.0 }))
            .collect()
    }

    fn decimated(points: &[Point], columns: f32) -> Vec<Point> {
        let mut p = Projection::default();
        p.decimate(points.iter().copied(), 0.0, columns);
        p.points
    }

    #[test]
    fn a_short_series_is_drawn_as_it_is() {
        let points: Vec<_> = (0..50).map(|i| (f64::from(i), 1.0)).collect();
        let p = project(&points, Plot::Line, (0.0, 50.0), (0.0, 2.0));
        assert!(!p.envelope());
        assert_eq!(p.points().len(), 50);
    }

    #[test]
    fn a_long_series_is_decimated_to_the_screen() {
        let p = project(&square(100_000), Plot::Line, (0.0, 100_000.0), (-1.0, 1.0));
        assert!(p.envelope());
        assert!(p.points().len() <= 2 * 800 + 2, "got {}", p.points().len());
    }

    #[test]
    fn a_dense_scatter_stays_a_scatter() {
        let p = project(
            &square(100_000),
            Plot::Points,
            (0.0, 100_000.0),
            (-1.0, 1.0),
        );
        assert!(!p.envelope(), "a scatter is not an envelope");
        assert!(p.points().len() < 100_000, "still thinned");
    }

    #[test]
    fn a_dense_bar_chart_keeps_one_bar_per_column() {
        let points: Vec<_> = (0..100_000).map(|i| (f64::from(i), 0.5)).collect();
        let p = project(&points, Plot::Bars, (0.0, 100_000.0), (0.0, 1.0));
        assert!(!p.envelope());
        assert!(p.points().len() <= 801, "got {} bars", p.points().len());
    }

    #[test]
    fn a_warm_projection_reuses_its_buffers() {
        let data = square(100_000);
        let mut p = project(&data, Plot::Line, (0.0, 100_000.0), (-1.0, 1.0));
        let buffer = p.points().as_ptr();
        p.project(
            data.iter().copied(),
            data.len(),
            (Plot::Line, 2.0),
            maps((0.0, 100_000.0), (-1.0, 1.0)),
            AREA,
        );
        assert_eq!(p.points().as_ptr(), buffer, "no new allocation");
    }

    #[test]
    fn decimation_keeps_the_highest_and_lowest_in_a_column() {
        let points = [
            Point::new(0.0, 5.0),
            Point::new(0.2, -3.0),
            Point::new(0.4, 9.0),
            Point::new(0.6, 1.0),
        ];
        let out = decimated(&points, 4.0);
        let lowest = out.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let highest = out.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        assert_eq!((lowest, highest), (-3.0, 9.0));
    }

    #[test]
    fn a_flat_column_yields_one_point() {
        let out = decimated(&[Point::new(0.0, 5.0), Point::new(0.3, 5.0)], 4.0);
        assert_eq!(out.len(), 1, "nothing to span");
    }

    #[test]
    fn an_out_of_order_series_gets_one_span_per_column() {
        let points = [
            Point::new(0.0, 10.0),
            Point::new(2.0, 5.0),
            Point::new(0.0, 30.0),
        ];
        let out = decimated(&points, 4.0);
        let first: Vec<_> = out.iter().filter(|p| p.x == 0.0).map(|p| p.y).collect();
        assert_eq!(first, vec![10.0, 30.0], "one bottom and one top");
    }

    #[test]
    fn a_point_on_the_far_edge_lands_in_the_last_column() {
        let out = decimated(&[Point::new(0.0, 1.0), Point::new(4.0, 9.0)], 4.0);
        assert!(
            out.iter().any(|p| p.y == 9.0),
            "edge value dropped: {out:?}"
        );
    }

    #[test]
    fn a_point_outside_the_plot_joins_no_column() {
        let out = decimated(&[Point::new(-50.0, 999.0), Point::new(1.0, 5.0)], 4.0);
        assert!(out.iter().all(|p| p.y == 5.0), "got {out:?}");
    }

    #[test]
    fn bar_width_comes_from_the_closest_pair() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(14.0, 0.0),
        ];
        assert!((bar_width(&points) - 3.2).abs() < 1e-4);
        assert!(bar_width(&[Point::new(0.0, 0.0)]) > 0.0, "a lone bar too");
    }
}
