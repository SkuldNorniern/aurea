//! A progress bar: a [`Track`], eased, or sweeping when the amount is not known.

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::accessibility::{Role, set_numeric};
use crate::ui::component::Track;
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::Component;
use crate::ui::state::{Channel, WidgetId};
use crate::ui::theme::Theme;

/// How a progress bar looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressStyle {
    /// Colour of the empty track.
    pub track: Color,
    /// Colour of the filled part.
    pub filled: Color,
    /// How many times a second an unknown amount sweeps across.
    pub sweep: f32,
}

impl ProgressStyle {
    /// Creates the default style from `theme`.
    pub fn new(theme: &Theme) -> Self {
        Self {
            track: theme.palette.raised,
            filled: theme.palette.accent,
            sweep: 0.8,
        }
    }
}

/// A bar filling as work goes on.
#[derive(Clone, Copy, Debug)]
pub struct ProgressBar<'v> {
    amount: Option<f32>,
    style: Option<ProgressStyle>,
    label: Option<&'v str>,
}

impl<'v> ProgressBar<'v> {
    /// `amount` done, `0..=1`; `None` while it cannot be told.
    pub fn new(amount: Option<f32>) -> Self {
        Self {
            amount,
            style: None,
            label: None,
        }
    }

    /// What screen readers call the bar, such as `Download`. It is not drawn.
    pub fn label(mut self, label: &'v str) -> Self {
        self.label = Some(label);
        self
    }

    /// Sets the style.
    pub fn style(mut self, style: ProgressStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Shows the bar as `id` at `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

/// What a [`ProgressBar`] draws, from [`ProgressBar::look`].
#[derive(Clone, Copy, Debug)]
pub struct ProgressParts {
    /// The amount done, clamped to `0..=1`; `None` while it cannot be told.
    pub amount: Option<f32>,
    /// The filled part as shares of the length: eased towards the amount, or sweeping when unknown.
    pub span: (f32, f32),
    /// Where the sweep is, `0..1`, while the amount is unknown.
    pub phase: Option<f32>,
    /// The style it draws with, the theme's unless one was given.
    pub style: ProgressStyle,
}

impl ProgressBar<'_> {
    /// Works out what the bar would draw, without drawing: the app draws it, or
    /// [`Look::paint_default`] does. It takes no input.
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<ProgressParts> {
        let style = self.style.unwrap_or_else(|| ProgressStyle::new(ui.theme));
        {
            ui.accessible_region(id, 0, rect);
            let (amount, label) = (self.amount, self.label);
            ui.with_accessible(id, 0, |node| {
                node.set_role(Role::ProgressIndicator);
                if let Some(label) = label {
                    node.set_label(label);
                }
                if let Some(amount) = amount {
                    set_numeric(node, f64::from(amount.clamp(0.0, 1.0)), (0.0, 1.0));
                }
            });
        }
        let (span, phase) = match self.amount {
            Some(amount) => {
                let rate = ui.theme.motion.slide;
                let shown = ui.animate(id, 0, Channel::Value, amount.clamp(0.0, 1.0), rate);
                ((0.0, shown), None)
            }
            None => {
                ui.keep_animating();
                let phase = (ui.time() * style.sweep).fract();
                let start = -1.0 / 3.0 + (4.0 / 3.0) * phase;
                ((start.max(0.0), (start + 1.0 / 3.0).min(1.0)), Some(phase))
            }
        };
        Look {
            rect,
            // no input, so never hovered or pressed
            state: Eased {
                enabled: 1.0,
                ..Eased::default()
            },
            held_at: None,
            response: Response {
                rect,
                ..Response::default()
            },
            parts: ProgressParts {
                amount: self.amount.map(|amount| amount.clamp(0.0, 1.0)),
                span,
                phase,
                style,
            },
        }
    }
}

impl Look<ProgressParts> {
    /// The bar's own look: the track with the filled part across it.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_track(ui)
    }

    /// The empty track with the filled part across it.
    pub fn paint_track<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let style = self.parts.style;
        let (from, to) = self.parts.span;
        Track::new(0.0, style.track, style.filled)
            .span(from, to)
            .paint(ui, self.rect)
    }
}

impl<Id: WidgetId> Widget<Id> for ProgressBar<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let look = self.look(ui, id, rect);
        look.paint_default(ui)?;
        Ok(look.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    #[test]
    fn look_draws_the_apps_look_and_fills_to_the_amount() {
        let mut rig = Rig::new(120, 20).still();
        let at = Rect::new(0.0, 0.0, 120.0, 20.0);
        let mut seen = None;
        rig.frame(|ui| {
            let bar = ProgressBar::new(Some(0.5)).look(ui, 1, at);
            seen = Some(bar.parts.span);
            ui.fill(bar.rect, Color::rgb(0, 200, 0)).expect("draw");
        });
        assert_eq!(
            rig.pixel(60, 10),
            [0, 200, 0, 255],
            "the app's look, not the default"
        );
        assert_eq!(seen, Some((0.0, 0.5)), "filled to the amount, at once");
    }
}
