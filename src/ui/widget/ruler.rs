//! A ruler along an edge: marks and numbers at a step that follows the zoom.

use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::frame::{Align, Ui};
use crate::ui::state::WidgetId;

/// Closest two numbered marks may come, in logical pixels.
const NUMBER_GAP: f32 = 56.0;
/// Closest two small marks may come.
const MARK_GAP: f32 = 6.0;
/// A numbered mark's and a small mark's length, as shares of the ruler's depth.
const LONG: f32 = 0.5;
const SHORT: f32 = 0.2;

/// The smallest of 1, 2 or 5 times a power of ten at least `at_least`.
fn nice(at_least: f32) -> f32 {
    if at_least <= 0.0 || !at_least.is_finite() {
        return 1.0;
    }
    let power = 10_f32.powf(at_least.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * power)
        .find(|step| *step >= at_least * 0.9999)
        .unwrap_or(power * 10.0)
}

/// The step between numbers and between small marks at `pixels_per_unit`.
fn steps(pixels_per_unit: f32) -> (f32, f32) {
    let major = nice(NUMBER_GAP / pixels_per_unit);
    let minor = [10.0, 5.0, 2.0]
        .into_iter()
        .map(|parts| major / parts)
        .find(|minor| minor * pixels_per_unit >= MARK_GAP)
        .unwrap_or(major);
    (major, minor)
}

/// How many decimals `step` needs.
fn decimals(step: f32) -> usize {
    if step >= 1.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let places = (-step.log10()).ceil().max(0.0) as usize;
    places.min(6)
}

/// A ruler of a view's own units, drawn along a canvas's edge. `origin` is the unit at the start
/// of the rect, and `pixels_per_unit` the zoom.
///
/// ```no_run
/// # use aurea::render::Rect;
/// # use aurea::ui::{Ruler, Ui};
/// # fn f(ui: &mut Ui<'_, u8>) -> aurea::AureaResult<()> {
/// Ruler::horizontal(-20.0, 2.0).show(ui, Rect::new(0.0, 0.0, 800.0, 20.0))
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Ruler {
    origin: f32,
    pixels_per_unit: f32,
    vertical: bool,
    marker: Option<f32>,
}

impl Ruler {
    /// A ruler running left to right, with its marks rising from its bottom edge.
    pub fn horizontal(origin: f32, pixels_per_unit: f32) -> Self {
        Self {
            origin,
            pixels_per_unit,
            vertical: false,
            marker: None,
        }
    }

    /// A ruler running top to bottom, with its marks coming from its right edge.
    pub fn vertical(origin: f32, pixels_per_unit: f32) -> Self {
        Self {
            vertical: true,
            ..Self::horizontal(origin, pixels_per_unit)
        }
    }

    /// Marks this unit in the accent colour, such as where the pointer is.
    pub fn marker(mut self, unit: Option<f32>) -> Self {
        self.marker = unit;
        self
    }

    /// Draws the ruler in `rect`.
    pub fn show<Id: WidgetId>(self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        ui.fill(rect, ui.theme.palette.surface)?;
        if self.pixels_per_unit <= 0.0 || !self.pixels_per_unit.is_finite() {
            return Ok(());
        }
        let (major, minor) = steps(self.pixels_per_unit);
        let length = if self.vertical {
            rect.height
        } else {
            rect.width
        };
        let (mark, text) = (ui.theme.palette.border, ui.theme.palette.dim);
        ui.clipped(rect, |ui| {
            let first = (self.origin / minor).floor();
            let count = (length / self.pixels_per_unit / minor).ceil() + 1.0;
            let every = (major / minor).round().max(1.0);
            let mut i = 0.0;
            while i <= count {
                let k = first + i;
                let unit = k * minor;
                let at = (unit - self.origin) * self.pixels_per_unit;
                let numbered = (k / every - (k / every).round()).abs() < 1e-3;
                let long = if numbered { LONG } else { SHORT };
                self.tick(ui, rect, at, long, mark)?;
                if numbered {
                    self.number(ui, rect, at, unit, decimals(major), text)?;
                }
                i += 1.0;
            }
            if let Some(unit) = self.marker {
                let at = (unit - self.origin) * self.pixels_per_unit;
                self.tick(ui, rect, at, 1.0, ui.theme.palette.accent)?;
            }
            Ok(())
        })
    }

    fn tick<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        rect: Rect,
        at: f32,
        share: f32,
        color: Color,
    ) -> AureaResult<()> {
        let bar = if self.vertical {
            let long = rect.width * share;
            Rect::new(rect.right() - long, rect.y + at, long, 1.0)
        } else {
            let long = rect.height * share;
            Rect::new(rect.x + at, rect.bottom() - long, 1.0, long)
        };
        ui.fill(bar, color)
    }

    fn number<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        rect: Rect,
        at: f32,
        unit: f32,
        places: usize,
        color: Color,
    ) -> AureaResult<()> {
        let fonts = ui.fonts();
        let cell = if self.vertical {
            Rect::new(
                rect.x + 2.0,
                rect.y + at + 1.0,
                rect.width - 2.0,
                NUMBER_GAP,
            )
        } else {
            Rect::new(rect.x + at + 3.0, rect.y, NUMBER_GAP, rect.height * 0.7)
        };
        ui.text_fmt(
            format_args!("{unit:.places$}"),
            cell,
            &fonts.small,
            color,
            Align::Start,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::ui::theme::Theme;

    #[test]
    fn the_step_follows_the_zoom() {
        assert_eq!(steps(1.0), (100.0, 10.0), "a pixel a unit");
        assert_eq!(steps(10.0), (10.0, 1.0));
        assert_eq!(steps(0.1), (1000.0, 100.0));
        let (major, minor) = steps(37.0);
        assert_eq!(major, 2.0);
        assert!(major * 37.0 >= NUMBER_GAP && minor * 37.0 >= MARK_GAP);
        assert_eq!(nice(1.2), 2.0);
        assert_eq!(nice(5.0), 5.0);
        assert!((nice(0.03) - 0.05).abs() < 1e-6);
    }

    #[test]
    fn numbers_keep_the_places_their_step_needs() {
        assert_eq!(decimals(100.0), 0);
        assert_eq!(decimals(1.0), 0);
        assert_eq!(decimals(0.5), 1);
        assert_eq!(decimals(0.02), 2);
    }

    #[test]
    fn it_draws_marks_in_its_rect_and_nothing_outside() {
        let (mut raster, mut state) = (CpuRasterizer::new(200, 60), UiState::<u8>::new());
        let theme = Theme::dark();
        {
            let mut ctx = raster.begin_frame().expect("frame");
            ctx.clear(Color::rgb(0, 0, 0)).expect("clear");
            let mut ui = state.frame(&mut *ctx, &theme);
            Ruler::horizontal(-7.0, 1.5)
                .marker(Some(30.0))
                .show(&mut ui, Rect::new(0.0, 0.0, 200.0, 20.0))
                .expect("ruler");
        }
        raster.end_frame().expect("end");
        let at = |x: usize, y: usize| raster.pixels()[y * 200 + x];
        let row: Vec<u32> = (0..200).map(|x| at(x, 18)).collect();
        let surface = row.iter().copied().fold(0, u32::max);
        assert!(row.iter().any(|p| *p != row[0]), "marks show: {surface:x}");
        assert!((0..200).all(|x| at(x, 25) == 0xFF00_0000), "nothing below");
    }

    #[test]
    fn a_bad_zoom_draws_only_its_ground() {
        let (mut raster, mut state) = (CpuRasterizer::new(50, 20), UiState::<u8>::new());
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("frame");
        let mut ui = state.frame(&mut *ctx, &theme);
        for zoom in [0.0, -2.0, f32::NAN, f32::INFINITY] {
            Ruler::vertical(0.0, zoom)
                .show(&mut ui, Rect::new(0.0, 0.0, 50.0, 20.0))
                .expect("ruler");
        }
    }
}
