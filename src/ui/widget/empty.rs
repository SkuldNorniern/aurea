//! An empty state: what a view says when it has nothing to show yet, with a mark, a title, a line
//! of help and, if given, a button to start.

use super::Widget;
use super::button::{Button, ButtonParts};
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::frame::{Eased, Response, Ui};
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

/// What an [`EmptyState`] draws, from [`EmptyState::look`].
#[derive(Clone, Copy, Debug)]
pub struct EmptyParts<'t> {
    /// The title.
    pub title: &'t str,
    /// The help text; empty for none.
    pub help: &'t str,
    /// The action's label, if it has one.
    pub action: Option<&'t str>,
    /// The action's button, its input taken and its own look ready to draw.
    pub button: Option<Look<ButtonParts<'t>>>,
    /// The mark in its disc, if it has one.
    pub mark: Option<MarkShape>,
    /// The disc the mark sits in, if it has a mark.
    pub disc: Option<Rect>,
    /// Where the title goes.
    pub title_rect: Rect,
    /// Where the help goes; `None` with no help.
    pub help_rect: Option<Rect>,
    /// How the help overflows: wrapped when it runs past one line.
    pub help_overflow: Overflow,
    /// Where the action's button goes; `None` with no action.
    pub action_rect: Option<Rect>,
    /// Fill of the disc behind the mark.
    pub disc_fill: Color,
    /// Colour of the mark.
    pub mark_color: Color,
    /// Colour of the title.
    pub title_color: Color,
    /// Colour of the help.
    pub help_color: Color,
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

    /// Takes the action's input as [`EmptyState::show`] does and works out where everything goes,
    /// without drawing: the app draws it, or [`Look::paint_default`] does, the mark, title and
    /// help, then the action's button.
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<EmptyParts<'t>> {
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
        let mark_h = if self.mark.is_some() {
            56.0 + 16.0
        } else {
            0.0
        };
        let button = ui.theme.control_height;
        let action_h = if self.action.is_some() {
            20.0 + button
        } else {
            0.0
        };
        let height = mark_h + fonts.strong.size * 1.6 + help_lines * line + action_h;
        let mut y = rect.center().y - height / 2.0;
        let x = rect.center().x - width / 2.0;
        let disc = self
            .mark
            .map(|_| Rect::new(rect.center().x - 28.0, y, 56.0, 56.0));
        if self.mark.is_some() {
            y += mark_h;
        }
        let title = Rect::new(x, y, width, fonts.strong.size * 1.6);
        y = title.bottom();
        let help = (help_lines > 0.0).then(|| Rect::new(x, y, width, help_lines * line));
        if let Some(help) = help {
            y = help.bottom();
        }
        let action_rect = self.action.map(|label| {
            let w = Button::new(label).primary().width(ui);
            Rect::new(rect.center().x - w / 2.0, y + 20.0, w, button)
        });
        let action = match (self.action, action_rect) {
            (Some(label), Some(at)) => Some(Button::new(label).primary().look(ui, id, at)),
            _ => None,
        };
        let response = action.as_ref().map_or(
            Response {
                rect,
                ..Response::default()
            },
            |look| look.response,
        );
        Look {
            rect,
            state: Eased {
                enabled: 1.0,
                ..Eased::default()
            },
            held_at: None,
            response,
            parts: EmptyParts {
                title: self.title,
                help: self.help,
                action: self.action,
                button: action,
                mark: self.mark,
                disc,
                title_rect: title,
                help_rect: help,
                help_overflow: if help_lines > 1.0 {
                    Overflow::Wrap(line)
                } else {
                    Overflow::Visible
                },
                action_rect,
                disc_fill: p.raised,
                mark_color: p.dim,
                title_color: p.text,
                help_color: p.dim,
            },
        }
    }
}

impl Look<EmptyParts<'_>> {
    /// The empty state's own look: the mark, the title, the help, then the action's button.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_mark(ui)?;
        self.paint_title(ui)?;
        self.paint_help(ui)?;
        self.paint_action(ui)
    }

    /// The action's button with its own look, if it has one.
    pub fn paint_action<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        match &self.parts.button {
            Some(button) => button.paint_default(ui),
            None => Ok(()),
        }
    }

    /// The mark in its soft disc, if it has one.
    pub fn paint_mark<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (Some(shape), Some(disc)) = (self.parts.mark, self.parts.disc) else {
            return Ok(());
        };
        Surface::new(self.parts.disc_fill).pill().paint(ui, disc)?;
        Mark::new(shape, self.parts.mark_color).paint(ui, disc.inset(16.0))
    }

    /// The title, in the strong text.
    pub fn paint_title<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        Label::new(self.parts.title, self.parts.title_color)
            .strong()
            .centered()
            .paint(ui, self.parts.title_rect)
    }

    /// The help, centred and wrapped past one line, if there is any.
    pub fn paint_help<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let Some(help) = self.parts.help_rect else {
            return Ok(());
        };
        Label::new(self.parts.help, self.parts.help_color)
            .centered()
            .overflow(self.parts.help_overflow)
            .paint(ui, help)
    }
}

impl<Id: WidgetId> Widget<Id> for EmptyState<'_> {
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
    use crate::ui::rig::Rig;
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

    #[test]
    fn look_paints_the_apps_look_and_the_action_still_clicks() {
        let mut rig = Rig::new(400, 300);
        let area = Rect::new(0.0, 0.0, 400.0, 300.0);
        let show = |rig: &mut Rig| {
            rig.frame(|ui| {
                let empty = EmptyState::new("No trackers")
                    .help("Turn a tracker on and hold it near this computer to add it.")
                    .mark(MarkShape::Plus)
                    .action("Add a tracker")
                    .look(ui, 2, area);
                ui.fill(empty.rect, Color::rgb(0, 200, 0)).expect("draw");
                empty.response
            })
        };
        let first = show(&mut rig);
        assert_eq!(
            rig.pixel(5, 5),
            [0, 200, 0, 255],
            "the app's look, not the default"
        );
        let (x, y) = (
            f64::from(first.rect.x + first.rect.width / 2.0),
            f64::from(first.rect.y + first.rect.height / 2.0),
        );
        rig.at(x, y);
        show(&mut rig);
        rig.click(x, y);
        assert!(show(&mut rig).clicked, "the action still clicks");
    }
}
