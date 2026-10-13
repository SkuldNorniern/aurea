//! A disclosure: a header that opens and closes the section under it, its chevron turning from
//! right to down.

use std::f32::consts::FRAC_PI_2;

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Point, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Surface, TextSize};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a disclosure header looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisclosureStyle {
    pub face: StateColors,
    pub text: Color,
    pub chevron: Color,
    pub radius: f32,
    pub text_size: TextSize,
}

impl DisclosureStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            face: StateColors::new(p.hover.with_alpha(0), p.hover, p.pressed),
            text: p.text,
            chevron: p.dim,
            radius: theme.radius.min(6.0),
            text_size: TextSize::Strong,
        }
    }
}

/// A header titled `title`; clicking it, or Enter or Space, flips `open`.
#[derive(Debug)]
pub struct Disclosure<'v> {
    title: &'v str,
    open: &'v mut bool,
    style: Option<DisclosureStyle>,
}

impl<'v> Disclosure<'v> {
    pub fn new(title: &'v str, open: &'v mut bool) -> Self {
        Self {
            title,
            open,
            style: None,
        }
    }

    #[must_use]
    pub fn style(mut self, style: DisclosureStyle) -> Self {
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

    /// How far open `id`'s section is this frame, `0..=1`, eased.
    pub fn openness<Id: WidgetId>(ui: &Ui<'_, Id>, id: Id) -> f32 {
        ui.state
            .tweens
            .get((Key::new(id, 0), Channel::Value))
            .unwrap_or(0.0)
    }
}

impl<Id: WidgetId> Widget<Id> for Disclosure<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let input = ui.interact(id, 0, rect, Sense::Click, true);
        let mut response = Response::new(rect, input);
        if input.clicked {
            *self.open = !*self.open;
            response.changed = true;
            ui.mark_changed();
        }
        let style = self.style.unwrap_or_else(|| DisclosureStyle::new(ui.theme));
        let state = ui.eased(id, 0, input);
        let t = ui.animate(
            id,
            0,
            Channel::Value,
            f32::from(u8::from(*self.open)),
            ui.theme.motion.slide,
        );
        Surface::new(style.face.at(state))
            .radius(style.radius)
            .paint(ui, rect)?;
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
        let (mark, text) = rect.split_left(rect.height, 0.0);
        chevron(ui, mark.inset(rect.height * 0.32), t, style.chevron)?;
        Label::new(self.title, style.text)
            .size(style.text_size)
            .paint(ui, text)?;
        Ok(response)
    }
}

fn chevron<Id: WidgetId>(ui: &mut Ui<'_, Id>, at: Rect, t: f32, color: Color) -> AureaResult<()> {
    let c = at.center();
    let s = at.width.min(at.height) / 2.0;
    let (sin, cos) = (t * FRAC_PI_2).sin_cos();
    let turn =
        |x: f32, y: f32| Point::new(c.x + (x * cos - y * sin) * s, c.y + (x * sin + y * cos) * s);
    let points = [turn(-0.3, -0.6), turn(0.3, 0.0), turn(-0.3, 0.6)];
    ui.polyline(&points, (s * 0.28).max(1.2), color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::window::{Modifiers, MouseButton, WindowEvent};

    #[test]
    fn a_click_opens_it_and_the_section_follows() {
        let theme = Theme::dark();
        let mut raster = CpuRasterizer::new(200, 40);
        let mut state = UiState::<u8>::new();
        state.set_reduced_motion(true);
        let mut open = false;
        let mut shown = 0.0;
        for event in [None, Some(true), Some(false)] {
            if let Some(pressed) = event {
                state.handle(&WindowEvent::MouseButton {
                    button: MouseButton::Left,
                    pressed,
                    modifiers: Modifiers::default(),
                    x: 50.0,
                    y: 15.0,
                    click_count: 1,
                    time: Default::default(),
                });
            }
            let mut ctx = raster.begin_frame().expect("frame");
            {
                let mut ui = state.frame(&mut *ctx, &theme);
                Disclosure::new("Advanced", &mut open)
                    .show(&mut ui, 1, Rect::new(0.0, 0.0, 200.0, 30.0))
                    .expect("draw");
                shown = Disclosure::openness(&ui, 1);
            }
            drop(ctx);
            raster.end_frame().expect("end");
        }
        assert!(open);
        assert_eq!(shown, 1.0, "no motion asked for, so open at once");
    }
}
