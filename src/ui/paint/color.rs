//! The fields a colour picker is drawn from: a saturation and value square for one hue, a hue
//! strip, an alpha strip over a checkerboard, and the ring that marks the picked spot.

use crate::AureaResult;
use crate::render::{Color, GradientStop, Hsv, LinearGradient, Point, Rect};
use crate::ui::frame::Ui;
use crate::ui::state::WidgetId;

use super::Component;

/// Light and dark squares under a see-through colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Checker {
    /// Light square colour.
    pub light: Color,
    /// Dark square colour.
    pub dark: Color,
    /// One square's side.
    pub cell: f32,
}

impl Checker {
    /// The default checkerboard.
    pub const fn new() -> Self {
        Self {
            light: Color::rgb(255, 255, 255),
            dark: Color::rgb(204, 204, 204),
            cell: 6.0,
        }
    }
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for Checker {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        ui.fill(rect, self.light)?;
        let cell = self.cell.max(1.0);
        let (mut y, mut row) = (rect.y, 0u32);
        while y < rect.bottom() {
            let h = cell.min(rect.bottom() - y);
            let mut x = rect.x + if row % 2 == 0 { 0.0 } else { cell };
            while x < rect.right() {
                ui.fill(Rect::new(x, y, cell.min(rect.right() - x), h), self.dark)?;
                x += cell * 2.0;
            }
            y += cell;
            row += 1;
        }
        Ok(())
    }
}

/// Every saturation (left to right) and value (top to bottom) of one hue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvField {
    /// In degrees.
    pub hue: f32,
}

impl Component for SvField {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        let pure = Hsv::new(self.hue, 1.0, 1.0, 1.0).to_color();
        ui.fill(rect, pure)?;
        let white = Color::rgb(255, 255, 255);
        let black = Color::rgb(0, 0, 0);
        let across = (Point::new(rect.x, rect.y), Point::new(rect.right(), rect.y));
        let down = (
            Point::new(rect.x, rect.y),
            Point::new(rect.x, rect.bottom()),
        );
        gradient(ui, rect, across, &[white, white.with_alpha(0)])?;
        gradient(ui, rect, down, &[black.with_alpha(0), black])
    }
}

/// Hues all the way round, red at both ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HueStrip {
    /// Hue runs top to bottom, else left to right.
    pub vertical: bool,
}

impl Component for HueStrip {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        let stops: [Color; 7] = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0]
            .map(|h| Hsv::new(h, 1.0, 1.0, 1.0).to_color());
        gradient(ui, rect, ends(rect, self.vertical), &stops)
    }
}

/// `color` from clear to opaque over a checkerboard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaStrip {
    /// The colour to fade.
    pub color: Color,
    /// Background squares.
    pub checker: Checker,
    /// Opaque at the top and clear at the bottom, else clear on the left.
    pub vertical: bool,
}

impl Component for AlphaStrip {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        self.checker.paint(ui, rect)?;
        let (clear, opaque) = (self.color.with_alpha(0), self.color.with_alpha(255));
        let stops = if self.vertical {
            [opaque, clear]
        } else {
            [clear, opaque]
        };
        gradient(ui, rect, ends(rect, self.vertical), &stops)
    }
}

/// A ring round the picked spot, light inside a dark edge so it shows on any colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickRing {
    /// Centre of the ring.
    pub at: Point,
    /// Radius of the ring.
    pub radius: f32,
}

impl Component for PickRing {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, _rect: Rect) -> AureaResult<()> {
        let r = self.radius;
        let ring = Rect::new(self.at.x - r, self.at.y - r, r * 2.0, r * 2.0);
        ui.border(ring.inset(-1.0), r + 1.0, 1.0, Color::rgba(0, 0, 0, 140))?;
        ui.border(ring, r, 2.0, Color::rgb(255, 255, 255))
    }
}

fn ends(rect: Rect, vertical: bool) -> (Point, Point) {
    let start = Point::new(rect.x, rect.y);
    if vertical {
        (start, Point::new(rect.x, rect.bottom()))
    } else {
        (start, Point::new(rect.right(), rect.y))
    }
}

fn gradient<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    rect: Rect,
    (start, end): (Point, Point),
    colors: &[Color],
) -> AureaResult<()> {
    let last = colors.len().saturating_sub(1).max(1);
    let stops = colors
        .iter()
        .enumerate()
        .map(|(i, &color)| GradientStop {
            offset: offset(i, last),
            color,
        })
        .collect();
    ui.ctx
        .fill_linear_gradient(&LinearGradient { start, end, stops }, rect)
}

#[allow(clippy::cast_precision_loss)]
fn offset(i: usize, last: usize) -> f32 {
    i as f32 / last as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_run_from_zero_to_one() {
        assert_eq!(offset(0, 6), 0.0);
        assert_eq!(offset(6, 6), 1.0);
        assert!((offset(3, 6) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_vertical_strip_runs_down() {
        let rect = Rect::new(10.0, 20.0, 16.0, 100.0);
        assert_eq!(ends(rect, true).1, Point::new(10.0, 120.0));
        assert_eq!(ends(rect, false).1, Point::new(26.0, 20.0));
    }
}
