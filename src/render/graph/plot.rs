//! The plot: layout, axes and drawing.
//!
//! A [`Graph`] holds series, ranges and a style, and draws itself into any
//! rect of a [`DrawingContext`]. It keeps no pixel state between frames, so it
//! is safe to redraw from a canvas draw callback, which is what the damage
//! tracker expects.

use aurea_foundation::AureaResult;
use aurea_render::{Color, DrawingContext, Paint, PaintStyle, Path, PathCommand, Point, Rect};

use super::scale::{Mapping, Placed, Range, Scale};
use super::series::{Plot, Points, Series};
use super::style::{AxisStyle, GraphStyle, Stroke};
use super::ticks::TickPlan;

/// How an axis decides what range to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bounds {
    /// A range the caller sets and the plot never changes.
    Fixed(Range),
    /// Grows to hold the data, with `padding` as a fraction of the span.
    Auto { padding: f64 },
    /// Follows the newest data, keeping a window this wide. What a live feed
    /// wants: the trace scrolls instead of squashing.
    Window { span: f64 },
}

impl Default for Bounds {
    fn default() -> Self {
        Self::Auto { padding: 0.05 }
    }
}

/// One axis of a plot.
#[derive(Debug, Clone)]
pub struct Axis {
    pub bounds: Bounds,
    pub scale: Scale,
    pub ticks: TickPlan,
    /// Written alongside the axis. Empty for none.
    pub label: String,
    /// The range in use, once [`Graph::draw`] has resolved [`Self::bounds`].
    resolved: Range,
}

impl Default for Axis {
    fn default() -> Self {
        Self {
            bounds: Bounds::default(),
            scale: Scale::Linear,
            ticks: TickPlan::default(),
            label: String::new(),
            resolved: Range::new(0.0, 1.0),
        }
    }
}

impl Axis {
    /// An axis pinned to a range.
    pub fn fixed(min: f64, max: f64) -> Self {
        Self {
            bounds: Bounds::Fixed(Range::new(min, max)),
            resolved: Range::new(min, max),
            ..Self::default()
        }
    }

    /// An axis that follows the data.
    pub fn auto() -> Self {
        Self::default()
    }

    /// An axis that keeps a window of the newest data.
    pub fn window(span: f64) -> Self {
        Self {
            bounds: Bounds::Window { span },
            ..Self::default()
        }
    }

    pub fn with_scale(mut self, scale: Scale) -> Self {
        self.scale = scale;
        self
    }

    pub fn with_ticks(mut self, ticks: TickPlan) -> Self {
        self.ticks = ticks;
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// The range currently drawn.
    pub fn range(&self) -> Range {
        self.resolved
    }

    /// Works out the range to draw from the data extent.
    fn resolve(&mut self, data: Option<Range>) {
        self.resolved = match self.bounds {
            Bounds::Fixed(range) => range,
            Bounds::Auto { padding } => match data {
                Some(range) => range.padded(padding).or_widened(1.0),
                None => self.resolved,
            },
            Bounds::Window { span } => match data {
                // Anchored to the newest value so the trace scrolls.
                Some(range) => Range::new(range.max - span.abs(), range.max),
                None => self.resolved,
            },
        };
        // A log axis cannot show zero, so lift a range that reaches it.
        if self.scale == Scale::Log10 && self.resolved.min <= 0.0 {
            let max = if self.resolved.max > 0.0 {
                self.resolved.max
            } else {
                1.0
            };
            self.resolved = Range::new(max / 1000.0, max);
        }
    }
}

/// A vertical or horizontal line at a value, with a readout.
#[derive(Debug, Clone)]
pub struct Cursor {
    /// Where it sits, in data values on its own axis.
    pub value: f64,
    /// Vertical cursors sit at an x value, horizontal ones at a y value.
    pub vertical: bool,
    pub stroke: Stroke,
    /// Written next to the cursor. Empty for none.
    pub label: String,
}

impl Cursor {
    pub fn vertical(value: f64, color: Color) -> Self {
        Self {
            value,
            vertical: true,
            stroke: Stroke::new(color, 1.0),
            label: String::new(),
        }
    }

    pub fn horizontal(value: f64, color: Color) -> Self {
        Self {
            value,
            vertical: false,
            stroke: Stroke::new(color, 1.0),
            label: String::new(),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

/// A plot with axes, a grid and any number of series.
#[derive(Debug, Clone)]
pub struct Graph {
    pub style: GraphStyle,
    pub x: Axis,
    pub y: Axis,
    pub series: Vec<Series>,
    pub cursors: Vec<Cursor>,
    /// The plotting area from the last [`Self::draw`], in pixels. Used to turn
    /// a pointer position back into data values.
    plot_area: Rect,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    /// A plot with auto axes and the dark theme.
    pub fn new() -> Self {
        Self {
            style: GraphStyle::default(),
            x: Axis::auto(),
            y: Axis::auto(),
            series: Vec::new(),
            cursors: Vec::new(),
            plot_area: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    pub fn with_style(mut self, style: GraphStyle) -> Self {
        self.style = style;
        self
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

    /// The plotting area from the last draw, without the axis margins.
    pub fn plot_area(&self) -> Rect {
        self.plot_area
    }

    /// The data value under a pixel, for a readout under the pointer.
    ///
    /// `None` before the first draw, or outside the plotting area.
    pub fn value_at(&self, point: Point) -> Option<(f64, f64)> {
        if self.plot_area.width <= 0.0 || !contains(self.plot_area, point) {
            return None;
        }
        let (x_map, y_map) = self.mappings();
        Some((x_map.value_at(point.x)?, y_map.value_at(point.y)?))
    }

    /// The mappings for the current plot area and resolved ranges.
    fn mappings(&self) -> (Mapping, Mapping) {
        let area = self.plot_area;
        (
            Mapping::new(self.x.resolved, self.x.scale, area.x, area.x + area.width),
            // Screen y grows downward, so the range minimum goes at the bottom.
            Mapping::new(self.y.resolved, self.y.scale, area.y + area.height, area.y),
        )
    }

    /// The extent of every visible series, or `None` when there is no data.
    fn data_extent(&self) -> (Option<Range>, Option<Range>) {
        let mut x: Option<Range> = None;
        let mut y: Option<Range> = None;
        for series in self.series.iter().filter(|s| s.visible) {
            let Some((sx, sy)) = series.points.extent() else {
                continue;
            };
            x = Some(match x {
                Some(r) => Range::new(r.min.min(sx.min), r.max.max(sx.max)),
                None => sx,
            });
            y = Some(match y {
                Some(r) => Range::new(r.min.min(sy.min), r.max.max(sy.max)),
                None => sy,
            });
        }
        (x, y)
    }

    /// Draws the plot into `area`.
    ///
    /// Ranges are resolved from the data first, so a caller that pushed samples
    /// this frame sees them without a separate update step.
    pub fn draw(&mut self, ctx: &mut dyn DrawingContext, area: Rect) -> AureaResult<()> {
        let (data_x, data_y) = self.data_extent();
        self.x.resolve(data_x);
        self.y.resolve(data_y);

        self.plot_area = inner_area(area, &self.style);

        if let Some(color) = self.style.background {
            ctx.draw_rect(area, &fill(color))?;
        }
        if self.plot_area.width <= 0.0 || self.plot_area.height <= 0.0 {
            // Nowhere to draw; the background is all that can be shown.
            return Ok(());
        }
        if let Some(color) = self.style.plot_background {
            ctx.draw_rect(self.plot_area, &fill(color))?;
        }

        let (x_map, y_map) = self.mappings();
        self.draw_grid(ctx, x_map, y_map)?;
        self.draw_series(ctx, x_map, y_map)?;
        self.draw_cursors(ctx, x_map, y_map)?;
        self.draw_axes(ctx, x_map, y_map)?;

        if let Some(border) = self.style.border {
            ctx.draw_rect(self.plot_area, &stroke(border))?;
        }
        Ok(())
    }

    fn draw_grid(
        &self,
        ctx: &mut dyn DrawingContext,
        x_map: Mapping,
        y_map: Mapping,
    ) -> AureaResult<()> {
        let area = self.plot_area;
        let grid = self.style.grid;

        if grid.show_vertical {
            for tick in self.x.ticks.ticks(self.x.resolved, self.x.scale) {
                let Placed::At(px) = x_map.place(tick.value) else {
                    continue;
                };
                let line = if tick.major { grid.major } else { grid.minor };
                vline(ctx, px, area.y, area.y + area.height, line)?;
            }
        }
        if grid.show_horizontal {
            for tick in self.y.ticks.ticks(self.y.resolved, self.y.scale) {
                let Placed::At(py) = y_map.place(tick.value) else {
                    continue;
                };
                let line = if tick.major { grid.major } else { grid.minor };
                hline(ctx, py, area.x, area.x + area.width, line)?;
            }
        }
        if let Some(zero) = grid.zero {
            if let Placed::At(py) = y_map.place(0.0) {
                hline(ctx, py, area.x, area.x + area.width, zero)?;
            }
            if let Placed::At(px) = x_map.place(0.0) {
                vline(ctx, px, area.y, area.y + area.height, zero)?;
            }
        }
        Ok(())
    }

    fn draw_series(
        &self,
        ctx: &mut dyn DrawingContext,
        x_map: Mapping,
        y_map: Mapping,
    ) -> AureaResult<()> {
        // Keep traces inside the frame; a sample outside the range would
        // otherwise be drawn over the axes and labels.
        ctx.save()?;
        ctx.clip_rect(self.plot_area)?;

        let result = self.draw_series_inner(ctx, x_map, y_map);
        ctx.restore()?;
        result
    }

    fn draw_series_inner(
        &self,
        ctx: &mut dyn DrawingContext,
        x_map: Mapping,
        y_map: Mapping,
    ) -> AureaResult<()> {
        for (index, series) in self.series.iter().enumerate() {
            if !series.visible || series.points.is_empty() {
                continue;
            }
            let color = series
                .color
                .unwrap_or_else(|| self.style.palette_color(index));
            let screen = self.project(&series.points, x_map, y_map);
            if screen.is_empty() {
                continue;
            }

            match series.plot {
                Plot::Line => stroke_polyline(ctx, &screen, color, series.width, false)?,
                Plot::Step => stroke_polyline(ctx, &screen, color, series.width, true)?,
                Plot::Points => {}
                Plot::Area => {
                    self.fill_area(ctx, &screen, series, color, y_map)?;
                    stroke_polyline(ctx, &screen, color, series.width, false)?;
                }
                Plot::Bars => self.draw_bars(ctx, &screen, series, color, y_map)?,
            }

            if series.show_points || series.plot == Plot::Points {
                for point in &screen {
                    ctx.draw_circle(*point, series.point_radius, &fill(color))?;
                }
            }
        }
        Ok(())
    }

    /// Data points turned into pixels, dropping any that have no position.
    fn project(&self, points: &Points, x_map: Mapping, y_map: Mapping) -> Vec<Point> {
        points
            .iter()
            .filter_map(|(x, y)| {
                let px = x_map.place(x).pixel()?;
                let py = y_map.place(y).pixel()?;
                Some(Point::new(px, py))
            })
            .collect()
    }

    fn fill_area(
        &self,
        ctx: &mut dyn DrawingContext,
        screen: &[Point],
        series: &Series,
        color: Color,
        y_map: Mapping,
    ) -> AureaResult<()> {
        let Some(first) = screen.first() else {
            return Ok(());
        };
        let Some(last) = screen.last() else {
            return Ok(());
        };
        let base = y_map
            .place(series.baseline)
            .pixel()
            .unwrap_or(self.plot_area.y + self.plot_area.height);

        let mut path = Path::new();
        path.commands
            .push(PathCommand::MoveTo(Point::new(first.x, base)));
        for point in screen {
            path.commands.push(PathCommand::LineTo(*point));
        }
        path.commands
            .push(PathCommand::LineTo(Point::new(last.x, base)));
        path.commands.push(PathCommand::Close);

        let faded = Color::rgba(
            color.r,
            color.g,
            color.b,
            scale_alpha(color.a, series.fill_alpha),
        );
        ctx.draw_path(&path, &fill(faded))
    }

    fn draw_bars(
        &self,
        ctx: &mut dyn DrawingContext,
        screen: &[Point],
        series: &Series,
        color: Color,
        y_map: Mapping,
    ) -> AureaResult<()> {
        let base = y_map
            .place(series.baseline)
            .pixel()
            .unwrap_or(self.plot_area.y + self.plot_area.height);
        // Bars fill the gap between neighbours, with a sliver of space.
        let width = bar_width(screen);

        for point in screen {
            let top = point.y.min(base);
            let height = (point.y - base).abs();
            if height <= 0.0 {
                continue;
            }
            ctx.draw_rect(
                Rect::new(point.x - width / 2.0, top, width, height),
                &fill(color),
            )?;
        }
        Ok(())
    }

    fn draw_cursors(
        &self,
        ctx: &mut dyn DrawingContext,
        x_map: Mapping,
        y_map: Mapping,
    ) -> AureaResult<()> {
        let area = self.plot_area;
        for cursor in &self.cursors {
            if cursor.vertical {
                if let Placed::At(px) = x_map.place(cursor.value) {
                    vline(ctx, px, area.y, area.y + area.height, cursor.stroke)?;
                    if !cursor.label.is_empty() {
                        let paint = fill(cursor.stroke.color);
                        ctx.draw_text(&cursor.label, Point::new(px + 3.0, area.y + 12.0), &paint)?;
                    }
                }
            } else if let Placed::At(py) = y_map.place(cursor.value) {
                hline(ctx, py, area.x, area.x + area.width, cursor.stroke)?;
                if !cursor.label.is_empty() {
                    let paint = fill(cursor.stroke.color);
                    ctx.draw_text(&cursor.label, Point::new(area.x + 3.0, py - 3.0), &paint)?;
                }
            }
        }
        Ok(())
    }

    fn draw_axes(
        &self,
        ctx: &mut dyn DrawingContext,
        x_map: Mapping,
        y_map: Mapping,
    ) -> AureaResult<()> {
        let area = self.plot_area;
        let bottom = area.y + area.height;

        if let Some(line) = self.style.x_axis.line {
            hline(ctx, bottom, area.x, area.x + area.width, line)?;
        }
        if let Some(line) = self.style.y_axis.line {
            vline(ctx, area.x, area.y, bottom, line)?;
        }

        for tick in self.x.ticks.ticks(self.x.resolved, self.x.scale) {
            let Placed::At(px) = x_map.place(tick.value) else {
                continue;
            };
            let style = &self.style.x_axis;
            let length = tick_length(style, tick.major);
            vline(ctx, px, bottom, bottom + length, style.tick)?;

            if tick.major && style.show_labels && !tick.label.is_empty() {
                self.draw_x_label(ctx, &tick.label, px, bottom + length + style.label_gap)?;
            }
        }

        for tick in self.y.ticks.ticks(self.y.resolved, self.y.scale) {
            let Placed::At(py) = y_map.place(tick.value) else {
                continue;
            };
            let style = &self.style.y_axis;
            let length = tick_length(style, tick.major);
            hline(ctx, py, area.x - length, area.x, style.tick)?;

            if tick.major && style.show_labels && !tick.label.is_empty() {
                self.draw_y_label(ctx, &tick.label, area.x - length - style.label_gap, py)?;
            }
        }
        Ok(())
    }

    /// X labels are centred under their tick.
    fn draw_x_label(
        &self,
        ctx: &mut dyn DrawingContext,
        label: &str,
        px: f32,
        top: f32,
    ) -> AureaResult<()> {
        let style = &self.style.x_axis;
        let metrics = ctx.measure_text(label, &style.label_font)?;
        let paint = fill(style.label_color);
        let origin = Point::new(px - metrics.width / 2.0, top + metrics.ascent);
        ctx.draw_text_with_font(label, origin, &style.label_font, &paint)
    }

    /// Y labels sit to the left of their tick, right-aligned against it.
    fn draw_y_label(
        &self,
        ctx: &mut dyn DrawingContext,
        label: &str,
        right: f32,
        py: f32,
    ) -> AureaResult<()> {
        let style = &self.style.y_axis;
        let metrics = ctx.measure_text(label, &style.label_font)?;
        let paint = fill(style.label_color);
        let origin = Point::new(right - metrics.width, py + metrics.ascent / 2.0);
        ctx.draw_text_with_font(label, origin, &style.label_font, &paint)
    }
}

/// The plotting area, once the margins are taken off.
fn inner_area(area: Rect, style: &GraphStyle) -> Rect {
    let m = style.margin;
    Rect::new(
        area.x + m.left,
        area.y + m.top,
        (area.width - m.left - m.right).max(0.0),
        (area.height - m.top - m.bottom).max(0.0),
    )
}

fn tick_length(style: &AxisStyle, major: bool) -> f32 {
    if major {
        style.tick_length
    } else {
        style.minor_tick_length
    }
}

/// Bar width from the gap between neighbours, leaving a little space.
fn bar_width(screen: &[Point]) -> f32 {
    let mut smallest = f32::INFINITY;
    for pair in screen.windows(2) {
        let gap = (pair[1].x - pair[0].x).abs();
        if gap > 0.0 {
            smallest = smallest.min(gap);
        }
    }
    if smallest.is_finite() {
        (smallest * 0.8).max(1.0)
    } else {
        // A single bar has no neighbour to measure against.
        6.0
    }
}

fn scale_alpha(alpha: u8, factor: f32) -> u8 {
    let scaled = f32::from(alpha) * factor.clamp(0.0, 1.0);
    narrow_to_u8(scaled)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn narrow_to_u8(v: f32) -> u8 {
    v.clamp(0.0, 255.0).round() as u8
}

fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x
        && point.x <= rect.x + rect.width
        && point.y >= rect.y
        && point.y <= rect.y + rect.height
}

fn fill(color: Color) -> Paint {
    Paint::new().color(color).style(PaintStyle::Fill)
}

fn stroke(line: Stroke) -> Paint {
    Paint::new()
        .color(line.color)
        .style(PaintStyle::Stroke)
        .stroke_width(line.width)
}

fn hline(ctx: &mut dyn DrawingContext, y: f32, x0: f32, x1: f32, line: Stroke) -> AureaResult<()> {
    ctx.draw_line(x0, y, x1, y, &stroke(line))
}

fn vline(ctx: &mut dyn DrawingContext, x: f32, y0: f32, y1: f32, line: Stroke) -> AureaResult<()> {
    ctx.draw_line(x, y0, x, y1, &stroke(line))
}

/// Joins the points up, either straight or with a hold-then-jump.
fn stroke_polyline(
    ctx: &mut dyn DrawingContext,
    screen: &[Point],
    color: Color,
    width: f32,
    stepped: bool,
) -> AureaResult<()> {
    if screen.len() < 2 {
        return Ok(());
    }
    let mut path = Path::new();
    path.commands.push(PathCommand::MoveTo(screen[0]));
    for pair in screen.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if stepped {
            path.commands
                .push(PathCommand::LineTo(Point::new(to.x, from.y)));
        }
        path.commands.push(PathCommand::LineTo(to));
    }
    let paint = Paint::new()
        .color(color)
        .style(PaintStyle::Stroke)
        .stroke_width(width);
    ctx.draw_path(&path, &paint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_auto_axis_grows_to_the_data() {
        let mut axis = Axis::auto();
        axis.resolve(Some(Range::new(0.0, 10.0)));

        assert!(axis.range().min < 0.0, "padded below");
        assert!(axis.range().max > 10.0, "padded above");
    }

    #[test]
    fn an_auto_axis_with_no_data_keeps_what_it_had() {
        let mut axis = Axis::fixed(2.0, 8.0);
        axis.bounds = Bounds::Auto { padding: 0.0 };
        axis.resolve(None);

        assert_eq!(axis.range(), Range::new(2.0, 8.0));
    }

    #[test]
    fn a_fixed_axis_ignores_the_data() {
        let mut axis = Axis::fixed(0.0, 1.0);
        axis.resolve(Some(Range::new(-100.0, 100.0)));

        assert_eq!(axis.range(), Range::new(0.0, 1.0));
    }

    #[test]
    fn a_window_axis_follows_the_newest_value() {
        let mut axis = Axis::window(10.0);
        axis.resolve(Some(Range::new(0.0, 55.0)));

        assert_eq!(axis.range(), Range::new(45.0, 55.0));
    }

    #[test]
    fn a_flat_auto_axis_is_widened_so_a_constant_signal_shows() {
        let mut axis = Axis::auto();
        axis.resolve(Some(Range::new(5.0, 5.0)));

        assert!(!axis.range().is_degenerate());
        assert!(axis.range().contains(5.0));
    }

    #[test]
    fn a_log_axis_is_lifted_off_zero() {
        let mut axis = Axis::auto().with_scale(Scale::Log10);
        axis.resolve(Some(Range::new(0.0, 100.0)));

        assert!(axis.range().min > 0.0, "got {:?}", axis.range());
    }

    #[test]
    fn the_plot_area_is_the_frame_less_the_margins() {
        let style = GraphStyle::default();
        let area = inner_area(Rect::new(0.0, 0.0, 200.0, 100.0), &style);

        assert_eq!(area.x, style.margin.left);
        assert_eq!(area.width, 200.0 - style.margin.left - style.margin.right);
    }

    #[test]
    fn a_frame_smaller_than_its_margins_has_no_plot_area() {
        let style = GraphStyle::default();
        let area = inner_area(Rect::new(0.0, 0.0, 10.0, 10.0), &style);

        assert_eq!(area.width, 0.0);
        assert_eq!(area.height, 0.0);
    }

    #[test]
    fn data_extent_covers_every_visible_series() {
        let mut graph = Graph::new();
        graph.add_series(Series::xy("a", vec![(0.0, 0.0), (5.0, 5.0)]));
        graph.add_series(Series::xy("b", vec![(-2.0, 9.0)]));

        let (x, y) = graph.data_extent();
        assert_eq!(x, Some(Range::new(-2.0, 5.0)));
        assert_eq!(y, Some(Range::new(0.0, 9.0)));
    }

    #[test]
    fn a_hidden_series_stays_out_of_the_extent() {
        let mut graph = Graph::new();
        graph.add_series(Series::xy("a", vec![(0.0, 0.0), (5.0, 5.0)]));
        graph.add_series(Series::xy("b", vec![(100.0, 100.0)]).with_visible(false));

        let (x, _) = graph.data_extent();
        assert_eq!(x, Some(Range::new(0.0, 5.0)));
    }

    #[test]
    fn a_hidden_series_keeps_its_palette_slot() {
        let mut graph = Graph::new();
        graph.add_series(Series::rolling("a", 4).with_visible(false));
        graph.add_series(Series::rolling("b", 4));

        // The second series is index 1 whether or not the first is drawn.
        assert_eq!(
            graph.style.palette_color(1),
            GraphStyle::default().palette_color(1)
        );
    }

    #[test]
    fn bar_width_comes_from_the_closest_pair() {
        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(14.0, 0.0),
        ];
        assert!((bar_width(&points) - 3.2).abs() < 1e-4);
    }

    #[test]
    fn a_single_bar_still_gets_a_width() {
        assert!(bar_width(&[Point::new(0.0, 0.0)]) > 0.0);
    }

    #[test]
    fn value_at_is_none_before_the_first_draw() {
        let graph = Graph::new();
        assert_eq!(graph.value_at(Point::new(10.0, 10.0)), None);
    }

    #[test]
    fn series_can_be_found_by_name() {
        let mut graph = Graph::new();
        graph.add_series(Series::rolling("temperature", 4));

        assert!(graph.series_by_name("temperature").is_some());
        assert!(graph.series_by_name("missing").is_none());
    }

    #[test]
    fn pushing_to_a_missing_series_is_ignored() {
        let mut graph = Graph::new();
        graph.push(7, 1.0);
        assert!(graph.series.is_empty());
    }
}
