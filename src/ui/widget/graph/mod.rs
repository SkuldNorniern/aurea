//! Plots and scopes.

mod axis;
mod buffer;
mod numeric;
mod plot;
pub mod prelude;
mod project;
pub mod quick;
mod scale;
mod scope;
mod scope_view;
mod series;
mod ticks;

pub use axis::{Axis, Bounds, Cursor};
pub use buffer::SampleBuffer;
pub use plot::Graph;
pub use project::Projection;
pub(crate) use project::bar_width;
pub use scale::{Mapping, Placed, Range, Scale};
pub use scope::{Channel, Scope, Timebase, Trigger, TriggerEdge, TriggerMode};
pub use scope_view::{ScopeStyle, ScopeView};
pub use series::{Plot, PointIter, Points, Series};
pub(crate) use ticks::engineering;
pub use ticks::{Tick, TickPlan};

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::FocusRing;
use crate::ui::frame::{Align, Response, Ui};
use crate::ui::paint::{
    AxisMarks, AxisSide, AxisStyle, Component, GridLines, GridStyle, Marker, Stroke, Trace,
};
use crate::ui::state::{Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::CursorIcon;

/// Space around the plotting area, for the axes and their labels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Margin {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Margin {
    pub const fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }

    /// The same margin on every side.
    pub const fn uniform(size: f32) -> Self {
        Self::new(size, size, size, size)
    }

    /// `rect` with the margin taken off, never less than nothing.
    pub fn inside(&self, rect: Rect) -> Rect {
        Rect::new(
            rect.x + self.left,
            rect.y + self.top,
            (rect.width - self.left - self.right).max(0.0),
            (rect.height - self.top - self.bottom).max(0.0),
        )
    }
}

/// How a graph looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphStyle {
    /// Behind everything, margins included.
    pub background: Option<Color>,
    /// Behind the plotting area only.
    pub plot_background: Option<Color>,
    pub border: Option<Stroke>,
    pub grid: GridStyle,
    pub x_axis: AxisStyle,
    pub y_axis: AxisStyle,
    pub margin: Margin,
    pub radius: f32,
    /// Colours handed out, in order, to series that do not pick one.
    pub palette: [Color; 6],
    /// Cursors that do not pick a colour.
    pub marker: Color,
    /// The lines through the pointer while it is over the plot.
    pub crosshair: Option<Stroke>,
    /// The value under the pointer, written in the plot's top right.
    pub readout: Option<Color>,
}

impl GraphStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        let axis = AxisStyle {
            line: Some(Stroke::new(p.border, 1.0)),
            tick: Some(Stroke::new(p.border, 1.0)),
            tick_length: 4.0,
            minor_tick_length: 2.0,
            label: Some(p.dim),
            label_gap: 4.0,
        };
        Self {
            background: Some(p.surface),
            plot_background: Some(p.background),
            border: Some(Stroke::new(p.border, 1.0)),
            grid: GridStyle {
                major: Stroke::new(p.text.fade(0.12), 1.0),
                minor: Stroke::new(p.text.fade(0.06), 1.0),
                zero: Some(Stroke::new(p.text.fade(0.3), 1.0)),
                vertical: true,
                horizontal: true,
            },
            x_axis: axis,
            y_axis: axis,
            margin: Margin::new(52.0, 12.0, 12.0, 28.0),
            radius: theme.radius,
            palette: [
                p.accent,
                p.warning,
                p.success,
                p.danger,
                Color::rgb(190, 160, 255),
                Color::rgb(120, 230, 225),
            ],
            marker: p.warning,
            crosshair: Some(Stroke::new(p.dim.fade(0.5), 1.0)),
            readout: Some(p.text),
        }
    }

    /// Nothing but the trace: no grid, axes, border or background.
    pub fn bare(theme: &Theme) -> Self {
        let none = AxisStyle {
            line: None,
            tick: None,
            label: None,
            ..Self::new(theme).x_axis
        };
        Self {
            background: None,
            plot_background: None,
            border: None,
            grid: GridStyle {
                zero: None,
                vertical: false,
                horizontal: false,
                ..Self::new(theme).grid
            },
            x_axis: none,
            y_axis: none,
            margin: Margin::uniform(1.0),
            crosshair: None,
            readout: None,
            ..Self::new(theme)
        }
    }

    /// The colour series `index` gets when it does not choose one.
    pub fn palette_color(&self, index: usize) -> Color {
        self.palette[index % self.palette.len()]
    }
}

/// Shows a [`Graph`].
///
/// ```rust,no_run
/// # use aurea::ui::{GraphView, Ui};
/// # use aurea::ui::graph::quick;
/// # fn f(ui: &mut Ui<'_, u8>) -> aurea::AureaResult<()> {
/// let (mut load, cpu) = quick::live("cpu", 600);
/// load.push(cpu, 0.42);
/// let at = aurea::Rect::new(0.0, 0.0, 400.0, 200.0);
/// GraphView::new(&mut load).show(ui, 1, at)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct GraphView<'g> {
    graph: &'g mut Graph,
    style: Option<GraphStyle>,
    bare: bool,
    enabled: bool,
}

impl<'g> GraphView<'g> {
    pub fn new(graph: &'g mut Graph) -> Self {
        Self {
            graph,
            style: None,
            bare: false,
            enabled: true,
        }
    }

    pub fn style(mut self, style: GraphStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// [`GraphStyle::bare`], and no input: a sparkline.
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self.enabled = false;
        self
    }

    /// Whether it takes input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }

    fn style_for(&self, theme: &Theme) -> GraphStyle {
        self.style.unwrap_or_else(|| {
            if self.bare {
                GraphStyle::bare(theme)
            } else {
                GraphStyle::new(theme)
            }
        })
    }
}

impl<Id: WidgetId> Widget<Id> for GraphView<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style_for(ui.theme);
        let graph = self.graph;
        let area = style.margin.inside(rect);
        graph.resolve(area);
        let input = ui.interact(id, 0, area, Sense::Adjust, self.enabled);
        let mut response = Response::new(rect, input);
        if self.enabled && graph.view.follow(ui, id, input, graph.mappings()) {
            response.changed = true;
            ui.mark_changed();
            graph.resolve(area);
        }
        if input.hovered && self.enabled {
            ui.set_cursor(CursorIcon::Crosshair);
        }
        let state = ui.eased(id, 0, input);

        if let Some(color) = style.background {
            ui.round(rect, style.radius, color)?;
        }
        if area.width <= 0.0 || area.height <= 0.0 {
            return Ok(response);
        }
        if let Some(color) = style.plot_background {
            ui.fill(area, color)?;
        }
        let maps = graph.mappings();
        let ticks = (graph.x.resolved_ticks(), graph.y.resolved_ticks());
        GridLines {
            x: ticks.0,
            y: ticks.1,
            maps,
            style: style.grid,
        }
        .paint(ui, area)?;
        ui.clipped(area, |ui| traces(ui, graph, &style, area))?;
        cursors(ui, graph, &style, area)?;
        if input.hovered {
            crosshair(ui, graph, &style, area)?;
        }
        axes(ui, graph, &style, area)?;
        if let Some(border) = style.border {
            ui.border(area, 0.0, border.width, border.color)?;
        }
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
        Ok(response)
    }
}

fn traces<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    graph: &mut Graph,
    style: &GraphStyle,
    area: Rect,
) -> AureaResult<()> {
    let maps = graph.mappings();
    let Graph {
        series, projection, ..
    } = graph;
    for (index, series) in series.iter().enumerate() {
        if !series.visible || series.points.is_empty() {
            continue;
        }
        projection.project(
            series.points.iter(),
            series.points.len(),
            (series.plot, series.point_radius),
            maps,
            area,
        );
        Trace {
            points: projection.points(),
            envelope: projection.envelope(),
            plot: series.plot,
            color: series.color.unwrap_or_else(|| style.palette_color(index)),
            width: series.width,
            marks: series.show_points.then_some(series.point_radius),
            base: maps
                .1
                .place(series.baseline)
                .pixel()
                .unwrap_or(area.bottom()),
            fill_alpha: series.fill_alpha,
        }
        .paint(ui, area)?;
    }
    Ok(())
}

fn cursors<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    graph: &Graph,
    style: &GraphStyle,
    area: Rect,
) -> AureaResult<()> {
    let (x_map, y_map) = graph.mappings();
    for cursor in &graph.cursors {
        let map = if cursor.vertical { x_map } else { y_map };
        if let Placed::At(at) = map.place(cursor.value) {
            Marker {
                at,
                vertical: cursor.vertical,
                stroke: Stroke::new(cursor.color.unwrap_or(style.marker), 1.0),
                label: &cursor.label,
            }
            .paint(ui, area)?;
        }
    }
    Ok(())
}

fn crosshair<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    graph: &Graph,
    style: &GraphStyle,
    area: Rect,
) -> AureaResult<()> {
    let (Some(at), Some((x, y))) = (ui.pointer(), graph.view.hovered()) else {
        return Ok(());
    };
    if let Some(stroke) = style.crosshair {
        for (at, vertical) in [(at.x, true), (at.y, false)] {
            Marker {
                at,
                vertical,
                stroke,
                label: "",
            }
            .paint(ui, area)?;
        }
    }
    let Some(color) = style.readout else {
        return Ok(());
    };
    let text = format!(
        "{}, {}",
        readout(x, graph.x.range()),
        readout(y, graph.y.range())
    );
    let fonts = ui.fonts();
    let line = Rect::new(
        area.x,
        area.y + 2.0,
        area.width - 6.0,
        fonts.small.size * 1.4,
    );
    ui.text(&text, line, &fonts.small, color, Align::End)
}

fn axes<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    graph: &Graph,
    style: &GraphStyle,
    area: Rect,
) -> AureaResult<()> {
    let (x_map, y_map) = graph.mappings();
    for (ticks, map, side, axis) in [
        (
            graph.x.resolved_ticks(),
            x_map,
            AxisSide::Bottom,
            style.x_axis,
        ),
        (
            graph.y.resolved_ticks(),
            y_map,
            AxisSide::Left,
            style.y_axis,
        ),
    ] {
        AxisMarks {
            ticks,
            map,
            side,
            style: axis,
        }
        .paint(ui, area)?;
    }
    Ok(())
}

fn readout(value: f64, range: Range) -> String {
    let span = range.span().abs();
    let decimals = if span > 0.0 && span.is_finite() {
        (3.0 - span.log10().floor()).clamp(0.0, 9.0)
    } else {
        2.0
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let decimals = decimals as usize;
    format!("{value:.decimals$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{Axis, Series, quick};
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::state::UiState;
    use crate::window::{Modifiers, MouseButton, MouseButtons, WindowEvent};
    use std::slice::from_raw_parts;

    const W: u32 = 240;
    const H: u32 = 160;
    const FRAME: Rect = Rect::new(0.0, 0.0, W as f32, H as f32);

    struct Rig {
        raster: CpuRasterizer,
        ui: UiState<u8>,
        theme: Theme,
    }

    impl Rig {
        fn new() -> Self {
            let mut ui = UiState::new();
            ui.set_reduced_motion(true);
            Self {
                raster: CpuRasterizer::new(W, H),
                ui,
                theme: Theme::dark(),
            }
        }

        fn frame(&mut self, show: impl FnOnce(&mut Ui<'_, u8>)) -> Vec<u32> {
            {
                let mut ctx = self.raster.begin_frame().expect("frame");
                let mut ui = self.ui.frame(&mut *ctx, &self.theme);
                show(&mut ui);
            }
            self.raster.end_frame().expect("end");
            let (ptr, len, _, _) = self.raster.get_buffer();
            let bytes = unsafe { from_raw_parts(ptr, len) };
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&c| u32::from_le_bytes(c))
                .collect()
        }

        fn show(&mut self, graph: &mut Graph, style: Option<GraphStyle>) -> Vec<u32> {
            self.frame(|ui| {
                let mut view = GraphView::new(graph);
                if let Some(style) = style {
                    view = view.style(style);
                }
                view.show(ui, 0, FRAME).expect("draw");
            })
        }

        fn send(&mut self, event: WindowEvent) {
            self.ui.handle(&event);
        }
    }

    fn ink(buf: &[u32], ground: u32) -> usize {
        buf.iter().filter(|&&p| p != 0 && p != ground).count()
    }

    fn px(buf: &[u32], x: u32, y: u32) -> u32 {
        buf[(y * W + x) as usize]
    }

    fn line(count: usize) -> Graph {
        let mut graph = Graph::new();
        graph.add_series(Series::xy(
            "s",
            (0..count)
                .map(|i| {
                    let t = i as f64 / count as f64;
                    (t * 100.0, (t * 12.0).sin())
                })
                .collect(),
        ));
        graph
    }

    fn ground(rig: &Rig) -> u32 {
        let c = rig.theme.palette.background;
        u32::from_be_bytes([255, c.r, c.g, c.b])
    }

    #[test]
    fn a_line_is_drawn_at_every_density() {
        let mut rig = Rig::new();
        let ground = ground(&rig);
        let mut blank = Vec::new();
        for count in [2usize, 50, 479, 480, 481, 1000, 100_000] {
            let buf = rig.show(&mut line(count), None);
            if ink(&buf, ground) < 200 {
                blank.push(count);
            }
        }
        assert!(blank.is_empty(), "traces with too little ink: {blank:?}");
    }

    #[test]
    fn a_windowed_line_draws_its_newest_data() {
        let mut rig = Rig::new();
        let ground = ground(&rig);
        for count in [50usize, 5000, 100_000] {
            let mut graph = line(count).with_x(Axis::window(50.0));
            let buf = rig.show(&mut graph, None);
            assert!(
                ink(&buf, ground) > 200,
                "{count} points left the plot blank"
            );
        }
    }

    #[test]
    fn a_bare_graph_leaves_the_frame_alone() {
        let mut rig = Rig::new();
        let mut graph = quick::sparkline(&[0.0, 1.0, 0.5]);
        let buf = rig.frame(|ui| {
            GraphView::new(&mut graph)
                .bare()
                .show(ui, 0, FRAME)
                .expect("draw");
        });
        assert_eq!(px(&buf, 0, 0), 0, "no background in a corner");
        assert!(ink(&buf, 0) > 0, "but the trace is there");
    }

    #[test]
    fn the_trace_is_clipped_to_the_plot_area() {
        let mut rig = Rig::new();
        let style = GraphStyle {
            margin: Margin::new(40.0, 10.0, 10.0, 20.0),
            ..GraphStyle::bare(&rig.theme)
        };
        let mut graph = Graph::new()
            .with_x(Axis::fixed(0.0, 1.0))
            .with_y(Axis::fixed(0.0, 1.0));
        graph.add_series(Series::xy("s", vec![(-50.0, -50.0), (50.0, 50.0)]));
        let buf = rig.show(&mut graph, Some(style));
        for y in 0..H {
            for x in 0..38 {
                assert_eq!(px(&buf, x, y), 0, "trace escaped at ({x}, {y})");
            }
        }
    }

    #[test]
    fn a_graph_smaller_than_its_margins_draws_its_background() {
        let mut rig = Rig::new();
        let mut graph = line(10);
        let buf = rig.frame(|ui| {
            GraphView::new(&mut graph)
                .show(ui, 0, Rect::new(0.0, 0.0, 20.0, 20.0))
                .expect("draw");
        });
        assert_ne!(px(&buf, 10, 10) >> 24, 0);
    }

    #[test]
    fn the_wheel_zooms_where_the_pointer_is_and_a_double_click_resets() {
        let mut rig = Rig::new();
        let mut graph = Graph::new()
            .with_x(Axis::fixed(0.0, 100.0))
            .with_y(Axis::fixed(0.0, 1.0));
        rig.show(&mut graph, None);
        let area = graph.plot_area();
        let (x, y) = (area.x + area.width / 4.0, area.y + area.height / 2.0);
        rig.send(WindowEvent::MouseMove {
            x: f64::from(x),
            y: f64::from(y),
            buttons: MouseButtons::default(),
            modifiers: Modifiers::default(),
        });
        rig.send(WindowEvent::MouseWheel {
            delta_x: 0.0,
            delta_y: 2.0,
            modifiers: Modifiers::default(),
        });
        rig.show(&mut graph, None);
        let zoomed = graph.x.range();
        assert!(zoomed.span() < 100.0, "zoomed in: {zoomed:?}");
        assert!(zoomed.contains(25.0), "around the pointer: {zoomed:?}");
        assert_eq!(graph.y.range(), Range::new(0.0, 1.0), "y left alone");
        for _ in 0..2 {
            rig.send(WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed: true,
                modifiers: Modifiers::default(),
                x: f64::from(x),
                y: f64::from(y),
                click_count: 2,
            });
        }
        rig.show(&mut graph, None);
        assert_eq!(graph.x.range(), Range::new(0.0, 100.0), "reset");
    }

    #[test]
    fn readouts_keep_the_digits_the_view_needs() {
        assert_eq!(readout(5.123_456, Range::new(0.0, 10.0)), "5.12");
        assert_eq!(readout(1234.4, Range::new(0.0, 10_000.0)), "1234");
        assert_eq!(readout(0.000_123_4, Range::new(0.0, 0.001)), "0.000123");
    }
}
