//! A progress bar: a [`Track`], eased, or sweeping when the amount is not known.

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::Track;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::Component;
use crate::ui::state::{Channel, WidgetId};
use crate::ui::theme::Theme;

/// How a progress bar looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressStyle {
    pub track: Color,
    pub filled: Color,
    /// How many times a second an unknown amount sweeps across.
    pub sweep: f32,
}

impl ProgressStyle {
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
pub struct ProgressBar {
    amount: Option<f32>,
    style: Option<ProgressStyle>,
}

impl ProgressBar {
    /// `amount` done, `0..=1`; `None` while it cannot be told.
    pub fn new(amount: Option<f32>) -> Self {
        Self {
            amount,
            style: None,
        }
    }

    pub fn style(mut self, style: ProgressStyle) -> Self {
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

impl<Id: WidgetId> Widget<Id> for ProgressBar {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| ProgressStyle::new(ui.theme));
        let track = Track::new(0.0, style.track, style.filled);
        let track = match self.amount {
            Some(amount) => {
                let rate = ui.theme.motion.slide;
                let shown = ui.animate(id, 0, Channel::Value, amount.clamp(0.0, 1.0), rate);
                track.span(0.0, shown)
            }
            None => {
                ui.keep_animating();
                let phase = (ui.time() * style.sweep).fract();
                let start = -1.0 / 3.0 + (4.0 / 3.0) * phase;
                track.span(start.max(0.0), (start + 1.0 / 3.0).min(1.0))
            }
        };
        track.paint(ui, rect)?;
        Ok(Response {
            rect,
            ..Response::default()
        })
    }
}
