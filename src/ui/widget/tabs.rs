//! A tab bar: labels in a row, each sized to its text, and a line under the picked one that slides
//! to the next.

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Surface, TextSize};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

const LINE_X: u16 = u16::MAX;
const LINE_W: u16 = u16::MAX - 1;

/// How a tab bar looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabStyle {
    pub tab: StateColors,
    pub text: Color,
    pub picked_text: Color,
    pub line: Color,
    /// Under the whole bar, across its width.
    pub rule: Color,
    pub line_width: f32,
    pub padding: f32,
    pub gap: f32,
    pub radius: f32,
    pub text_size: TextSize,
}

impl TabStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            tab: StateColors::new(p.hover.with_alpha(0), p.hover, p.pressed),
            text: p.dim,
            picked_text: p.text,
            line: p.accent,
            rule: p.border,
            line_width: 2.0,
            padding: theme.padding * 1.2,
            gap: 2.0,
            radius: theme.radius.min(6.0),
            text_size: TextSize::Strong,
        }
    }
}

/// `labels` in a row from the rect's left, the one at `selected` picked.
#[derive(Debug)]
pub struct TabBar<'v> {
    labels: &'v [&'v str],
    selected: &'v mut usize,
    style: Option<TabStyle>,
    enabled: bool,
}

impl<'v> TabBar<'v> {
    pub fn new(labels: &'v [&'v str], selected: &'v mut usize) -> Self {
        Self {
            labels,
            selected,
            style: None,
            enabled: true,
        }
    }

    #[must_use]
    pub fn style(mut self, style: TabStyle) -> Self {
        self.style = Some(style);
        self
    }

    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
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

impl<Id: WidgetId> Widget<Id> for TabBar<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| TabStyle::new(ui.theme));
        let mut response = Response {
            rect,
            ..Response::default()
        };
        let rule = Rect::new(rect.x, rect.bottom() - 1.0, rect.width, 1.0);
        ui.fill(rule, style.rule)?;
        let last = self.labels.len().saturating_sub(1);
        let mut x = rect.x;
        let mut picked_cell = None;
        let mut moved = None;
        for (i, label) in self.labels.iter().enumerate() {
            let part = u16::try_from(i).unwrap_or(LINE_W - 1).min(LINE_W - 1);
            let text = Label::new(label, style.text).size(style.text_size);
            let width = text.width(ui) + style.padding * 2.0;
            let cell = Rect::new(
                x,
                rect.y,
                width.min((rect.right() - x).max(0.0)),
                rect.height,
            );
            x += width + style.gap;
            let input = ui.interact(id, part, cell, Sense::Click, self.enabled);
            response.hovered |= input.hovered;
            response.focused |= input.focused;
            if input.clicked && *self.selected != i {
                moved = Some(i);
            }
            if input.focused {
                for (key, _) in ui.keys() {
                    match key {
                        KeyCode::Right => moved = Some((moved.unwrap_or(i) + 1).min(last)),
                        KeyCode::Left => moved = Some(moved.unwrap_or(i).saturating_sub(1)),
                        _ => {}
                    }
                }
            }
            let state = ui.eased(id, part, input);
            let picked = *self.selected == i;
            if picked {
                picked_cell = Some(cell);
            }
            let face = Rect::new(cell.x, cell.y + 3.0, cell.width, cell.height - 6.0);
            Surface::new(style.tab.at(state))
                .radius(style.radius)
                .paint(ui, face)?;
            FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, face)?;
            let lit = ui.animate(
                id,
                part,
                Channel::Value,
                f32::from(u8::from(picked)),
                ui.theme.motion.hover,
            );
            let color = style.text.mix(style.picked_text, lit).fade(state.enabled);
            Label { color, ..text }.centered().paint(ui, cell)?;
        }
        if let Some(cell) = picked_cell {
            let rate = ui.theme.motion.slide;
            let lx = ui.animate(
                id,
                LINE_X,
                Channel::Value,
                cell.x + style.padding * 0.5,
                rate,
            );
            let lw = ui.animate(id, LINE_W, Channel::Value, cell.width - style.padding, rate);
            let line = Rect::new(lx, rect.bottom() - style.line_width, lw, style.line_width);
            ui.round(line, style.line_width / 2.0, style.line)?;
        }
        if let Some(i) = moved.filter(|i| *i != *self.selected) {
            if response.focused {
                let part = u16::try_from(i).unwrap_or(LINE_W - 1);
                ui.state.give_focus(Some(Key::new(id, part)));
            }
            *self.selected = i;
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
    use crate::window::{Modifiers, MouseButton, WindowEvent};

    fn frame(raster: &mut CpuRasterizer, state: &mut UiState<u8>, picked: &mut usize) {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("frame");
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            TabBar::new(&["General", "Audio", "Video"], picked)
                .show(&mut ui, 1, Rect::new(0.0, 0.0, 300.0, 36.0))
                .expect("draw");
        }
        drop(ctx);
        raster.end_frame().expect("end");
    }

    #[test]
    fn a_click_picks_a_tab_and_the_arrows_walk_them() {
        let (mut raster, mut state, mut picked) = (CpuRasterizer::new(300, 36), UiState::new(), 0);
        frame(&mut raster, &mut state, &mut picked);
        for pressed in [true, false] {
            state.handle(&WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                modifiers: Modifiers::default(),
                x: 5.0,
                y: 18.0,
                click_count: 1,
            });
            frame(&mut raster, &mut state, &mut picked);
        }
        assert_eq!(picked, 0);
        for key in [
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Left,
        ] {
            state.handle(&WindowEvent::KeyInput {
                key,
                pressed: true,
                modifiers: Modifiers::default(),
            });
            frame(&mut raster, &mut state, &mut picked);
        }
        assert_eq!(picked, 1);
    }
}
