//! A tab bar: labels in a row, each sized to its text, and a line under the picked one that slides
//! to the next.

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{Component, Label, Surface, TextSize};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

const LINE_X: u16 = u16::MAX;
const LINE_W: u16 = u16::MAX - 1;

/// How a tab bar looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabStyle {
    /// Tab fill by state.
    pub tab: StateColors,
    /// Unpicked label colour.
    pub text: Color,
    /// Picked label colour.
    pub picked_text: Color,
    /// Line under the picked tab.
    pub line: Color,
    /// Under the whole bar, across its width.
    pub rule: Color,
    /// Line thickness.
    pub line_width: f32,
    /// Space either side of a label.
    pub padding: f32,
    /// Space between tabs.
    pub gap: f32,
    /// Corner radius of each tab.
    pub radius: f32,
    /// Label size.
    pub text_size: TextSize,
}

impl TabStyle {
    /// Default style from the theme.
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

/// One tab of a [`TabBar`], for its painter.
#[derive(Clone, Copy, Debug)]
pub struct Tab<'v> {
    /// The tab's text.
    pub label: &'v str,
    /// Its cell, which the face and label are drawn in.
    pub rect: Rect,
    /// Its hover, press and focus, eased.
    pub state: Eased,
    /// How far it is picked, 0 unpicked to 1 picked, eased.
    pub picked: f32,
}

/// What a [`TabBar`] draws, from [`TabBar::look`].
///
/// The tabs are one [`Look`] between them: the line under the picked tab slides across the bar,
/// so it is shared, and each tab is a [`Tab`] in `tabs`, from the left.
#[derive(Clone, Debug)]
pub struct TabBarParts<'v> {
    /// The tabs, from the left.
    pub tabs: Vec<Tab<'v>>,
    /// The line under the picked tab, when one is picked.
    pub line: Option<Rect>,
    /// The rule across the bottom of the bar.
    pub rule: Rect,
    /// Whether tabs take input; the default look fades when not.
    pub enabled: bool,
    /// The style it draws with, the theme's unless one was given.
    pub style: TabStyle,
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
    /// Creates a tab bar for `labels`.
    pub fn new(labels: &'v [&'v str], selected: &'v mut usize) -> Self {
        Self {
            labels,
            selected,
            style: None,
            enabled: true,
        }
    }

    /// Sets the look; defaults to the theme's.
    #[must_use]
    pub fn style(mut self, style: TabStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Whether tabs take input.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws the tab bar in `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

impl<'v> TabBar<'v> {
    /// Takes the input as [`TabBar::show`] does and works out what it would draw, without drawing:
    /// the app draws it, or [`Look::paint_default`] does, or its pieces [`Look::paint_rule`],
    /// [`Look::paint_tab`] and [`Look::paint_line`]. The picked tab and the line are as the bar
    /// stood when the frame began; a click picks from the next frame.
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<TabBarParts<'v>> {
        let style = self.style.unwrap_or_else(|| TabStyle::new(ui.theme));
        let mut response = Response {
            rect,
            ..Response::default()
        };
        let rule = Rect::new(rect.x, rect.bottom() - 1.0, rect.width, 1.0);
        let last = self.labels.len().saturating_sub(1);
        let mut x = rect.x;
        let mut picked_cell = None;
        let mut moved = None;
        let mut tabs = Vec::with_capacity(self.labels.len());
        for (i, &label) in self.labels.iter().enumerate() {
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
            #[cfg(feature = "accessibility")]
            {
                let on = *self.selected == i;
                ui.accessible_description(id, part, Role::Tab, label, None);
                ui.with_accessible(id, part, |node| node.set_selected(on));
            }
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
            let lit = ui.animate(
                id,
                part,
                Channel::Value,
                f32::from(u8::from(picked)),
                ui.theme.motion.hover,
            );
            tabs.push(Tab {
                label,
                rect: cell,
                state,
                picked: lit,
            });
        }
        let mut line = None;
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
            line = Some(Rect::new(
                lx,
                rect.bottom() - style.line_width,
                lw,
                style.line_width,
            ));
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
        Look {
            rect,
            // no input of its own: the bar is as enabled as it was asked to be
            state: Eased {
                enabled: f32::from(u8::from(self.enabled)),
                ..Eased::default()
            },
            held_at: None,
            response,
            parts: TabBarParts {
                tabs,
                line,
                rule,
                enabled: self.enabled,
                style,
            },
        }
    }
}

impl Look<TabBarParts<'_>> {
    /// The rule, each tab from the left, then the line under the picked one.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_rule(ui)?;
        for i in 0..self.parts.tabs.len() {
            self.paint_tab(ui, i)?;
        }
        self.paint_line(ui)
    }

    /// The rule across the bottom of the bar.
    pub fn paint_rule<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        ui.fill(self.parts.rule, self.parts.style.rule)
    }

    /// Tab `i`'s face, focus ring and label, its text picked as the line lands on it.
    pub fn paint_tab<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, i: usize) -> AureaResult<()> {
        let Some(tab) = self.parts.tabs.get(i) else {
            return Ok(());
        };
        let style = &self.parts.style;
        let face = Rect::new(
            tab.rect.x,
            tab.rect.y + 3.0,
            tab.rect.width,
            tab.rect.height - 6.0,
        );
        Surface::new(style.tab.at(tab.state))
            .radius(style.radius)
            .paint(ui, face)?;
        FocusRing::new(ui.theme, style.radius, tab.state.focus).paint(ui, face)?;
        let color = style
            .text
            .mix(style.picked_text, tab.picked)
            .fade(tab.state.enabled);
        Label::new(tab.label, color)
            .size(style.text_size)
            .centered()
            .paint(ui, tab.rect)
    }

    /// The line under the picked tab, sliding between tabs.
    pub fn paint_line<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        match self.parts.line {
            Some(line) => ui.round(
                line,
                self.parts.style.line_width / 2.0,
                self.parts.style.line,
            ),
            None => Ok(()),
        }
    }
}

impl<Id: WidgetId> Widget<Id> for TabBar<'_> {
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
                time: Default::default(),
                pointer: PointerKind::Mouse,
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

    #[test]
    fn look_draws_the_apps_look_and_still_picks() {
        let mut rig: Rig = Rig::new(300, 36);
        let at = Rect::new(0.0, 0.0, 300.0, 36.0);
        let mut picked = 2;
        let draw = |rig: &mut Rig, picked: &mut usize| {
            rig.frame(|ui| {
                let bar = TabBar::new(&["General", "Audio", "Video"], picked).look(ui, 1, at);
                ui.fill(bar.rect, Color::rgb(0, 200, 0)).expect("draw");
            });
        };
        draw(&mut rig, &mut picked);
        assert_eq!(rig.pixel(150, 10), [0, 200, 0, 255], "the app's look");
        rig.at(5.0, 18.0);
        draw(&mut rig, &mut picked);
        rig.button(5.0, 18.0, true);
        draw(&mut rig, &mut picked);
        rig.button(5.0, 18.0, false);
        draw(&mut rig, &mut picked);
        assert_eq!(picked, 0, "the click still picks the first tab");
    }
}
