//! A link: text in the accent colour, underlined while the pointer is on it, that is clicked like a
//! button.

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, TextSize};
use crate::ui::state::{Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a link looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkStyle {
    pub text: Color,
    pub hover: Color,
    pub text_size: TextSize,
}

impl LinkStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            text: p.accent,
            hover: p.accent.mix(p.text, 0.25),
            text_size: TextSize::Body,
        }
    }
}

/// `text` that can be clicked, drawn from the rect's left.
#[derive(Debug)]
pub struct Link<'t> {
    text: &'t str,
    style: Option<LinkStyle>,
}

impl<'t> Link<'t> {
    pub fn new(text: &'t str) -> Self {
        Self { text, style: None }
    }

    #[must_use]
    pub fn style(mut self, style: LinkStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// The width of its text, to size its rect to.
    pub fn width<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> f32 {
        let style = self.style.unwrap_or_else(|| LinkStyle::new(ui.theme));
        Label::new(self.text, style.text)
            .size(style.text_size)
            .width(ui)
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

impl<Id: WidgetId> Widget<Id> for Link<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| LinkStyle::new(ui.theme));
        let width = self.width(ui).min(rect.width);
        let text = Rect::new(rect.x, rect.y, width, rect.height);
        let input = ui.interact(id, 0, text, Sense::Click, true);
        let state = ui.eased(id, 0, input);
        FocusRing::new(ui.theme, 3.0, state.focus).paint(ui, text.inset(-2.0))?;
        let color = style.text.mix(style.hover, state.hover);
        Label::new(self.text, color)
            .size(style.text_size)
            .paint(ui, text)?;
        if state.hover > 0.01 {
            let size = ui.fonts().of(style.text_size).size;
            let y = text.center().y + size * 0.55;
            ui.fill(Rect::new(text.x, y, width, 1.0), color.fade(state.hover))?;
        }
        Ok(Response::new(text, input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::window::{Modifiers, MouseButton, WindowEvent};

    #[test]
    fn only_the_text_takes_the_click() {
        let theme = Theme::dark();
        let mut raster = CpuRasterizer::new(300, 30);
        let mut state = UiState::<u8>::new();
        let mut clicks = 0;
        let mut frame = |state: &mut UiState<u8>, clicks: &mut u32| {
            let mut ctx = raster.begin_frame().expect("frame");
            {
                let mut ui = state.frame(&mut *ctx, &theme);
                let r = Link::new("Docs")
                    .show(&mut ui, 1, Rect::new(0.0, 0.0, 300.0, 30.0))
                    .expect("draw");
                *clicks += u32::from(r.clicked);
            }
            drop(ctx);
            raster.end_frame().expect("end");
        };
        frame(&mut state, &mut clicks);
        for x in [10.0, 250.0] {
            for pressed in [true, false] {
                state.handle(&WindowEvent::MouseButton {
                    button: MouseButton::Left,
                    pressed,
                    modifiers: Modifiers::default(),
                    x,
                    y: 15.0,
                    click_count: 1,
                    time: Default::default(),
                });
                frame(&mut state, &mut clicks);
            }
        }
        assert_eq!(clicks, 1, "past the text is not the link");
    }
}
