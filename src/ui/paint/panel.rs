//! A shaped surface with the layers glass and material looks are made of.

use super::{Component, Shadow};
use crate::AureaResult;
use crate::render::{Color, Corners, GradientStop, LinearGradient, Paint, Path, Point, Rect};
use crate::ui::frame::Ui;
use crate::ui::state::WidgetId;

/// Most shadows a panel stacks, such as a tight one and a wide soft one.
const SHADOWS: usize = 3;

/// How a panel's face is filled.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fill {
    Solid(Color),
    /// From the first colour at the top to the second at the bottom.
    Vertical(Color, Color),
    /// From the first colour at the left to the second at the right.
    Horizontal(Color, Color),
}

/// A surface with corners of their own, a solid or gradient face, stacked shadows, a sheen down
/// from the top, a glow in from the edges and a rim. Each layer is optional; paint it with
/// [`Component::paint`].
///
/// ```rust,no_run
/// # use aurea::ui::{Component, Panel, Shadow, Ui};
/// # use aurea::render::{Color, Corners};
/// # fn f(ui: &mut Ui<'_, u8>) -> aurea::AureaResult<()> {
/// let white = |a: f32| Color::rgb(255, 255, 255).fade(a);
/// Panel::new()
///     .corners(Corners::all(24.0).smooth(0.6))
///     .fill_vertical(white(0.3), white(0.1))
///     .shadow(Shadow::new(Color::rgba(0, 0, 0, 70)).offset(0.0, 8.0).blur(18.0))
///     .sheen(white(0.25), 0.45)
///     .rim_vertical(1.0, white(0.6), white(0.15))
///     .paint(ui, aurea::Rect::new(20.0, 20.0, 300.0, 200.0))
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panel {
    fill: Option<Fill>,
    corners: Corners,
    shadows: [Option<Shadow>; SHADOWS],
    sheen: Option<(Color, f32)>,
    glow: Option<(f32, Color)>,
    rim: Option<(f32, Fill)>,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel {
    /// A panel with no layers: square corners, nothing drawn.
    pub const fn new() -> Self {
        Self {
            fill: None,
            corners: Corners::all(0.0),
            shadows: [None; SHADOWS],
            sheen: None,
            glow: None,
            rim: None,
        }
    }

    /// Every corner `radius`, circular.
    #[must_use]
    pub fn radius(mut self, radius: f32) -> Self {
        self.corners = Corners::all(radius);
        self
    }

    /// Corners of their own, smoothed or not.
    #[must_use]
    pub fn corners(mut self, corners: Corners) -> Self {
        self.corners = corners;
        self
    }

    /// A face of one colour.
    #[must_use]
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(Fill::Solid(color));
        self
    }

    /// A face from `top` down to `bottom`.
    #[must_use]
    pub fn fill_vertical(mut self, top: Color, bottom: Color) -> Self {
        self.fill = Some(Fill::Vertical(top, bottom));
        self
    }

    /// A face from `left` across to `right`.
    #[must_use]
    pub fn fill_horizontal(mut self, left: Color, right: Color) -> Self {
        self.fill = Some(Fill::Horizontal(left, right));
        self
    }

    /// Another shadow under it, drawn after those added before; past three, the last is replaced.
    #[must_use]
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        let slot = self
            .shadows
            .iter()
            .position(Option::is_none)
            .unwrap_or(SHADOWS - 1);
        self.shadows[slot] = Some(shadow);
        self
    }

    /// A highlight `color` at the top fading out `depth` of the way down, as light on glass.
    #[must_use]
    pub fn sheen(mut self, color: Color, depth: f32) -> Self {
        self.sheen = Some((color, depth.clamp(0.0, 1.0)));
        self
    }

    /// `color` glowing in from every edge, fading out `width` inside.
    #[must_use]
    pub fn glow(mut self, width: f32, color: Color) -> Self {
        self.glow = Some((width.max(0.0), color));
        self
    }

    /// An edge `width` wide inside the outline, in one colour.
    #[must_use]
    pub fn rim(mut self, width: f32, color: Color) -> Self {
        self.rim = Some((width.max(0.0), Fill::Solid(color)));
        self
    }

    /// An edge `width` wide inside the outline, `top` along the top fading to `bottom`.
    #[must_use]
    pub fn rim_vertical(mut self, width: f32, top: Color, bottom: Color) -> Self {
        self.rim = Some((width.max(0.0), Fill::Vertical(top, bottom)));
        self
    }
}

fn stop(offset: f32, color: Color) -> GradientStop {
    GradientStop { offset, color }
}

fn gradient(rect: Rect, fill: Fill) -> Result<LinearGradient, Color> {
    let (start, end, a, b) = match fill {
        Fill::Solid(color) => return Err(color),
        Fill::Vertical(a, b) => (
            Point::new(rect.x, rect.y),
            Point::new(rect.x, rect.bottom()),
            a,
            b,
        ),
        Fill::Horizontal(a, b) => (
            Point::new(rect.x, rect.y),
            Point::new(rect.right(), rect.y),
            a,
            b,
        ),
    };
    Ok(LinearGradient {
        start,
        end,
        stops: vec![stop(0.0, a), stop(1.0, b)],
    })
}

/// Fills `path` with `fill`, a gradient spanning `rect`.
fn fill_path<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    path: &Path,
    rect: Rect,
    fill: Fill,
) -> AureaResult<()> {
    match gradient(rect, fill) {
        Ok(gradient) => ui.ctx.fill_path_linear_gradient(path, &gradient),
        Err(color) if color.a > 0 => ui.ctx.draw_path(path, &Paint::new().color(color)),
        Err(_) => Ok(()),
    }
}

impl Component for Panel {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return Ok(());
        }
        let c = self.corners;
        let radius = c
            .top_left
            .max(c.top_right)
            .max(c.bottom_right)
            .max(c.bottom_left);
        let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0);
        for s in self.shadows.iter().flatten() {
            let under = Rect::new(rect.x + s.dx, rect.y + s.dy, rect.width, rect.height);
            ui.shadow(under.inset(-s.grow), radius + s.grow, s.blur, s.color)?;
        }
        let shape = Path::rounded(rect, c);
        if let Some(fill) = self.fill {
            fill_path(ui, &shape, rect, fill)?;
        }
        if let Some((color, depth)) = self.sheen {
            let band = Rect::new(rect.x, rect.y, rect.width, rect.height * depth);
            fill_path(ui, &shape, band, Fill::Vertical(color, color.with_alpha(0)))?;
        }
        if let Some((width, color)) = self.glow {
            self.paint_glow(ui, &shape, rect, width, color)?;
        }
        if let Some((width, fill)) = self.rim {
            let ring = Path::rounded_ring(rect, c, width);
            fill_path(ui, &ring, rect, fill)?;
        }
        Ok(())
    }
}

impl Panel {
    /// Four bands, one in from each edge, clipped to the shape.
    fn paint_glow<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        shape: &Path,
        rect: Rect,
        width: f32,
        color: Color,
    ) -> AureaResult<()> {
        let width = width.min(rect.width / 2.0).min(rect.height / 2.0);
        if width <= 0.0 || color.a == 0 {
            return Ok(());
        }
        let clear = color.with_alpha(0);
        let bands = [
            (
                Rect::new(rect.x, rect.y, rect.width, width),
                Fill::Vertical(color, clear),
            ),
            (
                Rect::new(rect.x, rect.bottom() - width, rect.width, width),
                Fill::Vertical(clear, color),
            ),
            (
                Rect::new(rect.x, rect.y, width, rect.height),
                Fill::Horizontal(color, clear),
            ),
            (
                Rect::new(rect.right() - width, rect.y, width, rect.height),
                Fill::Horizontal(clear, color),
            ),
        ];
        ui.ctx.save()?;
        let painted = ui.ctx.clip_path(shape).and_then(|()| {
            bands
                .into_iter()
                .try_for_each(|(band, fill)| match gradient(band, fill) {
                    Ok(gradient) => ui.ctx.fill_linear_gradient(&gradient, band),
                    Err(_) => Ok(()),
                })
        });
        ui.ctx.restore()?;
        painted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    fn paint(panel: Panel) -> Rig {
        let mut rig: Rig = Rig::new(100, 60);
        rig.frame(|ui| {
            ui.fill(Rect::new(0.0, 0.0, 100.0, 60.0), Color::rgb(0, 0, 0))
                .expect("fill");
            panel
                .paint(ui, Rect::new(10.0, 10.0, 80.0, 40.0))
                .expect("panel");
        });
        rig
    }

    #[test]
    fn an_empty_panel_draws_nothing() {
        let rig = paint(Panel::new().radius(10.0));
        assert_eq!(rig.pixel(50, 30), [0, 0, 0, 255]);
    }

    #[test]
    fn a_vertical_fill_runs_top_to_bottom_inside_the_corners() {
        let rig = paint(
            Panel::new()
                .corners(Corners::all(12.0).smooth(0.6))
                .fill_vertical(Color::rgb(255, 0, 0), Color::rgb(0, 0, 255)),
        );
        let (top, bottom) = (rig.pixel(50, 12), rig.pixel(50, 48));
        assert!(top[2] > 200, "red at the top: {top:?}");
        assert!(bottom[0] > 200, "blue at the bottom: {bottom:?}");
        assert_eq!(
            rig.pixel(11, 11),
            [0, 0, 0, 255],
            "outside the rounded corner"
        );
    }

    #[test]
    fn the_rim_is_drawn_only_near_the_edge() {
        let rig = paint(Panel::new().radius(8.0).rim(2.0, Color::rgb(0, 255, 0)));
        assert!(
            rig.pixel(50, 10)[1] > 200,
            "on the rim: {:?}",
            rig.pixel(50, 10)
        );
        assert_eq!(rig.pixel(50, 30), [0, 0, 0, 255], "the middle is open");
    }

    #[test]
    fn the_glow_fades_in_from_the_edges() {
        let rig = paint(Panel::new().glow(10.0, Color::rgb(255, 255, 255)));
        let (edge, middle) = (rig.pixel(50, 11)[1], rig.pixel(50, 30)[1]);
        assert!(edge > 150 && middle < 10, "edge {edge}, middle {middle}");
    }
}
