//! The middle layer: small pieces that follow input, built from [`paint`](super::paint) and used by
//! the widgets.

pub(crate) mod editable;
mod pan_zoom;
mod plot_view;

pub use editable::{EditableText, TextOutcome};
pub use pan_zoom::PanZoom;
pub use plot_view::PlotView;

use super::frame::{Eased, Ui};
use super::paint::{Component, Shadow, Surface};
use super::state::WidgetId;
use super::theme::Theme;
use crate::AureaResult;
use crate::render::{Color, Rect};

/// A colour for each of rest, hover and press, blended by how far along each is, and faded when
/// disabled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateColors {
    pub rest: Color,
    pub hover: Color,
    pub pressed: Color,
}

impl StateColors {
    pub const fn new(rest: Color, hover: Color, pressed: Color) -> Self {
        Self {
            rest,
            hover,
            pressed,
        }
    }

    /// The same colour whatever the state.
    pub const fn same(color: Color) -> Self {
        Self::new(color, color, color)
    }

    /// The colour for `state`.
    pub fn at(&self, state: Eased) -> Color {
        self.rest
            .mix(self.hover, state.hover)
            .mix(self.pressed, state.press)
            .fade(state.enabled)
    }
}

/// The ring around what has keyboard focus, `amount` of the way in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusRing {
    pub color: Color,
    pub width: f32,
    /// Space between the ring and what it surrounds.
    pub gap: f32,
    /// The corner radius of what it surrounds; the ring's follows it.
    pub radius: f32,
    pub amount: f32,
}

impl FocusRing {
    pub fn new(theme: &Theme, radius: f32, amount: f32) -> Self {
        Self {
            color: theme.palette.focus,
            width: theme.focus_width,
            gap: 1.0,
            radius,
            amount,
        }
    }
}

impl Component for FocusRing {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        if self.amount <= 0.0 {
            return Ok(());
        }
        let out = self.gap + self.width;
        let radius = self.radius.min(rect.width / 2.0).min(rect.height / 2.0) + out;
        ui.border(
            rect.inset(-out),
            radius,
            self.width,
            self.color.fade(self.amount),
        )
    }
}

/// A rounded bar across the middle of its rect, filled over part of its length: a slider's groove,
/// a progress bar, a level meter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    /// Where the filled part starts and ends, as shares of the length.
    pub from: f32,
    pub to: f32,
    /// Height of the bar; `None` fills the rect.
    pub thickness: Option<f32>,
    pub empty: Color,
    pub filled: Color,
}

impl Track {
    /// Filled from the start to `value` of the way.
    pub fn new(value: f32, empty: Color, filled: Color) -> Self {
        Self {
            from: 0.0,
            to: value,
            thickness: None,
            empty,
            filled,
        }
    }

    /// Filled between `from` and `to` of the way.
    pub fn span(mut self, from: f32, to: f32) -> Self {
        self.from = from;
        self.to = to;
        self
    }

    pub fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = Some(thickness);
        self
    }

    /// The bar's own rect inside `rect`.
    pub fn bar(&self, rect: Rect) -> Rect {
        let h = self.thickness.unwrap_or(rect.height).min(rect.height);
        rect.centered(rect.width, h)
    }
}

impl Component for Track {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        let bar = self.bar(rect);
        let r = bar.height / 2.0;
        ui.round(bar, r, self.empty)?;
        let (a, b) = (self.from.clamp(0.0, 1.0), self.to.clamp(0.0, 1.0));
        if b > a {
            let filled = Rect::new(
                bar.x + bar.width * a,
                bar.y,
                bar.width * (b - a),
                bar.height,
            );
            ui.round(filled, r, self.filled)?;
        }
        Ok(())
    }
}

/// A round handle centred in its rect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knob {
    pub radius: f32,
    pub color: Color,
    pub shadow: Option<Shadow>,
    /// A dot in the middle: its share of the radius, and its colour.
    pub dot: Option<(f32, Color)>,
}

impl Knob {
    pub const fn new(radius: f32, color: Color) -> Self {
        Self {
            radius,
            color,
            shadow: None,
            dot: None,
        }
    }

    pub const fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    pub const fn dot(mut self, share: f32, color: Color) -> Self {
        self.dot = Some((share, color));
        self
    }
}

impl Component for Knob {
    fn paint<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, rect: Rect) -> AureaResult<()> {
        let c = rect.center();
        if let Some(s) = self.shadow {
            let r = self.radius + s.grow;
            let at = Rect::new(c.x + s.dx - r, c.y + s.dy - r, r * 2.0, r * 2.0);
            ui.shadow(at, r, s.blur, s.color)?;
        }
        ui.circle(c, self.radius, self.color)?;
        match self.dot {
            Some((share, color)) => ui.circle(c, self.radius * share, color),
            None => Ok(()),
        }
    }
}

/// The surface tooltips, menus and popovers sit on: raised, outlined and lifted by a shadow.
pub fn card(theme: &Theme) -> Surface {
    let p = theme.palette;
    Surface::new(p.raised.mix(p.hover, 0.5))
        .radius(theme.radius.min(6.0))
        .border(theme.border_width, p.border)
        .shadow(Shadow::new(p.shadow).offset(0.0, 2.0).blur(8.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_colors_blend_and_fade() {
        let c = StateColors::new(
            Color::rgb(0, 0, 0),
            Color::rgb(100, 0, 0),
            Color::rgb(0, 200, 0),
        );
        let rest = Eased {
            enabled: 1.0,
            ..Eased::default()
        };
        assert_eq!(c.at(rest), Color::rgb(0, 0, 0));
        let hovered = Eased { hover: 1.0, ..rest };
        assert_eq!(c.at(hovered), Color::rgb(100, 0, 0));
        let off = Eased {
            enabled: 0.5,
            ..rest
        };
        assert_eq!(c.at(off).a, 128);
    }

    #[test]
    fn a_track_bar_is_centred_and_kept_inside() {
        let t = Track::new(0.5, Color::rgb(0, 0, 0), Color::rgb(1, 1, 1)).thickness(4.0);
        assert_eq!(
            t.bar(Rect::new(0.0, 0.0, 100.0, 20.0)),
            Rect::new(0.0, 8.0, 100.0, 4.0)
        );
        assert_eq!(t.bar(Rect::new(0.0, 0.0, 100.0, 2.0)).height, 2.0);
    }
}
