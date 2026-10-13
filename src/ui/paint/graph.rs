//! The pieces a plot is drawn from: the grid, an axis's ticks and labels, a trace, and a marker
//! line.

use crate::AureaResult;
use crate::render::{Color, Point, Rect};
use crate::ui::frame::{Ui, fill};
use crate::ui::graph::{Mapping, Placed, Plot, Tick, bar_width};
use crate::ui::state::WidgetId;

use super::Component;

/// How a line is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
}

impl Stroke {
    pub const fn new(color: Color, width: f32) -> Self {
        Self { color, width }
    }
}

/// The lines behind a trace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridStyle {
    /// Lines at labelled ticks.
    pub major: Stroke,
    /// Lines at unlabelled ticks.
    pub minor: Stroke,
    /// The line at zero, drawn over the grid.
    pub zero: Option<Stroke>,
    pub vertical: bool,
    pub horizontal: bool,
}

/// One axis: its line, ticks and labels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisStyle {
    /// The axis line itself.
    pub line: Option<Stroke>,
    /// Tick marks poking out of the axis.
    pub tick: Option<Stroke>,
    /// How far a major tick sticks out, in pixels.
    pub tick_length: f32,
    pub minor_tick_length: f32,
    /// The labels' colour, in the theme's small font.
    pub label: Option<Color>,
    /// Gap between the ticks and their labels.
    pub label_gap: f32,
}

/// The grid over a plotting area, at the axes' ticks.
#[derive(Clone, Copy, Debug)]
pub struct GridLines<'t> {
    pub x: &'t [Tick],
    pub y: &'t [Tick],
    pub maps: (Mapping, Mapping),
    pub style: GridStyle,
}

impl Component for GridLines<'_> {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, area: Rect) -> AureaResult<()> {
        let s = self.style;
        let (x_map, y_map) = self.maps;
        if s.vertical {
            self.lines(ui, self.x, x_map, true, area)?;
        }
        if s.horizontal {
            self.lines(ui, self.y, y_map, false, area)?;
        }
        let Some(zero) = s.zero else {
            return Ok(());
        };
        if let Placed::At(py) = y_map.place(0.0) {
            hline(ui, py, area.x, area.right(), zero)?;
        }
        if let Placed::At(px) = x_map.place(0.0) {
            vline(ui, px, area.y, area.bottom(), zero)?;
        }
        Ok(())
    }
}

impl GridLines<'_> {
    fn lines<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        ticks: &[Tick],
        map: Mapping,
        vertical: bool,
        area: Rect,
    ) -> AureaResult<()> {
        for tick in ticks {
            let Placed::At(at) = map.place(tick.value) else {
                continue;
            };
            let line = if tick.major {
                self.style.major
            } else {
                self.style.minor
            };
            if vertical {
                vline(ui, at, area.y, area.bottom(), line)?;
            } else {
                hline(ui, at, area.x, area.right(), line)?;
            }
        }
        Ok(())
    }
}

/// Which side of the plotting area an axis runs along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisSide {
    /// The x axis, labels centred under their ticks.
    Bottom,
    /// The y axis, labels right-aligned against their ticks.
    Left,
}

/// An axis line, its ticks and their labels, along one side of a plotting area.
#[derive(Clone, Copy, Debug)]
pub struct AxisMarks<'t> {
    pub ticks: &'t [Tick],
    pub map: Mapping,
    pub side: AxisSide,
    pub style: AxisStyle,
}

impl Component for AxisMarks<'_> {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, area: Rect) -> AureaResult<()> {
        let s = self.style;
        if let Some(line) = s.line {
            match self.side {
                AxisSide::Bottom => hline(ui, area.bottom(), area.x, area.right(), line)?,
                AxisSide::Left => vline(ui, area.x, area.y, area.bottom(), line)?,
            }
        }
        let fonts = ui.fonts();
        for tick in self.ticks {
            let Placed::At(at) = self.map.place(tick.value) else {
                continue;
            };
            let length = if tick.major {
                s.tick_length
            } else {
                s.minor_tick_length
            };
            if let Some(mark) = s.tick {
                match self.side {
                    AxisSide::Bottom => vline(ui, at, area.bottom(), area.bottom() + length, mark)?,
                    AxisSide::Left => hline(ui, at, area.x - length, area.x, mark)?,
                }
            }
            let Some(color) = s.label.filter(|_| tick.major && !tick.label.is_empty()) else {
                continue;
            };
            let m = ui.measure(&tick.label, &fonts.small);
            let gap = length + s.label_gap;
            let origin = match self.side {
                AxisSide::Bottom => Point::new(at - m.width / 2.0, area.bottom() + gap + m.ascent),
                AxisSide::Left => Point::new(area.x - gap - m.width, at + m.ascent / 2.0),
            };
            ui.ctx
                .draw_text_with_font(&tick.label, origin, &fonts.small, &fill(color))?;
        }
        Ok(())
    }
}

/// One series, already projected to the screen.
#[derive(Clone, Copy, Debug)]
pub struct Trace<'p> {
    pub points: &'p [Point],
    /// The points are per-column spans rather than the trace itself.
    pub envelope: bool,
    pub plot: Plot,
    pub color: Color,
    pub width: f32,
    /// A mark at every point too, this big.
    pub marks: Option<f32>,
    /// Where an area or bars fill down to, in pixels.
    pub base: f32,
    /// An area's fill opacity, 0 to 1.
    pub fill_alpha: f32,
}

impl Component for Trace<'_> {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, _area: Rect) -> AureaResult<()> {
        let points = self.points;
        if points.is_empty() {
            return Ok(());
        }
        if self.envelope {
            return self.spans(ui);
        }
        match self.plot {
            Plot::Line => ui.polyline(points, self.width, self.color)?,
            Plot::Step => ui.stroke_through(steps(points), self.width, self.color)?,
            Plot::Points => {}
            Plot::Area => {
                let (first, last) = (points[0], points[points.len() - 1]);
                let outline = [Point::new(first.x, self.base)]
                    .into_iter()
                    .chain(points.iter().copied())
                    .chain([Point::new(last.x, self.base)]);
                ui.fill_through(outline, self.color.fade(self.fill_alpha))?;
                ui.polyline(points, self.width, self.color)?;
            }
            Plot::Bars => self.bars(ui)?,
        }
        let radius = match self.plot {
            Plot::Points => Some(self.marks.unwrap_or(2.0)),
            _ => self.marks,
        };
        if let Some(radius) = radius {
            for point in points {
                ui.circle(*point, radius, self.color)?;
            }
        }
        Ok(())
    }
}

impl Trace<'_> {
    fn spans<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let thickness = self.width.max(1.0);
        let points = self.points;
        let mut i = 0;
        while i < points.len() {
            let first = points[i];
            let second = points
                .get(i + 1)
                .filter(|next| (next.x - first.x).abs() < 0.5);
            let (top, bottom) = second.map_or((first.y, first.y), |next| {
                (first.y.min(next.y), first.y.max(next.y))
            });
            let height = (bottom - top).max(thickness);
            ui.fill(Rect::new(first.x, top, thickness, height), self.color)?;
            i += if second.is_some() { 2 } else { 1 };
        }
        Ok(())
    }

    fn bars<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let width = bar_width(self.points);
        for point in self.points {
            let height = (point.y - self.base).abs();
            if height > 0.0 {
                let top = point.y.min(self.base);
                ui.fill(
                    Rect::new(point.x - width / 2.0, top, width, height),
                    self.color,
                )?;
            }
        }
        Ok(())
    }
}

fn steps(points: &[Point]) -> impl Iterator<Item = Point> + '_ {
    let corners = points
        .windows(2)
        .flat_map(|pair| [Point::new(pair[1].x, pair[0].y), pair[1]]);
    points.first().copied().into_iter().chain(corners)
}

/// A line across a plotting area at one place, with a label by it.
#[derive(Clone, Copy, Debug)]
pub struct Marker<'l> {
    /// The pixel it sits at: x for a vertical marker, y for a horizontal.
    pub at: f32,
    pub vertical: bool,
    pub stroke: Stroke,
    /// Written beside it in the small font.
    pub label: &'l str,
}

impl Component for Marker<'_> {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, area: Rect) -> AureaResult<()> {
        let origin = if self.vertical {
            vline(ui, self.at, area.y, area.bottom(), self.stroke)?;
            Point::new(self.at + 3.0, area.y + 12.0)
        } else {
            hline(ui, self.at, area.x, area.right(), self.stroke)?;
            Point::new(area.x + 3.0, self.at - 3.0)
        };
        if self.label.is_empty() {
            return Ok(());
        }
        let fonts = ui.fonts();
        ui.ctx
            .draw_text_with_font(self.label, origin, &fonts.small, &fill(self.stroke.color))
    }
}

fn hline<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    y: f32,
    x0: f32,
    x1: f32,
    s: Stroke,
) -> AureaResult<()> {
    ui.line(Point::new(x0, y), Point::new(x1, y), s.width, s.color)
}

fn vline<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    x: f32,
    y0: f32,
    y1: f32,
    s: Stroke,
) -> AureaResult<()> {
    ui.line(Point::new(x, y0), Point::new(x, y1), s.width, s.color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_hold_then_jump() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 5.0),
            Point::new(2.0, 3.0),
        ];
        let corners: Vec<_> = steps(&points).collect();
        assert_eq!(
            corners,
            vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(1.0, 5.0),
                Point::new(2.0, 5.0),
                Point::new(2.0, 3.0),
            ]
        );
        assert_eq!(steps(&[]).count(), 0);
    }
}
