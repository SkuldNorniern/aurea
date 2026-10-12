//! Colour swatches: squares to pick a colour from, laid out in rows across
//! the rect. A see-through colour shows over a checkerboard.

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Surface};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

/// How swatches look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwatchStyle {
    pub size: f32,
    pub gap: f32,
    pub radius: f32,
    pub border: Color,
    /// The ring round the picked one.
    pub ring: Color,
    pub checker: (Color, Color),
}

impl SwatchStyle {
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

/// `colors` in rows from the rect's top left, `selected` picked. The arrow
/// keys move the pick through the rows once one has the keyboard.
#[derive(Debug)]
pub struct Swatches<'v> {
    colors: &'v [Color],
    selected: &'v mut Option<usize>,
    style: Option<SwatchStyle>,
}

impl<'v> Swatches<'v> {
    pub fn new(colors: &'v [Color], selected: &'v mut Option<usize>) -> Self {
        Self {
            colors,
            selected,
            style: None,
        }
    }

    #[must_use]
    pub fn style(mut self, style: SwatchStyle) -> Self {
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

/// How many swatches fit a row of `width`.
fn per_row(width: f32, style: &SwatchStyle) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((width + style.gap) / (style.size + style.gap))
        .floor()
        .max(1.0) as usize;
    n
}

/// Where an arrow key goes from `at` in rows of `row` among `count`.
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
        let style = self.style.unwrap_or_else(|| SwatchStyle::new(ui.theme));
        let mut response = Response {
            rect,
            ..Response::default()
        };
        let row = per_row(rect.width, &style);
        let mut moved = None;
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
            FocusRing::new(ui.theme, style.radius + 2.0, state.focus).paint(ui, cell)?;
            if color.a < 255 {
                checker(ui, cell, &style)?;
            }
            let grow = 1.5 * state.hover - 1.5 * state.press;
            Surface::new(*color)
                .radius(style.radius)
                .border(1.0, style.border)
                .paint(ui, cell.inset(-grow))?;
            if lit > 0.01 {
                let ring = cell.inset(-3.0);
                ui.border(ring, style.radius + 3.0, 2.0, style.ring.fade(lit))?;
            }
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
        Ok(response)
    }
}

/// A four-square checkerboard under a see-through colour.
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
