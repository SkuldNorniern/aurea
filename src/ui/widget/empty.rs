//! An empty state: what a view says when it has nothing to show yet, with a mark, a title, a line
//! of help and, if given, a button to start.

use super::Widget;
use super::button::Button;
use crate::AureaResult;
use crate::render::Rect;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape, Overflow, Surface};
use crate::ui::state::WidgetId;

const WIDEST: f32 = 360.0;

/// A title and help centred in the rect, a mark above them and an action below.
#[derive(Debug)]
pub struct EmptyState<'t> {
    title: &'t str,
    help: &'t str,
    action: Option<&'t str>,
    mark: Option<MarkShape>,
}

impl<'t> EmptyState<'t> {
    /// An empty state titled `title`.
    pub fn new(title: &'t str) -> Self {
        Self {
            title,
            help: "",
            action: None,
            mark: None,
        }
    }

    /// Sets the help text.
    #[must_use]
    pub fn help(mut self, help: &'t str) -> Self {
        self.help = help;
        self
    }

    /// A primary button under the text.
    #[must_use]
    pub fn action(mut self, label: &'t str) -> Self {
        self.action = Some(label);
        self
    }

    /// A mark in a soft circle above the title.
    #[must_use]
    pub fn mark(mut self, mark: MarkShape) -> Self {
        self.mark = Some(mark);
        self
    }

    /// Draws it in `rect` and reports the button's input.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

impl<Id: WidgetId> Widget<Id> for EmptyState<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let p = ui.theme.palette;
        let fonts = ui.fonts();
        let line = fonts.text.size * 1.45;
        let width = WIDEST.min(rect.width - ui.theme.padding * 2.0).max(40.0);
        let help_lines = if self.help.is_empty() {
            0.0
        } else {
            (ui.text_width(self.help, &fonts.text) / (width * 0.9))
                .ceil()
                .max(1.0)
        };
        let mark = if self.mark.is_some() {
            56.0 + 16.0
        } else {
            0.0
        };
        let button = ui.theme.control_height;
        let action = if self.action.is_some() {
            20.0 + button
        } else {
            0.0
        };
        let height = mark + fonts.strong.size * 1.6 + help_lines * line + action;
        let mut y = rect.center().y - height / 2.0;
        let x = rect.center().x - width / 2.0;
        if let Some(shape) = self.mark {
            let disc = Rect::new(rect.center().x - 28.0, y, 56.0, 56.0);
            Surface::new(p.raised).pill().paint(ui, disc)?;
            Mark::new(shape, p.dim).paint(ui, disc.inset(16.0))?;
            y += mark;
        }
        let title = Rect::new(x, y, width, fonts.strong.size * 1.6);
        Label::new(self.title, p.text)
            .strong()
            .centered()
            .paint(ui, title)?;
        y = title.bottom();
        if help_lines > 0.0 {
            let help = Rect::new(x, y, width, help_lines * line);
            let overflow = if help_lines > 1.0 {
                Overflow::Wrap(line)
            } else {
                Overflow::Visible
            };
            Label::new(self.help, p.dim)
                .centered()
                .overflow(overflow)
                .paint(ui, help)?;
            y = help.bottom();
        }
        let Some(label) = self.action else {
            return Ok(Response {
                rect,
                ..Response::default()
            });
        };
        let action = Button::new(label).primary();
        let w = action.width(ui);
        let at = Rect::new(rect.center().x - w / 2.0, y + 20.0, w, button);
        action.show(ui, id, at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::{Theme, UiState};

    #[test]
    fn it_draws_with_and_without_an_action() {
        let theme = Theme::dark();
        let mut raster = CpuRasterizer::new(400, 300);
        let mut state = UiState::<u8>::new();
        let mut ctx = raster.begin_frame().expect("frame");
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            let area = Rect::new(0.0, 0.0, 400.0, 300.0);
            let plain = EmptyState::new("Nothing here")
                .show(&mut ui, 1, area)
                .expect("draw");
            assert!(!plain.clicked);
            let full = EmptyState::new("No trackers")
                .help("Turn a tracker on and hold it near this computer to add it.")
                .mark(MarkShape::Plus)
                .action("Add a tracker")
                .show(&mut ui, 2, area)
                .expect("draw");
            assert!(
                full.rect.width > 0.0 && full.rect.y > 150.0,
                "the button sits under the text"
            );
        }
        drop(ctx);
        raster.end_frame().expect("end");
    }
}
