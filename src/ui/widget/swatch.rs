//! Colour swatches: squares to pick a colour from, laid out in rows across the rect.

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::FocusRing;
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{Component, Surface};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

/// How swatches look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwatchStyle {
    /// Side of each square, in pixels.
    pub size: f32,
    /// Space between squares, in pixels.
    pub gap: f32,
    /// Corner radius in pixels.
    pub radius: f32,
    /// Border colour.
    pub border: Color,
    /// The ring round the picked one.
    pub ring: Color,
    /// Colours of the checkerboard behind transparent colours.
    pub checker: (Color, Color),
}

impl SwatchStyle {
    /// Creates the default style from `theme`.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            size: 24.0,
            gap: 6.0,
            radius: 5.0,
            border: p.border,
            ring: p.text,
            checker: (Color::rgb(255, 255, 255), Color::rgb(204, 204, 204)),
        }
    }
}

/// `colors` in rows from the rect's top left, `selected` picked.
#[derive(Debug)]
pub struct Swatches<'v> {
    colors: &'v [Color],
    selected: &'v mut Option<usize>,
    style: Option<SwatchStyle>,
}

impl<'v> Swatches<'v> {
    /// Creates swatches for `colors` editing `selected`.
    pub fn new(colors: &'v [Color], selected: &'v mut Option<usize>) -> Self {
        Self {
            colors,
            selected,
            style: None,
        }
    }

    /// Sets the style.
    #[must_use]
    pub fn style(mut self, style: SwatchStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Shows the swatches as `id` at `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }

    /// Takes the input as [`Swatches::show`] does, then has `paint` draw it instead of its own
    /// look. [`Look::paint_default`] and [`Look::paint_swatch`] draw the parts of the usual look.
    pub fn show_with<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        paint: impl FnOnce(&mut Ui<'_, Id>, &Look<SwatchParts>) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        let (response, look) = self.run(ui, id, rect);
        paint(ui, &look)?;
        Ok(response)
    }
}

/// One swatch of [`Swatches`], for its painter.
#[derive(Clone, Copy, Debug)]
pub struct Swatch {
    /// The colour it shows.
    pub color: Color,
    /// Its square.
    pub rect: Rect,
    /// Its hover, press and focus, eased.
    pub state: Eased,
    /// How far it is picked, 0 unpicked to 1 picked, eased.
    pub picked: f32,
}

/// What [`Swatches`] draws, for a painter given to [`Swatches::show_with`].
///
/// The swatches are one [`Look`] between them, each a [`Swatch`] in `swatches`, row by row from
/// the top left; [`Look::paint_swatch`] draws one.
#[derive(Clone, Debug)]
pub struct SwatchParts {
    /// The swatches, row by row from the top left.
    pub swatches: Vec<Swatch>,
    /// The style it draws with, the theme's unless one was given.
    pub style: SwatchStyle,
}

impl<'v> Swatches<'v> {
    /// Takes the input and works out what it would draw, without drawing.
    fn run<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> (Response, Look<SwatchParts>) {
        let style = self.style.unwrap_or_else(|| SwatchStyle::new(ui.theme));
        let mut response = Response {
            rect,
            ..Response::default()
        };
        let row = per_row(rect.width, &style);
        let mut moved = None;
        let mut swatches = Vec::with_capacity(self.colors.len());
        for (i, color) in self.colors.iter().enumerate() {
            let part = u16::try_from(i).unwrap_or(u16::MAX);
            #[allow(clippy::cast_precision_loss)]
            let (col, line) = ((i % row) as f32, (i / row) as f32);
            let step = style.size + style.gap;
            let cell = Rect::new(
                rect.x + col * step,
                rect.y + line * step,
                style.size,
                style.size,
            );
            let input = ui.interact(id, part, cell, Sense::Click, true);
            #[cfg(feature = "accessibility")]
            ui.with_accessible(id, part, |node| {
                let [r, g, b, _] = color.to_rgba8();
                node.set_role(Role::Button);
                node.set_label(format!("#{r:02x}{g:02x}{b:02x}"));
            });
            response.hovered |= input.hovered;
            response.focused |= input.focused;
            if input.clicked {
                moved = Some(i);
            }
            if input.focused {
                let mut at = i;
                for (key, _) in ui.keys() {
                    at = arrow(key, at, row, self.colors.len());
                }
                if at != i {
                    moved = Some(at);
                }
            }
            let state = ui.eased(id, part, input);
            let picked = *self.selected == Some(i);
            let lit = ui.animate(
                id,
                part,
                Channel::Value,
                f32::from(u8::from(picked)),
                ui.theme.motion.focus,
            );
            swatches.push(Swatch {
                color: *color,
                rect: cell,
                state,
                picked: lit,
            });
        }
        if let Some(i) = moved.filter(|i| *self.selected != Some(*i)) {
            if response.focused {
                ui.state
                    .give_focus(Some(Key::new(id, u16::try_from(i).unwrap_or(u16::MAX))));
            }
            *self.selected = Some(i);
            response.changed = true;
            ui.mark_changed();
        }
        let look = Look {
            rect,
            // no input of its own: the swatches are always enabled
            state: Eased {
                enabled: 1.0,
                ..Eased::default()
            },
            held_at: None,
            parts: SwatchParts { swatches, style },
        };
        (response, look)
    }
}

impl Look<SwatchParts> {
    /// Every swatch, row by row.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        for i in 0..self.parts.swatches.len() {
            self.paint_swatch(ui, i)?;
        }
        Ok(())
    }

    /// Swatch `i`: its focus ring, the checkerboard behind a transparent colour, its square, and
    /// the ring round it when picked.
    pub fn paint_swatch<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, i: usize) -> AureaResult<()> {
        let Some(swatch) = self.parts.swatches.get(i) else {
            return Ok(());
        };
        let style = &self.parts.style;
        let cell = swatch.rect;
        FocusRing::new(ui.theme, style.radius + 2.0, swatch.state.focus).paint(ui, cell)?;
        if swatch.color.a < 255 {
            checker(ui, cell, style)?;
        }
        let grow = 1.5 * swatch.state.hover - 1.5 * swatch.state.press;
        Surface::new(swatch.color)
            .radius(style.radius)
            .border(1.0, style.border)
            .paint(ui, cell.inset(-grow))?;
        if swatch.picked > 0.01 {
            let ring = cell.inset(-3.0);
            ui.border(
                ring,
                style.radius + 3.0,
                2.0,
                style.ring.fade(swatch.picked),
            )?;
        }
        Ok(())
    }
}

fn per_row(width: f32, style: &SwatchStyle) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((width + style.gap) / (style.size + style.gap))
        .floor()
        .max(1.0) as usize;
    n
}

fn arrow(key: KeyCode, at: usize, row: usize, count: usize) -> usize {
    let last = count - 1;
    match key {
        KeyCode::Right => (at + 1).min(last),
        KeyCode::Left => at.saturating_sub(1),
        KeyCode::Down if at + row <= last => at + row,
        KeyCode::Up if at >= row => at - row,
        KeyCode::Home => 0,
        KeyCode::End => last,
        _ => at,
    }
}

impl<Id: WidgetId> Widget<Id> for Swatches<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        self.show_with(ui, id, rect, |ui, look| look.paint_default(ui))
    }
}

fn checker<Id: WidgetId>(ui: &mut Ui<'_, Id>, cell: Rect, style: &SwatchStyle) -> AureaResult<()> {
    let (light, dark) = style.checker;
    Surface::new(light).radius(style.radius).paint(ui, cell)?;
    let half = cell.width / 2.0;
    ui.fill(Rect::new(cell.x + half, cell.y, half, half), dark)?;
    ui.fill(Rect::new(cell.x, cell.y + half, half, half), dark)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    #[test]
    fn show_with_paints_each_swatch_and_still_picks() {
        let mut rig = Rig::new(120, 40).still();
        let at = Rect::new(0.0, 0.0, 120.0, 40.0);
        let colors = [Color::rgb(255, 0, 0), Color::rgb(0, 0, 255)];
        let mut selected = None;
        let show = |rig: &mut Rig, selected: &mut Option<usize>| {
            rig.frame(|ui| {
                Swatches::new(&colors, selected)
                    .show_with(ui, 1, at, |ui, look| {
                        for swatch in look.parts.swatches.clone() {
                            ui.fill(swatch.rect, Color::rgb(0, 200, 0))?;
                        }
                        Ok(())
                    })
                    .expect("draw")
            })
        };
        show(&mut rig, &mut selected);
        assert_eq!(
            rig.pixel(12, 12),
            [0, 200, 0, 255],
            "the app's look, not the default"
        );
        rig.at(42, 12);
        show(&mut rig, &mut selected);
        rig.button(42, 12, true);
        show(&mut rig, &mut selected);
        rig.button(42, 12, false);
        show(&mut rig, &mut selected);
        assert_eq!(selected, Some(1), "the click still picks it");
    }

    #[test]
    fn rows_fit_the_width_and_the_arrows_walk_them() {
        let style = SwatchStyle::new(&Theme::dark());
        assert_eq!(per_row(24.0, &style), 1);
        assert_eq!(per_row(114.0, &style), 4);
        assert_eq!(arrow(KeyCode::Down, 1, 4, 10), 5);
        assert_eq!(arrow(KeyCode::Down, 7, 4, 10), 7, "no row under it");
        assert_eq!(arrow(KeyCode::Up, 5, 4, 10), 1);
        assert_eq!(arrow(KeyCode::Right, 9, 4, 10), 9);
        assert_eq!(arrow(KeyCode::End, 0, 4, 10), 9);
    }
}
