//! Radio buttons: options in a column, one picked.

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Surface};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

/// How radio buttons look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadioStyle {
    pub ring: StateColors,
    pub border: Color,
    pub picked: Color,
    pub dot: Color,
    pub label: Color,
    pub size: f32,
    /// Height of one option's row.
    pub row: f32,
}

impl RadioStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            ring: StateColors::new(p.raised, p.hover, p.pressed),
            border: p.border,
            picked: p.accent,
            dot: p.on_accent,
            label: p.text,
            size: 18.0,
            row: theme.control_height,
        }
    }
}

/// `options` in a column from the rect's top, one row each; the one at `selected` is picked.
#[derive(Debug)]
pub struct RadioGroup<'v> {
    options: &'v [&'v str],
    selected: &'v mut usize,
    style: Option<RadioStyle>,
    enabled: bool,
}

impl<'v> RadioGroup<'v> {
    pub fn new(options: &'v [&'v str], selected: &'v mut usize) -> Self {
        Self {
            options,
            selected,
            style: None,
            enabled: true,
        }
    }

    #[must_use]
    pub fn style(mut self, style: RadioStyle) -> Self {
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

    fn moved<Id: WidgetId>(&self, ui: &Ui<'_, Id>, at: usize) -> Option<usize> {
        let last = self.options.len().checked_sub(1)?;
        let mut to = at;
        for (key, _) in ui.keys() {
            to = match key {
                KeyCode::Down | KeyCode::Right => (to + 1).min(last),
                KeyCode::Up | KeyCode::Left => to.saturating_sub(1),
                _ => continue,
            };
        }
        (to != at).then_some(to)
    }
}

impl<Id: WidgetId> Widget<Id> for RadioGroup<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let style = self.style.unwrap_or_else(|| RadioStyle::new(ui.theme));
        let mut response = Response {
            rect,
            ..Response::default()
        };
        let mut moved = None;
        for (i, option) in self.options.iter().enumerate() {
            let part = u16::try_from(i).unwrap_or(u16::MAX);
            #[allow(clippy::cast_precision_loss)]
            let row = Rect::new(rect.x, rect.y + style.row * i as f32, rect.width, style.row);
            let input = ui.interact(id, part, row, Sense::Click, self.enabled);
            response.hovered |= input.hovered;
            response.focused |= input.focused;
            if input.clicked && *self.selected != i {
                moved = Some(i);
            }
            if input.focused && moved.is_none() {
                moved = self.moved(ui, i);
            }
            let state = ui.eased(id, part, input);
            let on = f32::from(u8::from(*self.selected == i));
            let t = ui.animate(id, part, Channel::Value, on, ui.theme.motion.slide * 1.5);
            let s = style.size.min(row.height);
            let (cell, text) = row.split_left(s, ui.theme.padding * 0.8);
            let ring = cell.centered(s, s);
            FocusRing::new(ui.theme, s / 2.0, state.focus).paint(ui, ring)?;
            let fill = style
                .ring
                .at(state)
                .mix(style.picked.fade(state.enabled), t);
            Surface::new(fill)
                .pill()
                .border(
                    ui.theme.border_width,
                    style.border.fade(state.enabled * (1.0 - t)),
                )
                .paint(ui, ring)?;
            if t > 0.01 {
                ui.circle(ring.center(), s * 0.2 * t, style.dot.fade(state.enabled))?;
            }
            Label::new(option, style.label.fade(state.enabled)).paint(ui, text)?;
        }
        if let Some(i) = moved {
            if response.focused {
                let part = u16::try_from(i).unwrap_or(u16::MAX);
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

    const OPTIONS: [&str; 3] = ["Low", "Mid", "High"];

    fn frame(raster: &mut CpuRasterizer, state: &mut UiState<u8>, picked: &mut usize) {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("frame");
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            RadioGroup::new(&OPTIONS, picked)
                .show(&mut ui, 1, Rect::new(0.0, 0.0, 160.0, 90.0))
                .expect("draw");
        }
        drop(ctx);
        raster.end_frame().expect("end");
    }

    fn send(state: &mut UiState<u8>, event: WindowEvent) {
        state.handle(&event);
    }

    #[test]
    fn a_click_picks_and_the_arrows_move_the_pick() {
        let (mut raster, mut state, mut picked) = (CpuRasterizer::new(160, 90), UiState::new(), 0);
        state.set_reduced_motion(true);
        frame(&mut raster, &mut state, &mut picked);
        for pressed in [true, false] {
            send(
                &mut state,
                WindowEvent::MouseButton {
                    button: MouseButton::Left,
                    pressed,
                    modifiers: Modifiers::default(),
                    x: 20.0,
                    y: 45.0,
                    click_count: 1,
                },
            );
            frame(&mut raster, &mut state, &mut picked);
        }
        assert_eq!(picked, 1);
        for key in [KeyCode::Down, KeyCode::Down, KeyCode::Up] {
            send(
                &mut state,
                WindowEvent::KeyInput {
                    key,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            );
            frame(&mut raster, &mut state, &mut picked);
        }
        assert_eq!(picked, 1, "down to the end, then one back");
        assert!(state.has_focus(), "the focus went along");
    }
}
