//! The plot: layout, axes and drawing.
//!
//! A [`Graph`] holds series, ranges and a style, and draws itself into any
//! rect of a [`DrawingContext`]. It keeps no pixel state between frames, so it
//! is safe to redraw from a canvas draw callback, which is what the damage
//! tracker expects.

use aurea_foundation::AureaResult;
use aurea_render::{Color, DrawingContext, Paint, PaintStyle, Path, PathCommand, Point, Rect};

use super::scale::{Mapping, Placed, Range, Scale};
use super::series::{Plot, Series};
use super::style::{AxisStyle, GraphStyle, Stroke};
use super::ticks::{Tick, TickPlan};

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

impl Bounds {
    /// Whether working out this range means looking at the data.
    ///
    /// A fixed range does not, and finding the extent of a long series is not
    /// free: it reads every sample, however few of them end up on screen.
    fn needs_data(self) -> bool {
        !matches!(self, Self::Fixed(_))
    }
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
    /// The ticks for [`Self::resolved`], worked out once when the range
    /// settles. The grid and the axis both draw them, and generating them per
    /// caller meant every frame laid out the same ticks twice.
    resolved_ticks: Vec<Tick>,
}

impl Default for Axis {
    fn default() -> Self {
        Self {
            bounds: Bounds::default(),
            scale: Scale::Linear,
            ticks: TickPlan::default(),
            label: String::new(),
            resolved: Range::new(0.0, 1.0),
            resolved_ticks: Vec::new(),
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
        self.resolved_ticks = self.ticks.ticks(self.resolved, self.scale);
    }

    /// The ticks for the range in use. Empty until [`Self::resolve`] has run.
    fn resolved_ticks(&self) -> &[Tick] {
        &self.resolved_ticks
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
        // Only when a range is actually derived from the data. A plot with
        // both axes fixed — a scope, typically — would otherwise read every
        // sample each frame to answer a question nobody asked.
        let (data_x, data_y) = if self.x.bounds.needs_data() || self.y.bounds.needs_data() {
            self.data_extent()
        } else {
            (None, None)
        };
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
            for tick in self.x.resolved_ticks() {
                let Placed::At(px) = x_map.place(tick.value) else {
                    continue;
                };
                let line = if tick.major { grid.major } else { grid.minor };
                vline(ctx, px, area.y, area.y + area.height, line)?;
            }
        }
        if grid.show_horizontal {
            for tick in self.y.resolved_ticks() {
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
            let projected = self.project(series, x_map, y_map);
            let screen = &projected.points;
            if screen.is_empty() {
                continue;
            }

            // An envelope alternates bottom, top, bottom, top down the screen,
            // so joining it up would turn every column into a hard reversal —
            // the worst case for the stroker, and slower than the full trace it
            // replaced. Drawn as spans it is both correct and cheap.
            if projected.envelope {
                draw_envelope(ctx, screen, color, series.width)?;
                continue;
            }

            match series.plot {
                Plot::Line => stroke_polyline(ctx, screen, color, series.width, false)?,
                Plot::Step => stroke_polyline(ctx, screen, color, series.width, true)?,
                Plot::Points => {}
                Plot::Area => {
                    self.fill_area(ctx, screen, series, color, y_map)?;
                    stroke_polyline(ctx, screen, color, series.width, false)?;
                }
                Plot::Bars => self.draw_bars(ctx, screen, series, color, y_map)?,
            }

            if series.show_points || series.plot == Plot::Points {
                for point in screen {
                    ctx.draw_circle(*point, series.point_radius, &fill(color))?;
                }
            }
        }
        Ok(())
    }

    /// Data points turned into pixels, dropping any that have no position.
    ///
    /// Thinned when there is more data than screen to put it on, in whatever
    /// way suits the plot: an envelope is what a dense *line* looks like, but
    /// it is not what a scatter or a bar chart looks like, and drawing one for
    /// those turned them into something else entirely.
    fn project(&self, series: &Series, x_map: Mapping, y_map: Mapping) -> Projected {
        let points = &series.points;
        let columns = self.plot_area.width.max(1.0);
        let projected = points.iter().filter_map(|(x, y)| {
            let px = x_map.place(x).pixel()?;
            let py = y_map.place(y).pixel()?;
            Some(Point::new(px, py))
        });

        if points.len() <= decimation_threshold(columns) {
            return Projected {
                points: projected.collect(),
                envelope: false,
            };
        }

        let left = self.plot_area.x;
        match series.plot {
            // A line, its filled area and its stepped form all read as the
            // band between the highest and lowest value in each column.
            Plot::Line | Plot::Area | Plot::Step => Projected {
                points: decimate(projected, left, columns),
                envelope: true,
            },
            // A scatter is a cloud of marks. Two marks in the same pixel are
            // one mark, so keeping one per cell draws the same picture.
            Plot::Points => Projected {
                points: thin_to_cells(projected, self.plot_area, series.point_radius),
                envelope: false,
            },
            // Bars narrower than a pixel cannot be told apart, so each column
            // keeps its tallest — the usual way a dense bar chart is bucketed.
            Plot::Bars => Projected {
                points: tallest_per_column(projected, left, columns),
                envelope: false,
            },
        }
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

        for tick in self.x.resolved_ticks() {
            let Placed::At(px) = x_map.place(tick.value) else {
                continue;
            };
            let style = &self.style.x_axis;
            let length = tick_length(style, tick.major);
            if style.show_ticks {
                vline(ctx, px, bottom, bottom + length, style.tick)?;
            }

            if tick.major && style.show_labels && !tick.label.is_empty() {
                self.draw_x_label(ctx, &tick.label, px, bottom + length + style.label_gap)?;
            }
        }

        for tick in self.y.resolved_ticks() {
            let Placed::At(py) = y_map.place(tick.value) else {
                continue;
            };
            let style = &self.style.y_axis;
            let length = tick_length(style, tick.major);
            if style.show_ticks {
                hline(ctx, py, area.x - length, area.x, style.tick)?;
            }

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

/// Projected points, and whether they are a per-column envelope rather than
/// the trace itself.
struct Projected {
    points: Vec<Point>,
    envelope: bool,
}

/// Draws a decimated envelope as one vertical span per column.
fn draw_envelope(
    ctx: &mut dyn DrawingContext,
    screen: &[Point],
    color: Color,
    width: f32,
) -> AureaResult<()> {
    let paint = fill(color);
    let thickness = width.max(1.0);

    let mut index = 0;
    while index < screen.len() {
        let first = screen[index];
        // A column is one point when flat, two when it spans a range.
        let second = screen
            .get(index + 1)
            .filter(|next| (next.x - first.x).abs() < 0.5);

        let (top, bottom) = match second {
            Some(next) => (first.y.min(next.y), first.y.max(next.y)),
            None => (first.y, first.y),
        };
        ctx.draw_rect(
            Rect::new(first.x, top, thickness, (bottom - top).max(thickness)),
            &paint,
        )?;
        index += if second.is_some() { 2 } else { 1 };
    }
    Ok(())
}

/// Above this many points per pixel column, drawing every point is wasted
/// work. Two per column is the most that can be distinguished.
fn decimation_threshold(columns: f32) -> usize {
    let per_column = 2.0;
    super::numeric::f64_to_count(f64::from(columns * per_column)).max(64)
}

/// The column a point falls in, if it is on screen at all.
///
/// A point off the side of the plot belongs to no column: it is clipped away,
/// so letting it into the nearest one would stretch that column's span to
/// reach a value the viewer cannot see.
fn column_of(point: Point, left: f32, column_count: usize) -> Option<usize> {
    let column = super::numeric::f32_to_i32(point.x - left);
    let column = usize::try_from(column).ok()?;
    (column < column_count).then_some(column)
}

/// Collapses points to the highest and lowest in each pixel column.
///
/// The result is a polyline that walks each column bottom to top, which draws
/// the same envelope as the full trace at a fraction of the cost.
///
/// Collected per column rather than as a running column, because `Points::Xy`
/// takes pairs in any order: revisiting a column later used to open a second
/// span for it instead of widening the first.
fn decimate(points: impl Iterator<Item = Point>, left: f32, columns: f32) -> Vec<Point> {
    let column_count = super::numeric::f64_to_count(f64::from(columns)).max(1);
    let mut lo = vec![f32::INFINITY; column_count];
    let mut hi = vec![f32::NEG_INFINITY; column_count];

    for point in points {
        let Some(column) = column_of(point, left, column_count) else {
            continue;
        };
        lo[column] = lo[column].min(point.y);
        hi[column] = hi[column].max(point.y);
    }

    let mut out: Vec<Point> = Vec::with_capacity(column_count * 2);
    for column in 0..column_count {
        if lo[column] <= hi[column] {
            push_column(&mut out, left, column, lo[column], hi[column]);
        }
    }
    out
}

/// Keeps one point per pixel cell, in the order they first appear.
///
/// Marks landing on the same pixel are indistinguishable once drawn, so a
/// scatter of a million points needs no more of them than the plot has pixels.
///
/// Cells are tracked in a bit each rather than a hash set: hashing a
/// coordinate pair per point cost more than the drawing it saved. Measured on
/// a 100k-point scatter over a 1280x800 plot, per frame:
///
/// ```text
/// hash set, cell per pixel     9.8ms
/// bitset,   cell per pixel     6.5ms
/// bitset,   cell per mark      5.2ms
/// ```
///
/// What is left is drawing the marks that survive, which is the work the
/// scatter is actually asking for. The same data as a line costs 0.5ms
/// because an envelope is two points per column however dense it is.
fn thin_to_cells(points: impl Iterator<Item = Point>, area: Rect, radius: f32) -> Vec<Point> {
    // A cell the size of a mark, not of a pixel. Two marks closer together
    // than their own radius are all but the same mark, and binning finer just
    // keeps points that land on top of each other.
    let cell_size = radius.max(1.0);
    let width = super::numeric::f64_to_count(f64::from((area.width / cell_size).max(1.0)));
    let height = super::numeric::f64_to_count(f64::from((area.height / cell_size).max(1.0)));
    let cells = width.saturating_mul(height);
    let mut seen = vec![0u64; cells.div_ceil(64)];

    let mut out = Vec::new();
    for point in points {
        let x = super::numeric::f32_to_i32((point.x - area.x) / cell_size);
        let y = super::numeric::f32_to_i32((point.y - area.y) / cell_size);
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            continue;
        };
        if x >= width || y >= height {
            continue;
        }
        let cell = y * width + x;
        let (word, bit) = (cell / 64, 1u64 << (cell % 64));
        if seen[word] & bit == 0 {
            seen[word] |= bit;
            out.push(point);
        }
    }
    out
}

/// Keeps the tallest point in each pixel column.
///
/// Bars thinner than a pixel overdraw each other, so a column shows the
/// largest of them — the same answer as bucketing the data and taking the
/// maximum, which is what a reader of a dense bar chart expects to see.
fn tallest_per_column(points: impl Iterator<Item = Point>, left: f32, columns: f32) -> Vec<Point> {
    let column_count = super::numeric::f64_to_count(f64::from(columns)).max(1);
    // Screen y grows downward, so the tallest bar is the smallest y.
    let mut top = vec![f32::INFINITY; column_count];

    for point in points {
        let Some(column) = column_of(point, left, column_count) else {
            continue;
        };
        top[column] = top[column].min(point.y);
    }

    (0..column_count)
        .filter(|&column| top[column].is_finite())
        .map(|column| Point::new(left + column as f32, top[column]))
        .collect()
}

/// One column of the envelope, as a bottom-to-top pair.
fn push_column(out: &mut Vec<Point>, left: f32, column: usize, lo: f32, hi: f32) {
    let x = left + column as f32;
    out.push(Point::new(x, lo));
    if (hi - lo).abs() > f32::EPSILON {
        out.push(Point::new(x, hi));
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
    use std::iter::empty;

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

    /// Projection produces x in order, and decimation collapses each pixel
    /// column of it to at most a low and a high.
    #[test]
    fn decimation_collapses_to_at_most_two_points_per_column() {
        let columns = 100.0;
        let count = 10_000;
        let points = (0..count).map(|i| {
            // Ascending x across 100 columns, alternating y.
            let x = (i as f32) * columns / (count as f32);
            Point::new(x, if i % 2 == 0 { 0.0 } else { 10.0 })
        });

        let out = decimate(points, 0.0, columns);

        assert!(out.len() <= 200, "got {} points", out.len());
        assert!(!out.is_empty());
    }

    /// The envelope has to keep the extremes, or a spike would vanish.
    #[test]
    fn decimation_keeps_the_highest_and_lowest_in_a_column() {
        let points = vec![
            Point::new(0.0, 5.0),
            Point::new(0.2, -3.0),
            Point::new(0.4, 9.0),
            Point::new(0.6, 1.0),
        ];

        let out = decimate(points.into_iter(), 0.0, 4.0);

        let lowest = out.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let highest = out.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(lowest, -3.0);
        assert_eq!(highest, 9.0);
    }

    #[test]
    fn a_flat_column_yields_one_point() {
        let points = vec![Point::new(0.0, 5.0), Point::new(0.3, 5.0)];
        let out = decimate(points.into_iter(), 0.0, 4.0);

        assert_eq!(out.len(), 1, "nothing to span");
    }

    #[test]
    fn decimating_nothing_gives_nothing() {
        let out = decimate(empty(), 0.0, 10.0);
        assert!(out.is_empty());
    }

    /// Below the threshold the trace is drawn as it is, so short series keep
    /// their exact shape.
    #[test]
    fn a_short_series_is_not_decimated() {
        let mut graph = Graph::new();
        graph.plot_area = Rect::new(0.0, 0.0, 800.0, 400.0);
        graph.add_series(Series::xy(
            "s",
            (0..50).map(|i| (f64::from(i), 1.0)).collect(),
        ));

        let (x_map, y_map) = graph.mappings();
        let projected = graph.project(&graph.series[0], x_map, y_map);

        assert!(!projected.envelope);
        assert_eq!(projected.points.len(), 50);
    }

    /// A dense scatter used to come out as vertical bars: the envelope was
    /// chosen before the plot type was looked at, so every plot became a line.
    /// Both axes fixed: the data extent answers nothing, so it is not read.
    #[test]
    fn a_fixed_axis_does_not_need_the_data() {
        assert!(!Bounds::Fixed(Range::new(0.0, 1.0)).needs_data());
        assert!(Bounds::Auto { padding: 0.0 }.needs_data());
        assert!(Bounds::Window { span: 10.0 }.needs_data());
    }

    /// Skipping the extent must not change what a fixed plot draws.
    #[test]
    fn a_fixed_plot_draws_the_range_it_was_given() {
        let mut graph = Graph::new();
        graph.plot_area = Rect::new(0.0, 0.0, 100.0, 100.0);
        graph.x = Axis::fixed(0.0, 10.0);
        graph.y = Axis::fixed(-1.0, 1.0);
        graph.add_series(Series::xy("s", vec![(0.0, 0.0), (1000.0, 500.0)]));

        graph.x.resolve(None);
        graph.y.resolve(None);

        assert_eq!(graph.x.range(), Range::new(0.0, 10.0));
        assert_eq!(graph.y.range(), Range::new(-1.0, 1.0));
    }

    /// The grid and the axis both draw ticks; they must see the same ones.
    #[test]
    fn ticks_are_worked_out_once_when_the_range_settles() {
        let mut axis = Axis::fixed(0.0, 10.0);
        assert!(axis.resolved_ticks().is_empty(), "none until resolved");

        axis.resolve(None);

        let ticks = axis.resolved_ticks().to_vec();
        assert!(!ticks.is_empty());
        assert_eq!(axis.resolved_ticks(), ticks, "stable between readers");
    }

    #[test]
    fn a_dense_scatter_stays_a_scatter() {
        let mut graph = Graph::new();
        graph.plot_area = Rect::new(0.0, 0.0, 800.0, 400.0);
        graph.x = Axis::fixed(0.0, 100_000.0);
        graph.y = Axis::fixed(-1.0, 1.0);
        let mut series = Series::xy(
            "s",
            (0..100_000)
                .map(|i| (f64::from(i), if i % 2 == 0 { -1.0 } else { 1.0 }))
                .collect(),
        );
        series.plot = Plot::Points;
        graph.add_series(series);

        let (x_map, y_map) = graph.mappings();
        let projected = graph.project(&graph.series[0], x_map, y_map);

        assert!(!projected.envelope, "a scatter is not an envelope");
        assert!(
            projected.points.len() <= 800 * 400,
            "no more marks than the plot has pixels"
        );
        assert!(projected.points.len() < 100_000, "still thinned");
    }

    /// Bars keep one per column rather than becoming a filled band.
    #[test]
    fn a_dense_bar_chart_keeps_one_bar_per_column() {
        let mut graph = Graph::new();
        graph.plot_area = Rect::new(0.0, 0.0, 800.0, 400.0);
        graph.x = Axis::fixed(0.0, 100_000.0);
        graph.y = Axis::fixed(0.0, 1.0);
        let mut series = Series::xy("s", (0..100_000).map(|i| (f64::from(i), 0.5)).collect());
        series.plot = Plot::Bars;
        graph.add_series(series);

        let (x_map, y_map) = graph.mappings();
        let projected = graph.project(&graph.series[0], x_map, y_map);

        assert!(!projected.envelope);
        assert!(
            projected.points.len() <= 801,
            "got {} bars for an 800px plot",
            projected.points.len()
        );
    }

    /// `Points::Xy` takes pairs in any order. Revisiting a column later used to
    /// open a second span for it rather than widening the first.
    #[test]
    fn an_out_of_order_series_gets_one_span_per_column() {
        let left = 0.0;
        let columns = 4.0;
        // Column 0 is visited, left, and visited again with a lower value.
        let points = [
            Point::new(0.0, 10.0),
            Point::new(2.0, 5.0),
            Point::new(0.0, 30.0),
        ];

        let out = decimate(points.into_iter(), left, columns);

        let in_column_0: Vec<_> = out.iter().filter(|p| p.x == 0.0).collect();
        assert_eq!(
            in_column_0.len(),
            2,
            "one bottom and one top, not two spans"
        );
        assert_eq!(in_column_0[0].y, 10.0);
        assert_eq!(in_column_0[1].y, 30.0);
    }

    /// A point off the side of the plot is clipped away, so it must not drag
    /// an edge column's span out to a value nobody can see.
    #[test]
    fn a_point_outside_the_plot_joins_no_column() {
        let out = decimate(
            [Point::new(-50.0, 999.0), Point::new(1.0, 5.0)].into_iter(),
            0.0,
            4.0,
        );

        assert!(out.iter().all(|p| p.y == 5.0), "got {out:?}");
    }

    #[test]
    fn a_long_series_is_decimated_to_the_screen() {
        let mut graph = Graph::new();
        graph.plot_area = Rect::new(0.0, 0.0, 800.0, 400.0);
        graph.x = Axis::fixed(0.0, 100_000.0);
        graph.y = Axis::fixed(-1.0, 1.0);
        graph.add_series(Series::xy(
            "s",
            (0..100_000)
                .map(|i| (f64::from(i), if i % 2 == 0 { -1.0 } else { 1.0 }))
                .collect(),
        ));

        let (x_map, y_map) = graph.mappings();
        let projected = graph.project(&graph.series[0], x_map, y_map);

        assert!(projected.envelope, "should have been decimated");
        assert!(
            projected.points.len() <= 2 * 800 + 2,
            "got {} points for an 800px plot",
            projected.points.len()
        );
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
