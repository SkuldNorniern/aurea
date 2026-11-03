//! A colour wheel: a hue ring around a saturation and value square.

use std::f32::consts::{SQRT_2, TAU};

use super::Widget;
use super::color::{follow_square, merge, step};
use crate::AureaResult;
use crate::render::{Hsv, Paint, Path, PathCommand, Picture, Point, Rect};
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, PickRing, SvField};
use crate::ui::state::{Interaction, Sense, WidgetId};
use crate::window::KeyCode;

const RING: u16 = 0;
const SQUARE: u16 = 1;
/// Slices of the ring: fine enough that neighbours read as one sweep of colour.
const SLICES: u16 = 72;
/// The ring's thickness, as a share of the wheel's side.
const THICKNESS: f32 = 0.12;
/// Room between the ring and the square's corners.
const GAP: f32 = 4.0;
/// How far outside the ring a press still takes it.
const SLACK: f32 = 3.0;

/// Where the parts of a wheel go in `rect`.
#[derive(Clone, Copy, Debug)]
struct Layout {
    outer: Rect,
    center: Point,
    radius: f32,
    inner: f32,
    square: Rect,
}

impl Layout {
    fn of(rect: Rect) -> Self {
        let side = rect.width.min(rect.height).max(0.0);
        let outer = rect.centered(side, side);
        let radius = side / 2.0;
        let inner = radius * (1.0 - 2.0 * THICKNESS);
        let center = Point::new(outer.x + radius, outer.y + radius);
        let square_side = ((inner - GAP).max(0.0)) * SQRT_2;
        Self {
            outer,
            center,
            radius,
            inner,
            square: outer.centered(square_side, square_side),
        }
    }

    fn on_ring(&self, p: Point) -> bool {
        let d = (p.x - self.center.x).hypot(p.y - self.center.y);
        d >= self.inner - SLACK && d <= self.radius + SLACK
    }

    /// The hue at `p`, in degrees: red to the right, turning clockwise on screen.
    fn hue_at(&self, p: Point) -> f32 {
        (p.y - self.center.y)
            .atan2(p.x - self.center.x)
            .to_degrees()
            .rem_euclid(360.0)
    }

    /// The middle of the ring at `hue`.
    fn ring_point(&self, hue: f32) -> Point {
        let (sin, cos) = hue.to_radians().sin_cos();
        let middle = (self.radius + self.inner) / 2.0;
        Point::new(self.center.x + cos * middle, self.center.y + sin * middle)
    }
}

/// A colour wheel, with hit testing for its ring and square. The app keeps `art`, the ring drawn
/// once as a [`Picture`], which saves a frame its dozens of paths; it is redrawn when the wheel's
/// size or the scale changes.
///
/// ```no_run
/// # use aurea::render::{Hsv, Picture, Rect};
/// # use aurea::ui::{ColorWheel, Ui};
/// # fn f(ui: &mut Ui<'_, u8>, colour: &mut Hsv, art: &mut Option<Picture>) {
/// let response = ColorWheel::new(colour, art)
///     .show(ui, 1, Rect::new(0.0, 0.0, 200.0, 200.0))
///     .expect("draw");
/// # let _ = response;
/// # }
/// ```
#[derive(Debug)]
pub struct ColorWheel<'v> {
    value: &'v mut Hsv,
    art: &'v mut Option<Picture>,
    enabled: bool,
}

impl<'v> ColorWheel<'v> {
    /// A wheel editing `value`.
    pub fn new(value: &'v mut Hsv, art: &'v mut Option<Picture>) -> Self {
        Self {
            value,
            art,
            enabled: true,
        }
    }

    /// Whether it takes input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Shows the wheel in the middle of `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

impl<Id: WidgetId> Widget<Id> for ColorWheel<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let at = Layout::of(rect);
        let before = *self.value;
        let ring = ui.interact(id, RING, at.outer, Sense::Adjust, self.enabled);
        let square = ui.interact(id, SQUARE, at.square, Sense::Adjust, self.enabled);
        follow_ring(ui, id, ring, &at, self.value);
        follow_square(ui, square, at.square, self.value);

        let mut response = Response::new(rect, Interaction::default());
        merge(&mut response, ring);
        merge(&mut response, square);
        if *self.value != before {
            response.changed = true;
            ui.mark_changed();
        }

        draw_ring(ui, &at, self.art)?;
        PickRing {
            at: at.ring_point(self.value.h),
            radius: (at.radius - at.inner) / 2.0,
        }
        .paint(ui, at.outer)?;
        SvField { hue: self.value.h }.paint(ui, at.square)?;
        let spot = Point::new(
            at.square.x + self.value.s * at.square.width,
            at.square.y + (1.0 - self.value.v) * at.square.height,
        );
        PickRing {
            at: spot,
            radius: 5.0,
        }
        .paint(ui, at.outer)?;
        let focus = ui.eased(id, SQUARE, square).focus;
        FocusRing::new(ui.theme, 0.0, focus).paint(ui, at.square)?;
        Ok(response)
    }
}

fn follow_ring<Id: WidgetId>(
    ui: &Ui<'_, Id>,
    id: Id,
    input: Interaction,
    at: &Layout,
    value: &mut Hsv,
) {
    // Corners outside the ring and the hole inside it take no press.
    let pressed_on_ring = ui
        .pressed_in(id, RING, at.outer)
        .is_some_and(|p| at.on_ring(p));
    if input.held
        && pressed_on_ring
        && let Some(p) = ui.pointer()
    {
        value.h = at.hue_at(p);
    }
    if !input.focused {
        return;
    }
    let by = step(ui) * 360.0;
    for (key, _) in ui.keys() {
        match key {
            KeyCode::Left => value.h = (value.h - by).rem_euclid(360.0),
            KeyCode::Right => value.h = (value.h + by).rem_euclid(360.0),
            _ => {}
        }
    }
}

fn draw_ring<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    at: &Layout,
    art: &mut Option<Picture>,
) -> AureaResult<()> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let side = at.outer.width.ceil() as u32;
    let scale = ui.ctx.scale_factor();
    let stale = art
        .as_ref()
        .is_none_or(|picture| picture.size() != (side, side) || picture.scale() != scale);
    if stale {
        *art = (side > 0)
            .then(|| record_ring(side, scale, at.radius, at.inner))
            .transpose()?;
    }
    match art {
        Some(picture) => picture.draw(ui.ctx, Point::new(at.outer.x, at.outer.y)),
        None => Ok(()),
    }
}

fn record_ring(side: u32, scale: f32, radius: f32, inner: f32) -> AureaResult<Picture> {
    Picture::record(side, side, scale, |ctx| {
        let center = Point::new(radius, radius);
        let slice = TAU / f32::from(SLICES);
        for i in 0..SLICES {
            let from = f32::from(i) * slice;
            // A little past its neighbour, so no seam shows between them.
            let to = from + slice * 1.1;
            let mut path = Path::new();
            path.arc(center, radius, (from, to), false);
            path.arc(center, inner, (to, from), true);
            path.commands.push(PathCommand::Close);
            let hue = (f32::from(i) + 0.5) * 360.0 / f32::from(SLICES);
            let paint = Paint::new().color(Hsv::new(hue, 1.0, 1.0, 1.0).to_color());
            ctx.draw_path(&path, &paint)?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    const WHEEL: Rect = Rect::new(0.0, 0.0, 200.0, 200.0);

    fn show(rig: &mut Rig, value: &mut Hsv, art: &mut Option<Picture>) {
        rig.frame(|ui| {
            ColorWheel::new(value, art)
                .show(ui, 1, WHEEL)
                .expect("draw")
        });
    }

    fn click(rig: &mut Rig, value: &mut Hsv, art: &mut Option<Picture>, x: f32, y: f32) {
        rig.at(x, y);
        show(rig, value, art);
        rig.button(x, y, true);
        show(rig, value, art);
        rig.button(x, y, false);
        show(rig, value, art);
    }

    #[test]
    fn the_ring_sets_the_hue_by_where_it_is_pressed() {
        let (mut rig, mut art) = (Rig::new(200, 200), None);
        let mut value = Hsv::new(200.0, 0.5, 0.5, 1.0);
        show(&mut rig, &mut value, &mut art);
        click(&mut rig, &mut value, &mut art, 195.0, 100.0);
        assert!(
            value.h < 1.0 || value.h > 359.0,
            "right is red: {}",
            value.h
        );
        click(&mut rig, &mut value, &mut art, 100.0, 195.0);
        assert!((value.h - 90.0).abs() < 1.0, "down: {}", value.h);
        click(&mut rig, &mut value, &mut art, 5.0, 100.0);
        assert!((value.h - 180.0).abs() < 1.0, "left: {}", value.h);
        assert_eq!((value.s, value.v), (0.5, 0.5), "the square stays");
    }

    #[test]
    fn the_square_sets_saturation_and_value_and_the_gaps_set_nothing() {
        let (mut rig, mut art) = (Rig::new(200, 200), None);
        let start = Hsv::new(200.0, 0.5, 0.5, 1.0);
        let mut value = start;
        show(&mut rig, &mut value, &mut art);
        let square = Layout::of(WHEEL).square;
        click(
            &mut rig,
            &mut value,
            &mut art,
            square.right() - 1.0,
            square.y + 1.0,
        );
        assert!(value.s > 0.95 && value.v > 0.95, "{value:?}");
        assert_eq!(value.h, start.h);

        value = start;
        click(&mut rig, &mut value, &mut art, 2.0, 2.0);
        assert_eq!(value, start, "a corner outside the ring");
    }

    #[test]
    fn the_ring_is_drawn_once_until_its_size_changes() {
        let (mut rig, mut art) = (Rig::new(200, 200), None);
        let mut value = Hsv::new(0.0, 1.0, 1.0, 1.0);
        show(&mut rig, &mut value, &mut art);
        let first = art.as_ref().map(Picture::size);
        assert_eq!(first, Some((200, 200)));
        show(&mut rig, &mut value, &mut art);
        rig.frame(|ui| {
            ColorWheel::new(&mut value, &mut art)
                .show(ui, 1, Rect::new(0.0, 0.0, 100.0, 100.0))
                .expect("draw")
        });
        assert_eq!(art.as_ref().map(Picture::size), Some((100, 100)));
    }

    #[test]
    fn a_hue_comes_back_to_where_it_points() {
        let at = Layout::of(WHEEL);
        for hue in [0.0, 45.0, 123.0, 359.0] {
            assert!((at.hue_at(at.ring_point(hue)) - hue).abs() < 0.01);
        }
    }
}
