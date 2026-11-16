//! A ruler along an edge: marks and numbers at a step that follows the zoom.

use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::frame::{Align, Eased, Response, Ui};
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

/// What a [`Ruler`] draws, from [`Ruler::look`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulerParts {
    /// The unit at the start of the rect.
    pub origin: f32,
    /// How many pixels a unit is.
    pub pixels_per_unit: f32,
    /// Whether it runs top to bottom, with its marks from the right edge.
    pub vertical: bool,
    /// The unit marked in the accent colour, such as where the pointer is.
    pub marker: Option<f32>,
    /// The step between numbers and between small marks, `None` when the zoom cannot be drawn.
    pub steps: Option<(f32, f32)>,
    /// Decimal places the numbers need; `0` when no step is drawn.
    pub decimals: usize,
    /// Colour of the ground.
    pub surface: Color,
    /// Colour of the marks.
    pub mark: Color,
    /// Colour of the numbers.
    pub text: Color,
    /// Colour of the marker.
    pub accent: Color,
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

    /// Works out what the ruler would draw, without drawing: the app draws it, or
    /// [`Look::paint_default`] does. It takes no input.
    pub fn look<Id: WidgetId>(self, ui: &Ui<'_, Id>, rect: Rect) -> Look<RulerParts> {
        let p = ui.theme.palette;
        let ppu = self.pixels_per_unit;
        let steps = (ppu > 0.0 && ppu.is_finite()).then(|| steps(ppu));
        Look {
            rect,
            state: Eased {
                enabled: 1.0,
                ..Eased::default()
            },
            held_at: None,
            response: Response {
                rect,
                ..Response::default()
            },
            parts: RulerParts {
                origin: self.origin,
                pixels_per_unit: ppu,
                vertical: self.vertical,
                marker: self.marker,
                steps,
                decimals: steps.map_or(0, |(major, _)| decimals(major)),
                surface: p.surface,
                mark: p.border,
                text: p.dim,
                accent: p.accent,
            },
        }
    }

    /// Draws the ruler in `rect`.
    pub fn show<Id: WidgetId>(self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        self.look(ui, rect).paint_default(ui)
    }
}

impl Look<RulerParts> {
    /// The ruler's own look: the ground, then the marks across it.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_ground(ui)?;
        self.paint_marks(ui)
    }

    /// The ground the marks sit on.
    pub fn paint_ground<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        ui.fill(self.rect, self.parts.surface)
    }

    /// The marks, the numbers and the marker, clipped to the rect; nothing when the zoom cannot be
    /// drawn.
    pub fn paint_marks<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let Some((major, minor)) = self.parts.steps else {
            return Ok(());
        };
        let parts = &self.parts;
        let rect = self.rect;
        let length = if parts.vertical {
            rect.height
        } else {
            rect.width
        };
        ui.clipped(rect, |ui| {
            let first = (parts.origin / minor).floor();
            let count = (length / parts.pixels_per_unit / minor).ceil() + 1.0;
            let every = (major / minor).round().max(1.0);
            let mut i = 0.0;
            while i <= count {
                let k = first + i;
                let unit = k * minor;
                let at = (unit - parts.origin) * parts.pixels_per_unit;
                let numbered = (k / every - (k / every).round()).abs() < 1e-3;
                let long = if numbered { LONG } else { SHORT };
                self.tick(ui, at, long, parts.mark)?;
                if numbered {
                    self.number(ui, at, unit, parts.decimals, parts.text)?;
                }
                i += 1.0;
            }
            if let Some(unit) = parts.marker {
                let at = (unit - parts.origin) * parts.pixels_per_unit;
                self.tick(ui, at, 1.0, parts.accent)?;
            }
            Ok(())
        })
    }

    fn tick<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        at: f32,
        share: f32,
        color: Color,
    ) -> AureaResult<()> {
        let rect = self.rect;
        let bar = if self.parts.vertical {
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
        at: f32,
        unit: f32,
        places: usize,
        color: Color,
    ) -> AureaResult<()> {
        let rect = self.rect;
        let fonts = ui.fonts();
        let cell = if self.parts.vertical {
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
    fn look_paints_the_apps_ground_and_keeps_the_marks() {
        let (mut raster, mut state) = (CpuRasterizer::new(200, 60), UiState::<u8>::new());
        let theme = Theme::dark();
        {
            let mut ctx = raster.begin_frame().expect("frame");
            ctx.clear(Color::rgb(0, 0, 0)).expect("clear");
            let mut ui = state.frame(&mut *ctx, &theme);
            let ruler = Ruler::horizontal(-7.0, 1.5).look(&ui, Rect::new(0.0, 0.0, 200.0, 20.0));
            ui.fill(ruler.rect, Color::rgb(0, 200, 0)).expect("ground");
            ruler.paint_marks(&mut ui).expect("marks");
        }
        raster.end_frame().expect("end");
        let at = |x: usize, y: usize| raster.pixels()[y * 200 + x];
        assert_eq!(at(100, 2), 0xFF00_C800, "the app's ground, not the default");
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
