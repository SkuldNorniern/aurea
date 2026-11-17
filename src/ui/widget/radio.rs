//! Radio buttons: options in a column, one picked.

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Point, Rect};
use crate::ui::accessibility::Role;
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{Component, Label, Surface};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

/// How radio buttons look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadioStyle {
    /// Ring fill by state.
    pub ring: StateColors,
    /// Ring outline.
    pub border: Color,
    /// Ring colour when picked.
    pub picked: Color,
    /// Dot colour when picked.
    pub dot: Color,
    /// Label text colour.
    pub label: Color,
    /// Ring diameter.
    pub size: f32,
    /// Height of one option's row.
    pub row: f32,
}

impl RadioStyle {
    /// Default style from the theme.
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
    /// Creates a group for `options`.
    pub fn new(options: &'v [&'v str], selected: &'v mut usize) -> Self {
        Self {
            options,
            selected,
            style: None,
            enabled: true,
        }
    }

    /// Sets the look; defaults to the theme's.
    #[must_use]
    pub fn style(mut self, style: RadioStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Whether the options take input.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws the options in `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }

    /// Takes the input as [`RadioGroup::show`] does and works out what each option would draw,
    /// without drawing: the app draws them, or [`Look::paint_default`] does, or one at a time
    /// [`Look::paint_option`], and its pieces [`Look::paint_ring`], [`Look::paint_dot`] and
    /// [`Look::paint_label`].
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<RadioParts<'v>> {
        let style = self.style.unwrap_or_else(|| RadioStyle::new(ui.theme));
        let mut response = Response {
            rect,
            ..Response::default()
        };
        let mut options = Vec::with_capacity(self.options.len());
        let mut moved = None;
        for (i, &label) in self.options.iter().enumerate() {
            let part = u16::try_from(i).unwrap_or(u16::MAX);
            #[allow(clippy::cast_precision_loss)]
            let row = Rect::new(rect.x, rect.y + style.row * i as f32, rect.width, style.row);
            let input = ui.interact(id, part, row, Sense::Click, self.enabled);
            {
                let on = *self.selected == i;
                ui.accessible_description(id, part, Role::RadioButton, label, None);
                ui.with_accessible(id, part, |node| node.set_toggled(on.into()));
            }
            let state = ui.eased(id, part, input);
            let on = f32::from(u8::from(*self.selected == i));
            let progress = ui.animate(id, part, Channel::Value, on, ui.theme.motion.slide * 1.5);
            options.push(RadioOption {
                index: i,
                label,
                picked: *self.selected == i,
                progress,
                rect: row,
                state,
                held_at: ui.held_at(id, part),
            });
            response.hovered |= input.hovered;
            response.focused |= input.focused;
            if input.clicked && *self.selected != i {
                moved = Some(i);
            }
            if input.focused && moved.is_none() {
                moved = self.moved(ui, i);
            }
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
        Look {
            rect,
            // no input of its own: the group is as enabled as it was asked to be
            state: Eased {
                enabled: f32::from(u8::from(self.enabled)),
                ..Eased::default()
            },
            held_at: None,
            response,
            parts: RadioParts {
                options,
                enabled: self.enabled,
                style,
            },
        }
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

/// What a [`RadioGroup`] draws, from [`RadioGroup::look`].
///
/// The options are one [`Look`] between them, each a [`RadioOption`] in `options`, from the top.
#[derive(Clone, Debug)]
pub struct RadioParts<'v> {
    /// The options, from the top.
    pub options: Vec<RadioOption<'v>>,
    /// Whether the group takes input; the default look fades when not.
    pub enabled: bool,
    /// The style it draws with, the theme's unless one was given.
    pub style: RadioStyle,
}

/// One option of a [`RadioGroup`], for its painter.
#[derive(Clone, Copy, Debug)]
pub struct RadioOption<'v> {
    /// Which option this is, from the top.
    pub index: usize,
    /// The option's text.
    pub label: &'v str,
    /// Whether it is the picked one, as the group stood when the frame began; a click picks from
    /// the next frame.
    pub picked: bool,
    /// How far the dot has grown, 0 unpicked to 1 picked.
    pub progress: f32,
    /// Its row.
    pub rect: Rect,
    /// Its hover, press, focus and enabled, eased.
    pub state: Eased,
    /// Where the press holding it landed, while a pointer holds it.
    pub held_at: Option<Point>,
}

impl Look<RadioParts<'_>> {
    /// Every option from the top.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        for i in 0..self.parts.options.len() {
            self.paint_option(ui, i)?;
        }
        Ok(())
    }

    /// Option `i`'s own look: the ring, the dot, then the label.
    pub fn paint_option<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, i: usize) -> AureaResult<()> {
        self.paint_ring(ui, i)?;
        self.paint_dot(ui, i)?;
        self.paint_label(ui, i)
    }

    /// The focus ring, and the ring's fill and outline, which turn to the picked colour as it is
    /// picked.
    pub fn paint_ring<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, i: usize) -> AureaResult<()> {
        let Some(option) = self.parts.options.get(i) else {
            return Ok(());
        };
        let (state, style, t) = (option.state, &self.parts.style, option.progress);
        let (ring, _) = self.split(ui, option);
        FocusRing::new(ui.theme, ring.width / 2.0, state.focus).paint(ui, ring)?;
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
            .paint(ui, ring)
    }

    /// The dot in the middle of the ring, growing as it is picked.
    pub fn paint_dot<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, i: usize) -> AureaResult<()> {
        let Some(option) = self.parts.options.get(i) else {
            return Ok(());
        };
        let t = option.progress;
        if t > 0.01 {
            let (ring, _) = self.split(ui, option);
            let dot = self.parts.style.dot.fade(option.state.enabled);
            ui.circle(ring.center(), ring.width * 0.2 * t, dot)?;
        }
        Ok(())
    }

    /// The label right of the ring, faded when disabled.
    pub fn paint_label<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, i: usize) -> AureaResult<()> {
        let Some(option) = self.parts.options.get(i) else {
            return Ok(());
        };
        let (_, text) = self.split(ui, option);
        let color = self.parts.style.label.fade(option.state.enabled);
        Label::new(option.label, color).paint(ui, text)
    }

    /// The ring at the row's start, and the rect right of it the label goes in.
    pub fn split<Id: WidgetId>(&self, ui: &Ui<'_, Id>, option: &RadioOption) -> (Rect, Rect) {
        let s = self.parts.style.size.min(option.rect.height);
        let (cell, text) = option.rect.split_left(s, ui.theme.padding * 0.8);
        (cell.centered(s, s), text)
    }
}

impl<Id: WidgetId> Widget<Id> for RadioGroup<'_> {
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

    const OPTIONS: [&str; 3] = ["Low", "Mid", "High"];

    #[test]
    fn look_paints_each_option_and_still_picks() {
        let mut rig = Rig::new(160, 90).still();
        let at = Rect::new(0.0, 0.0, 160.0, 90.0);
        let mut picked = 0;
        let mut painted = Vec::new();
        let draw = |rig: &mut Rig, picked: &mut usize, painted: &mut Vec<usize>| {
            rig.frame(|ui| {
                let group = RadioGroup::new(&OPTIONS, picked).look(ui, 1, at);
                painted.clear();
                painted.extend(group.parts.options.iter().map(|option| option.index));
                for option in &group.parts.options {
                    ui.fill(option.rect, Color::rgb(0, 200, 0)).expect("draw");
                }
            });
        };
        draw(&mut rig, &mut picked, &mut painted);
        assert_eq!(painted, [0, 1, 2], "each option, from the top");
        assert_eq!(rig.pixel(100, 5), [0, 200, 0, 255], "the app's look");
        rig.at(20, 45);
        draw(&mut rig, &mut picked, &mut painted);
        rig.button(20, 45, true);
        draw(&mut rig, &mut picked, &mut painted);
        rig.button(20, 45, false);
        draw(&mut rig, &mut picked, &mut painted);
        assert_eq!(picked, 1, "the click still picks");
    }

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
                    time: Default::default(),
                    pointer: PointerKind::Mouse,
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
