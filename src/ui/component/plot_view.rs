//! Zooming and panning a plot's data ranges, and the value under the
//! pointer.
//!
//! [`PlotView`] is [`PanZoom`](super::PanZoom) for data: it moves ranges of
//! values rather than a picture. The wheel zooms where the pointer is (x, or
//! y with Shift), a left drag pans, and a double click puts the axes back.
//! With focus, `=` and `-` zoom, the arrows pan and Home resets.
//!
//! Zooming works through the axis [`Mapping`], so a log axis zooms by
//! decades the way it is drawn rather than by raw values.

use crate::render::Point;
use crate::ui::frame::Ui;
use crate::ui::graph::{Mapping, Range};
use crate::ui::state::{Interaction, WidgetId};
use crate::window::KeyCode;

/// The user's view onto a plot. Without a zoom, the axes show what their
/// bounds say; once zoomed, the view holds a range of its own until reset.
#[derive(Clone, Debug, PartialEq)]
pub struct PlotView {
    x: Option<Range>,
    y: Option<Range>,
    /// The pointer's last position during a pan.
    drag: Option<Point>,
    hover: Option<(f64, f64)>,
    pub zoom_x: bool,
    pub zoom_y: bool,
    /// How much a wheel unit zooms: the span is multiplied by `e` to the
    /// minus this times the wheel's travel.
    pub wheel_zoom: f32,
}

impl Default for PlotView {
    fn default() -> Self {
        Self::new()
    }
}

/// How far a key press pans, as a part of the span.
const KEY_PAN: f32 = 0.1;
/// How much a key press zooms.
const KEY_ZOOM: f32 = 1.25;

impl PlotView {
    /// Both axes zoom and pan.
    pub fn new() -> Self {
        // macOS hands over a trackpad's travel in points, many per gesture;
        // elsewhere a wheel notch is 1.0.
        let wheel_zoom = if cfg!(target_os = "macos") {
            0.01
        } else {
            0.18
        };
        Self {
            x: None,
            y: None,
            drag: None,
            hover: None,
            zoom_x: true,
            zoom_y: true,
            wheel_zoom,
        }
    }

    /// Only x moves: for a time series, where y is better left to the data.
    pub fn x_only(mut self) -> Self {
        self.zoom_y = false;
        self
    }

    /// The x range the user picked, if they zoomed.
    pub fn x(&self) -> Option<Range> {
        self.x
    }

    /// The y range the user picked, if they zoomed.
    pub fn y(&self) -> Option<Range> {
        self.y
    }

    /// Whether either axis is zoomed or panned away from its bounds.
    pub fn zoomed(&self) -> bool {
        self.x.is_some() || self.y.is_some()
    }

    /// The data value under the pointer, from the last frame it was over
    /// the plot.
    pub fn hovered(&self) -> Option<(f64, f64)> {
        self.hover
    }

    /// Back to what the axes' bounds say.
    pub fn reset(&mut self) {
        self.x = None;
        self.y = None;
    }

    /// Takes this frame's input on a plot drawn with `maps`. Returns whether
    /// the view moved.
    pub fn follow<Id: WidgetId>(
        &mut self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        input: Interaction,
        maps: (Mapping, Mapping),
    ) -> bool {
        let pointer = ui.pointer();
        self.hover = pointer
            .filter(|_| input.hovered)
            .and_then(|at| Some((maps.0.value_at(at.x)?, maps.1.value_at(at.y)?)));
        let before = (self.x, self.y);
        if ui.press(id, 0).is_some_and(|press| press.clicks >= 2) {
            self.reset();
        } else {
            if input.hovered {
                self.wheel(ui, pointer, maps);
            }
            self.drag_with(input.held.then_some(pointer).flatten(), maps);
            if input.focused {
                self.keys(ui, maps);
            }
        }
        (self.x, self.y) != before
    }

    fn wheel<Id: WidgetId>(
        &mut self,
        ui: &mut Ui<'_, Id>,
        at: Option<Point>,
        maps: (Mapping, Mapping),
    ) {
        let Some(at) = at else {
            return;
        };
        let (_, dy) = ui.take_wheel();
        if dy == 0.0 {
            return;
        }
        let factor = (-dy * self.wheel_zoom).exp();
        if ui.modifiers().shift {
            self.zoom_y_at(maps.1, at.y, factor);
        } else {
            self.zoom_x_at(maps.0, at.x, factor);
        }
    }

    fn drag_with(&mut self, at: Option<Point>, (x_map, y_map): (Mapping, Mapping)) {
        let Some(at) = at else {
            self.drag = None;
            return;
        };
        if let Some(last) = self.drag {
            self.pan(x_map, y_map, at.x - last.x, at.y - last.y);
        }
        self.drag = Some(at);
    }

    fn keys<Id: WidgetId>(&mut self, ui: &Ui<'_, Id>, (x_map, y_map): (Mapping, Mapping)) {
        let (x0, x1) = x_map.pixels();
        let (y0, y1) = y_map.pixels();
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (w, h) = ((x1 - x0).abs(), (y1 - y0).abs());
        for (key, _) in ui.keys() {
            match key {
                KeyCode::Equals => {
                    self.zoom_x_at(x_map, cx, 1.0 / KEY_ZOOM);
                    self.zoom_y_at(y_map, cy, 1.0 / KEY_ZOOM);
                }
                KeyCode::Minus => {
                    self.zoom_x_at(x_map, cx, KEY_ZOOM);
                    self.zoom_y_at(y_map, cy, KEY_ZOOM);
                }
                KeyCode::Left => self.pan(x_map, y_map, w * KEY_PAN, 0.0),
                KeyCode::Right => self.pan(x_map, y_map, -w * KEY_PAN, 0.0),
                KeyCode::Up => self.pan(x_map, y_map, 0.0, h * KEY_PAN),
                KeyCode::Down => self.pan(x_map, y_map, 0.0, -h * KEY_PAN),
                KeyCode::Home => self.reset(),
                _ => {}
            }
        }
    }

    fn zoom_x_at(&mut self, map: Mapping, at: f32, factor: f32) {
        if self.zoom_x {
            self.x = zoomed(map, at, factor).or(self.x);
        }
    }

    fn zoom_y_at(&mut self, map: Mapping, at: f32, factor: f32) {
        if self.zoom_y {
            self.y = zoomed(map, at, factor).or(self.y);
        }
    }

    /// Moves the content `dx`, `dy` pixels, as a hand dragging it would.
    fn pan(&mut self, x_map: Mapping, y_map: Mapping, dx: f32, dy: f32) {
        if self.zoom_x && dx != 0.0 {
            self.x = shifted(x_map, -dx).or(self.x);
        }
        if self.zoom_y && dy != 0.0 {
            self.y = shifted(y_map, -dy).or(self.y);
        }
    }
}

/// The range `map` shows, scaled by `factor` about pixel `at`.
fn zoomed(map: Mapping, at: f32, factor: f32) -> Option<Range> {
    let (lo, hi) = map.pixels();
    span(map, at + (lo - at) * factor, at + (hi - at) * factor)
}

/// The range `map` shows, moved `by` pixels.
fn shifted(map: Mapping, by: f32) -> Option<Range> {
    let (lo, hi) = map.pixels();
    span(map, lo + by, hi + by)
}

fn span(map: Mapping, lo: f32, hi: f32) -> Option<Range> {
    let (a, b) = (map.value_at(lo)?, map.value_at(hi)?);
    let range = Range::new(a.min(b), a.max(b));
    (range.min.is_finite() && range.max.is_finite() && !range.is_degenerate()).then_some(range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::graph::Scale;

    fn x_map(min: f64, max: f64) -> Mapping {
        Mapping::new(Range::new(min, max), Scale::Linear, 0.0, 100.0)
    }

    fn close(a: Range, b: Range) -> bool {
        (a.min - b.min).abs() < 1e-6 && (a.max - b.max).abs() < 1e-6
    }

    #[test]
    fn zooming_keeps_the_value_under_the_pointer() {
        let map = x_map(0.0, 100.0);
        let range = zoomed(map, 25.0, 0.5).expect("zoomed");
        assert!(close(range, Range::new(12.5, 62.5)), "got {range:?}");
    }

    #[test]
    fn panning_moves_the_range_against_the_drag() {
        let map = x_map(0.0, 100.0);
        let range = shifted(map, -10.0).expect("moved");
        assert!(close(range, Range::new(-10.0, 90.0)), "got {range:?}");
    }

    #[test]
    fn a_log_axis_zooms_by_decades() {
        let map = Mapping::new(Range::new(1.0, 1000.0), Scale::Log10, 0.0, 300.0);
        let range = zoomed(map, 0.0, 2.0 / 3.0).expect("zoomed");
        assert!(close(range, Range::new(1.0, 100.0)), "got {range:?}");
    }

    #[test]
    fn an_x_only_view_leaves_y_to_the_data() {
        let mut view = PlotView::new().x_only();
        let map = x_map(0.0, 100.0);
        view.pan(map, map, 10.0, 10.0);
        assert!(view.x().is_some());
        assert_eq!(view.y(), None);
        view.reset();
        assert!(!view.zoomed());
    }
}
