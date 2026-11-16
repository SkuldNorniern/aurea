//! A chip: a pill that turns on and off, for filters and tags.

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape, Surface, TextSize};
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Theme;

/// How a chip looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChipStyle {
    /// Fill colours when off, by state.
    pub off: StateColors,
    /// Fill when on.
    pub on: Color,
    /// Outline when off.
    pub border: Color,
    /// Outline when on.
    pub on_border: Color,
    /// Label colour when off.
    pub text: Color,
    /// Label colour when on.
    pub on_text: Color,
    /// Horizontal inset from the pill's edge, in pixels.
    pub padding: f32,
    /// Size of the label text.
    pub text_size: TextSize,
}

impl ChipStyle {
    /// Creates the default style from `theme`.
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
    /// Creates a chip showing `label` and toggling `on`.
    pub fn new(label: &'v str, on: &'v mut bool) -> Self {
        Self {
            label,
            on,
            style: None,
            enabled: true,
        }
    }

    /// Sets the style.
    #[must_use]
    pub fn style(mut self, style: ChipStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Sets whether the chip takes input.
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

    /// Shows the chip as `id` at `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

/// What a [`Chip`] draws, for a painter given to [`Chip::show_with`].
#[derive(Clone, Copy, Debug)]
pub struct ChipParts<'v> {
    /// Whether it is on, after this frame's click.
    pub on: bool,
    /// How far on it is, 0 off to 1 on.
    pub progress: f32,
    /// The label.
    pub label: &'v str,
    /// Whether it takes input; the default look fades when not.
    pub enabled: bool,
    /// The style it draws with, the theme's unless one was given.
    pub style: ChipStyle,
}

impl<'v> Chip<'v> {
    /// Takes the input and works out what it would draw, without drawing.
    fn run<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> (Response, Look<ChipParts<'v>>) {
        let input = ui.interact(id, 0, rect, Sense::Click, self.enabled);
        let mut response = Response::new(rect, input);
        if input.clicked {
            *self.on = !*self.on;
            response.changed = true;
            ui.mark_changed();
        }
        #[cfg(feature = "accessibility")]
        {
            let on = *self.on;
            ui.accessible_description(id, 0, Role::Switch, self.label, None);
            ui.with_accessible(id, 0, |node| node.set_toggled(on.into()));
        }
        let style = self.style.unwrap_or_else(|| ChipStyle::new(ui.theme));
        let state = ui.eased(id, 0, input);
        let rate = ui.theme.motion.slide * 1.5;
        let progress = ui.animate(id, 0, Channel::Value, f32::from(u8::from(*self.on)), rate);
        let look = Look {
            rect,
            state,
            held_at: ui.held_at(id, 0),
            parts: ChipParts {
                on: *self.on,
                progress,
                label: self.label,
                enabled: self.enabled,
                style,
            },
        };
        (response, look)
    }

    /// Takes the input as [`Chip::show`] does, then has `paint` draw it instead of its own look.
    /// [`Look::paint_default`], [`Look::paint_surface`], [`Look::paint_tick`] and
    /// [`Look::paint_label`] draw the parts of the usual look.
    pub fn show_with<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        paint: impl FnOnce(&mut Ui<'_, Id>, &Look<ChipParts<'v>>) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        let (response, look) = self.run(ui, id, rect);
        paint(ui, &look)?;
        Ok(response)
    }
}

impl Look<ChipParts<'_>> {
    /// The chip's own look: the pill, then the tick and the label.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_surface(ui)?;
        self.paint_tick(ui)?;
        self.paint_label(ui)
    }

    /// The focus ring and the pill, filled and outlined as far on as it is.
    pub fn paint_surface<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style, t) = (self.state, &self.parts.style, self.parts.progress);
        FocusRing::new(ui.theme, self.rect.height / 2.0, state.focus).paint(ui, self.rect)?;
        let fill = style.off.at(state).mix(style.on.fade(state.enabled), t);
        let border = style.border.mix(style.on_border, t).fade(state.enabled);
        Surface::new(fill)
            .pill()
            .border(ui.theme.border_width, border)
            .paint(ui, self.rect)
    }

    /// The tick at the pill's start, drawn in as it turns on.
    pub fn paint_tick<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style, t) = (self.state, &self.parts.style, self.parts.progress);
        if t > 0.01 {
            let (tick, _) = self.split();
            Mark::new(MarkShape::Check, style.on_border.fade(state.enabled))
                .progress(t)
                .paint(ui, tick.inset(self.rect.height * 0.12))?;
        }
        Ok(())
    }

    /// The label after the tick, in the colour it is now.
    pub fn paint_label<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style, t) = (self.state, &self.parts.style, self.parts.progress);
        let (_, text) = self.split();
        let color = style.text.mix(style.on_text, t).fade(state.enabled);
        Label::new(self.parts.label, color)
            .size(style.text_size)
            .centered()
            .paint(ui, text)
    }

    /// The tick's cell, which is as wide as the tick has drawn in, and the rect the label is left
    /// in.
    pub fn split(&self) -> (Rect, Rect) {
        let style = &self.parts.style;
        let tick = self.rect.height * 0.55 * self.parts.progress;
        let inner = self.rect.inset_xy(style.padding * 0.8, 0.0);
        let at = Rect::new(inner.x, self.rect.y, tick, self.rect.height);
        let text = Rect::new(
            inner.x + tick,
            self.rect.y,
            (inner.width - tick).max(0.0),
            self.rect.height,
        );
        (at, text)
    }
}

impl<Id: WidgetId> Widget<Id> for Chip<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        self.show_with(ui, id, rect, |ui, look| look.paint_default(ui))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::ui::rig::Rig;
    use crate::window::{KeyCode, Modifiers, WindowEvent};

    fn draw_chip(rig: &mut Rig, on: &mut bool, at: Rect) {
        rig.frame(|ui| {
            Chip::new("Wired", on)
                .show_with(ui, 1, at, |ui, look| {
                    ui.fill(look.rect, Color::rgb(0, 200, 0))
                })
                .expect("draw");
        });
    }

    #[test]
    fn show_with_draws_the_apps_look_and_still_toggles() {
        let mut rig = Rig::new(120, 40);
        let at = Rect::new(0.0, 0.0, 120.0, 40.0);
        let mut on = false;
        draw_chip(&mut rig, &mut on, at);
        assert_eq!(
            rig.pixel(60, 20),
            [0, 200, 0, 255],
            "the app's look, not the default"
        );
        rig.at(60, 20);
        draw_chip(&mut rig, &mut on, at);
        rig.button(60, 20, true);
        draw_chip(&mut rig, &mut on, at);
        rig.button(60, 20, false);
        draw_chip(&mut rig, &mut on, at);
        assert!(on, "the click still turns it on");
    }

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
