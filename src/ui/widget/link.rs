//! A link: text in the accent colour, underlined while the pointer is on it, that is clicked like a
//! button.

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::accessibility::Role;
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, TextSize};
use crate::ui::state::{Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a link looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkStyle {
    /// Text colour.
    pub text: Color,
    /// Text colour under the pointer.
    pub hover: Color,
    /// Text size.
    pub text_size: TextSize,
}

impl LinkStyle {
    /// Default style from the theme.
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
    /// Creates a link with `text`.
    pub fn new(text: &'t str) -> Self {
        Self { text, style: None }
    }

    /// Sets the look; defaults to the theme's.
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

    /// Draws the link in `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

/// What a [`Link`] draws, from [`Link::look`].
#[derive(Clone, Copy, Debug)]
pub struct LinkParts<'t> {
    /// The text.
    pub label: &'t str,
    /// The style it draws with, the theme's unless one was given.
    pub style: LinkStyle,
}

impl<'t> Link<'t> {
    /// Takes the input as [`Link::show`] does and works out what it would draw, without drawing:
    /// the app draws it, or [`Look::paint_default`] does, or its pieces [`Look::paint_ring`],
    /// [`Look::paint_label`] and [`Look::paint_underline`].
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<LinkParts<'t>> {
        let style = self.style.unwrap_or_else(|| LinkStyle::new(ui.theme));
        let width = self.width(ui).min(rect.width);
        let text = Rect::new(rect.x, rect.y, width, rect.height);
        let input = ui.interact(id, 0, text, Sense::Click, true);
        ui.accessible_description(id, 0, Role::Link, self.text, None);
        Look {
            rect: text,
            state: ui.eased(id, 0, input),
            held_at: ui.held_at(id, 0),
            response: Response::new(text, input),
            parts: LinkParts {
                label: self.text,
                style,
            },
        }
    }
}

impl Look<LinkParts<'_>> {
    /// The link's own look: the focus ring, the text, then the underline under the pointer.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_ring(ui)?;
        self.paint_label(ui)?;
        self.paint_underline(ui)
    }

    /// The text colour: the accent, mixed towards the hover colour as the pointer comes on.
    pub fn ink(&self) -> Color {
        self.parts
            .style
            .text
            .mix(self.parts.style.hover, self.state.hover)
    }

    /// The focus ring around the text.
    pub fn paint_ring<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        FocusRing::new(ui.theme, 3.0, self.state.focus).paint(ui, self.rect.inset(-2.0))
    }

    /// The text, in [`Look::ink`].
    pub fn paint_label<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        Label::new(self.parts.label, self.ink())
            .size(self.parts.style.text_size)
            .paint(ui, self.rect)
    }

    /// The line under the text, which fades in as the pointer comes on.
    pub fn paint_underline<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        if self.state.hover > 0.01 {
            let text = self.rect;
            let size = ui.fonts().of(self.parts.style.text_size).size;
            let y = text.center().y + size * 0.55;
            ui.fill(
                Rect::new(text.x, y, text.width, 1.0),
                self.ink().fade(self.state.hover),
            )?;
        }
        Ok(())
    }
}

impl<Id: WidgetId> Widget<Id> for Link<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let look = self.look(ui, id, rect);
        look.paint_default(ui)?;
        Ok(look.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::ui::rig::Rig;
    use crate::window::{Modifiers, MouseButton, PointerKind, WindowEvent};

    #[test]
    fn look_draws_the_apps_look_and_still_clicks() {
        let mut rig = Rig::new(300, 30);
        let at = Rect::new(0.0, 0.0, 300.0, 30.0);
        let draw = |rig: &mut Rig| {
            rig.frame(|ui| {
                let link = Link::new("Docs").look(ui, 1, at);
                ui.fill(link.rect, Color::rgb(0, 200, 0)).expect("draw");
                link.response.clicked
            })
        };
        draw(&mut rig);
        assert_eq!(rig.pixel(5, 15), [0, 200, 0, 255], "the app's look");
        rig.at(10, 15);
        draw(&mut rig);
        rig.button(10, 15, true);
        assert!(!draw(&mut rig), "the press alone");
        rig.button(10, 15, false);
        assert!(draw(&mut rig), "the release clicks");
    }

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
                    pointer: PointerKind::Mouse,
                });
                frame(&mut state, &mut clicks);
            }
        }
        assert_eq!(clicks, 1, "past the text is not the link");
    }
}
