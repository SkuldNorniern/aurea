//! A chip: a pill that turns on and off, for filters and tags.

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape, Surface, TextSize};
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a chip looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChipStyle {
    pub off: StateColors,
    pub on: Color,
    pub border: Color,
    pub on_border: Color,
    pub text: Color,
    pub on_text: Color,
    pub padding: f32,
    pub text_size: TextSize,
}

impl ChipStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            off: StateColors::new(p.raised, p.hover, p.pressed),
            on: p.accent.fade(0.2),
            border: p.border,
            on_border: p.accent,
            text: p.dim,
            on_text: p.text,
            padding: theme.padding,
            text_size: TextSize::Small,
        }
    }
}

/// A label in a pill that `on` turns on and off.
#[derive(Debug)]
pub struct Chip<'v> {
    label: &'v str,
    on: &'v mut bool,
    style: Option<ChipStyle>,
    enabled: bool,
}

impl<'v> Chip<'v> {
    pub fn new(label: &'v str, on: &'v mut bool) -> Self {
        Self {
            label,
            on,
            style: None,
            enabled: true,
        }
    }

    #[must_use]
    pub fn style(mut self, style: ChipStyle) -> Self {
        self.style = Some(style);
        self
    }

    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// How wide it wants to be at `height`, room for the tick included.
    pub fn width<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, height: f32) -> f32 {
        let style = self.style.unwrap_or_else(|| ChipStyle::new(ui.theme));
        let text = Label::new(self.label, style.text)
            .size(style.text_size)
            .width(ui);
        text + style.padding * 2.0 + height * 0.55
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

impl<Id: WidgetId> Widget<Id> for Chip<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let input = ui.interact(id, 0, rect, Sense::Click, self.enabled);
        let mut response = Response::new(rect, input);
        if input.clicked {
            *self.on = !*self.on;
            response.changed = true;
            ui.mark_changed();
        }
        let style = self.style.unwrap_or_else(|| ChipStyle::new(ui.theme));
        let state = ui.eased(id, 0, input);
        let rate = ui.theme.motion.slide * 1.5;
        let t = ui.animate(id, 0, Channel::Value, f32::from(u8::from(*self.on)), rate);
        FocusRing::new(ui.theme, rect.height / 2.0, state.focus).paint(ui, rect)?;
        let fill = style.off.at(state).mix(style.on.fade(state.enabled), t);
        let border = style.border.mix(style.on_border, t).fade(state.enabled);
        Surface::new(fill)
            .pill()
            .border(ui.theme.border_width, border)
            .paint(ui, rect)?;
        let tick = rect.height * 0.55 * t;
        let inner = rect.inset_xy(style.padding * 0.8, 0.0);
        if t > 0.01 {
            let at = Rect::new(inner.x, rect.y, tick, rect.height);
            Mark::new(MarkShape::Check, style.on_border.fade(state.enabled))
                .progress(t)
                .paint(ui, at.inset(rect.height * 0.12))?;
        }
        let text = Rect::new(
            inner.x + tick,
            rect.y,
            (inner.width - tick).max(0.0),
            rect.height,
        );
        let color = style.text.mix(style.on_text, t).fade(state.enabled);
        Label::new(self.label, color)
            .size(style.text_size)
            .centered()
            .paint(ui, text)?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::window::{KeyCode, Modifiers, WindowEvent};

    #[test]
    fn space_turns_a_focused_chip_on_and_off() {
        let theme = Theme::dark();
        let mut raster = CpuRasterizer::new(120, 40);
        let mut state = UiState::<u8>::new();
        let mut on = false;
        let mut width = 0.0;
        for key in [
            KeyCode::F1,
            KeyCode::Tab,
            KeyCode::Space,
            KeyCode::Space,
            KeyCode::Space,
        ] {
            state.handle(&WindowEvent::KeyInput {
                key,
                pressed: true,
                modifiers: Modifiers::default(),
            });
            let mut ctx = raster.begin_frame().expect("frame");
            {
                let mut ui = state.frame(&mut *ctx, &theme);
                let chip = Chip::new("Wired", &mut on);
                width = chip.width(&mut ui, 28.0);
                chip.show(&mut ui, 1, Rect::new(0.0, 0.0, width, 28.0))
                    .expect("draw");
            }
            drop(ctx);
            raster.end_frame().expect("end");
        }
        assert!(on, "on, off, on");
        assert!(width > 28.0);
    }
}
