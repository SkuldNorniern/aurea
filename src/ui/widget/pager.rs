//! A pager: a dot per page in a row, the current one drawn long.

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Surface};
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::{CursorIcon, KeyCode};

/// How a pager looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PagerStyle {
    pub dot: Color,
    pub hover: Color,
    pub current: Color,
    pub size: f32,
    /// How long the current page's dot is, in dots.
    pub stretch: f32,
    pub gap: f32,
}

impl PagerStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            dot: p.faint.fade(0.6),
            hover: p.dim,
            current: p.accent,
            size: 7.0,
            stretch: 2.6,
            gap: 7.0,
        }
    }
}

/// `count` pages, `page` the one showing.
#[derive(Debug)]
pub struct Pager<'v> {
    count: usize,
    page: &'v mut usize,
    style: Option<PagerStyle>,
}

impl<'v> Pager<'v> {
    pub fn new(count: usize, page: &'v mut usize) -> Self {
        Self {
            count,
            page,
            style: None,
        }
    }

    #[must_use]
    pub fn style(mut self, style: PagerStyle) -> Self {
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

impl<Id: WidgetId> Widget<Id> for Pager<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| PagerStyle::new(ui.theme));
        let input = ui.interact(id, 0, rect, Sense::Adjust, self.count > 1);
        let mut response = Response::new(rect, input);
        if self.count == 0 {
            return Ok(response);
        }
        let last = self.count - 1;
        let mut to = (*self.page).min(last);
        if input.focused {
            for (key, _) in ui.keys() {
                match key {
                    KeyCode::Right => to = (to + 1).min(last),
                    KeyCode::Left => to = to.saturating_sub(1),
                    KeyCode::Home => to = 0,
                    KeyCode::End => to = last,
                    _ => {}
                }
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let total = style.size * (style.stretch + last as f32) + style.gap * last as f32;
        let mut x = rect.center().x - total / 2.0;
        let y = rect.center().y - style.size / 2.0;
        let rate = ui.theme.motion.slide;
        for i in 0..self.count {
            let part = 1 + u16::try_from(i).unwrap_or(u16::MAX - 1);
            let on = ui.animate(
                id,
                part,
                Channel::Value,
                f32::from(u8::from(i == *self.page)),
                rate,
            );
            let w = style.size * (1.0 + (style.stretch - 1.0) * on);
            let dot = Rect::new(x, y, w, style.size);
            x += w + style.gap;
            let reach = dot.inset(-style.gap / 2.0);
            let hit = ui.interact(id, part, reach, Sense::Hover, true);
            if hit.hovered {
                ui.set_cursor(CursorIcon::Pointer);
            }
            if ui.press(id, part).is_some() {
                to = i;
            }
            let rest = if hit.hovered { style.hover } else { style.dot };
            Surface::new(rest.mix(style.current, on))
                .pill()
                .paint(ui, dot)?;
        }
        let state = ui.eased(id, 0, input);
        FocusRing::new(ui.theme, rect.height / 2.0, state.focus).paint(
            ui,
            Rect::new(
                rect.center().x - total / 2.0 - 6.0,
                y - 6.0,
                total + 12.0,
                style.size + 12.0,
            ),
        )?;
        if to != *self.page {
            *self.page = to;
            response.changed = true;
            ui.mark_changed();
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::ui::state::Key;
    use crate::window::{Modifiers, MouseButton, WindowEvent};

    #[test]
    fn a_dot_click_goes_there_and_the_arrows_step() {
        let theme = Theme::dark();
        let mut raster = CpuRasterizer::new(200, 30);
        let mut state = UiState::<u8>::new();
        state.set_reduced_motion(true);
        let mut page = 0;
        let mut frame = |state: &mut UiState<u8>, page: &mut usize| {
            let mut ctx = raster.begin_frame().expect("frame");
            {
                let mut ui = state.frame(&mut *ctx, &theme);
                Pager::new(4, page)
                    .show(&mut ui, 1, Rect::new(0.0, 0.0, 200.0, 30.0))
                    .expect("draw");
            }
            drop(ctx);
            raster.end_frame().expect("end");
        };
        frame(&mut state, &mut page);
        state.handle(&WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
            modifiers: Modifiers::default(),
            x: 128.0,
            y: 15.0,
            click_count: 1,
            time: Default::default(),
        });
        frame(&mut state, &mut page);
        assert_eq!(page, 3);
        state.give_focus(Some(Key::new(1, 0)));
        state.handle(&WindowEvent::KeyInput {
            key: KeyCode::Left,
            pressed: true,
            modifiers: Modifiers::default(),
        });
        frame(&mut state, &mut page);
        assert_eq!(page, 2);
    }
}
