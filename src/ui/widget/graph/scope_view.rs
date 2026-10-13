//! An oscilloscope screen: a graticule in even divisions, a trace per
//! channel, the trigger level, and a front panel's readout along the top.

use std::mem;

use super::numeric::count_to_f64;
use super::{GraphStyle, Mapping, Margin, Plot, Scope, engineering};
use crate::AureaResult;
use crate::render::{Color, Font, Rect};
use crate::ui::frame::{Align, Response, Ui};
use crate::ui::paint::{AxisMarks, AxisSide, Component, GridLines, Marker, Stroke, Trace};
use crate::ui::state::{Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::ui::widget::Widget;

/// How a scope looks: a graph's furniture, plus its traces and readout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScopeStyle {
    pub graph: GraphStyle,
    pub trace_width: f32,
    /// How opaque the trigger level's line is, 0 to 1, so it does not
    /// compete with the trace.
    pub trigger_alpha: f32,
    /// The timebase and each channel's scale, in the top margin. `None` to
    /// leave it out.
    pub readout: Option<Color>,
}

impl ScopeStyle {
    pub fn new(theme: &Theme) -> Self {
        let mut graph = GraphStyle::new(theme);
        // A graticule's centre lines weigh the same as the rest of it.
        graph.grid.zero = Some(graph.grid.major);
        // Room along the top for the readout.
        graph.margin = Margin::new(56.0, 16.0, 24.0, 30.0);
        graph.crosshair = None;
        graph.readout = None;
        Self {
            graph,
            trace_width: 1.25,
            trigger_alpha: 0.6,
            readout: Some(theme.palette.dim),
        }
    }
}

/// Shows a [`Scope`]: the newest sweep, held still by the trigger.
///
/// ```rust,no_run
/// # use aurea::ui::{ScopeView, Ui};
/// # use aurea::ui::graph::quick;
/// # fn f(ui: &mut Ui<'_, u8>) -> aurea::AureaResult<()> {
/// let mut scope = quick::scope(2, 20_000.0);
/// scope.push(0, 0.5);
/// ScopeView::new(&mut scope).show(ui, 1, aurea::Rect::new(0.0, 0.0, 600.0, 300.0))?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ScopeView<'s> {
    scope: &'s mut Scope,
    style: Option<ScopeStyle>,
}

impl<'s> ScopeView<'s> {
    pub fn new(scope: &'s mut Scope) -> Self {
        Self { scope, style: None }
    }

    pub fn style(mut self, style: ScopeStyle) -> Self {
        self.style = Some(style);
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
}

impl<Id: WidgetId> Widget<Id> for ScopeView<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| ScopeStyle::new(ui.theme));
        let g = style.graph;
        let scope = self.scope;
        let area = g.margin.inside(rect);
        let input = ui.interact(id, 0, area, Sense::Hover, true);
        scope.settle();

        if let Some(color) = g.background {
            ui.round(rect, g.radius, color)?;
        }
        if area.width <= 0.0 || area.height <= 0.0 {
            return Ok(Response::new(rect, input));
        }
        if let Some(color) = g.plot_background {
            ui.fill(area, color)?;
        }
        let maps = scope.graticule(area);
        let (x, y) = scope.ticks();
        GridLines {
            x,
            y,
            maps,
            style: g.grid,
        }
        .paint(ui, area)?;
        ui.clipped(area, |ui| traces(ui, scope, &style, maps, area))?;
        trigger_level(ui, scope, &style, maps.1, area)?;
        let (x, y) = scope.ticks();
        for (ticks, map, side, axis) in [
            (x, maps.0, AxisSide::Bottom, g.x_axis),
            (y, maps.1, AxisSide::Left, g.y_axis),
        ] {
            AxisMarks {
                ticks,
                map,
                side,
                style: axis,
            }
            .paint(ui, area)?;
        }
        if let Some(border) = g.border {
            ui.border(area, 0.0, border.width, border.color)?;
        }
        readout(ui, scope, &style, area)?;
        Ok(Response::new(rect, input))
    }
}

fn traces<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    scope: &mut Scope,
    style: &ScopeStyle,
    maps: (Mapping, Mapping),
    area: Rect,
) -> AureaResult<()> {
    let interval = scope.timebase.sample_interval();
    // Taken out while the capture is read; the swap moves the buffers and
    // allocates nothing.
    let mut projection = mem::take(&mut scope.projection);
    let mut result = Ok(());
    for (index, channel) in scope.channels.iter().enumerate() {
        if !channel.visible {
            continue;
        }
        let (head, tail) = scope.sweep(index);
        // Offset moves the trace without touching the capture, the way a
        // front panel does, so it works on a frozen sweep too.
        let samples = head
            .iter()
            .chain(tail)
            .enumerate()
            .map(|(i, v)| (interval * count_to_f64(i), v + channel.offset));
        projection.project(
            samples,
            head.len() + tail.len(),
            (Plot::Line, 1.0),
            maps,
            area,
        );
        result = Trace {
            points: projection.points(),
            envelope: projection.envelope(),
            plot: Plot::Line,
            color: channel
                .color
                .unwrap_or_else(|| style.graph.palette_color(index)),
            width: style.trace_width,
            marks: None,
            base: area.bottom(),
            fill_alpha: 0.0,
        }
        .paint(ui, area);
        if result.is_err() {
            break;
        }
    }
    scope.projection = projection;
    result
}

/// A line at the trigger level, so it is clear what the sweep waits for and
/// which channel it watches.
fn trigger_level<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    scope: &Scope,
    style: &ScopeStyle,
    y_map: Mapping,
    area: Rect,
) -> AureaResult<()> {
    let trigger = scope.trigger;
    if !trigger.enabled {
        return Ok(());
    }
    let Some(at) = y_map.place(trigger.level).pixel() else {
        return Ok(());
    };
    let color = scope
        .channels
        .get(trigger.source)
        .and_then(|c| c.color)
        .unwrap_or_else(|| style.graph.palette_color(trigger.source));
    Marker {
        at,
        vertical: false,
        stroke: Stroke::new(color.fade(style.trigger_alpha), 1.0),
        label: "T",
    }
    .paint(ui, area)
}

/// The timebase, then each visible channel's volts per division in its own
/// colour, in the top margin. A margin too short for a line gets none.
fn readout<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    scope: &Scope,
    style: &ScopeStyle,
    area: Rect,
) -> AureaResult<()> {
    let top = style.graph.margin.top;
    let Some(color) = style.readout.filter(|_| top >= 12.0) else {
        return Ok(());
    };
    let fonts = ui.fonts();
    let font = &fonts.small;
    let mut line = Rect::new(area.x, area.y - top, area.width, top);
    let (per_div, prefix) = engineering(scope.timebase.seconds_per_division);
    segment(
        ui,
        &format!("{per_div:.0}{prefix}s/div"),
        &mut line,
        font,
        color,
    )?;
    for (index, channel) in scope.channels.iter().enumerate() {
        if !channel.visible {
            continue;
        }
        let (volts, prefix) = engineering(channel.volts_per_division);
        let text = format!("{}: {volts:.0}{prefix}V/div", channel.name);
        let tint = channel
            .color
            .unwrap_or_else(|| style.graph.palette_color(index));
        segment(ui, &text, &mut line, font, tint)?;
    }
    Ok(())
}

/// `text` at the start of `line`, which then moves past it.
fn segment<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    text: &str,
    line: &mut Rect,
    font: &Font,
    color: Color,
) -> AureaResult<()> {
    ui.text(text, *line, font, color, Align::Start)?;
    let advance = ui.text_width(text, font) + 12.0;
    line.x += advance;
    line.width = (line.width - advance).max(0.0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::graph::{Channel, Timebase, Trigger};
    use crate::ui::state::UiState;

    const W: u32 = 240;
    const H: u32 = 160;

    fn render(scope: &mut Scope) -> Vec<u32> {
        let mut raster = CpuRasterizer::new(W, H);
        let mut state = UiState::<u8>::new();
        let theme = Theme::dark();
        {
            let mut ctx = raster.begin_frame().expect("frame");
            let mut ui = state.frame(&mut *ctx, &theme);
            let rect = Rect::new(0.0, 0.0, W as f32, H as f32);
            ScopeView::new(scope).show(&mut ui, 0, rect).expect("draw");
        }
        raster.end_frame().expect("end");
        raster.pixels().to_vec()
    }

    #[test]
    fn a_scope_draws_its_channels() {
        let mut scope = Scope::new(512);
        scope.timebase = Timebase::new(0.001, 10_000.0);
        let ch = scope.add_input("CH1");
        for i in 0..512 {
            scope.push(ch, (f64::from(i) / 10.0).sin());
        }
        scope.trigger = Trigger::rising(0.0);
        let mut empty = Scope::new(512);
        empty.timebase = scope.timebase;
        empty.add_input("CH1");
        let traced = render(&mut scope);
        let bare = render(&mut empty);
        let differing = traced.iter().zip(&bare).filter(|(a, b)| a != b).count();
        assert!(
            differing > 200,
            "expected a trace, {differing} pixels differ"
        );
    }

    #[test]
    fn a_scope_with_no_samples_draws_the_graticule() {
        let mut scope = Scope::new(64);
        scope.add_channel(Channel::new("CH1"));
        let buf = render(&mut scope);
        assert_ne!(buf[(10 * W + 10) as usize] >> 24, 0, "background painted");
    }

    /// A long capture into a small screen projects into kept buffers, so a
    /// running scope stops allocating once warm.
    #[test]
    fn a_warm_scope_reuses_its_projection() {
        let mut scope = Scope::new(100_000);
        scope.timebase = Timebase::new(0.01, 100_000.0);
        let ch = scope.add_input("CH1");
        for i in 0..100_000 {
            scope.push(ch, f64::from(i % 7));
        }
        render(&mut scope);
        let kept = scope.projection.points().as_ptr();
        render(&mut scope);
        assert_eq!(scope.projection.points().as_ptr(), kept);
    }
}
