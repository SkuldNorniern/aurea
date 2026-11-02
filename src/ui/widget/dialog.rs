//! A message drawn on the canvas, over everything, until a button answers it.

use crate::AureaResult;
use crate::render::Rect;
use crate::ui::component::card;
use crate::ui::frame::Ui;
use crate::ui::paint::{Component, Label, Overflow};
use crate::ui::state::{Key, Sense, WidgetId};
use crate::ui::widget::button::Button;

const BACKDROP: u16 = 0xEF00;
const CARD: u16 = 0xEF01;
const WIDTH: f32 = 380.0;
const BODY: f32 = 72.0;

/// What a [`Dialog`] said this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogAnswer {
    /// Still up.
    Open,
    /// Button `index` of the list.
    Button(usize),
    /// Escape.
    Dismissed,
}

/// A modal message on the canvas. Draw it last, each frame it is up: it takes all input.
///
/// ```rust,no_run
/// # use aurea::ui::{Dialog, DialogAnswer, Ui};
/// # fn f(ui: &mut Ui<'_, u8>, asking: &mut bool) -> aurea::AureaResult<()> {
/// if *asking {
///     let answer = Dialog::new("Save changes?")
///         .message("Your edits go away otherwise.")
///         .buttons(&["Discard", "Save"])
///         .show(ui, |button| 9 + button as u8)?;
///     if answer != DialogAnswer::Open {
///         *asking = false;
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Dialog<'t> {
    title: &'t str,
    message: &'t str,
    buttons: &'t [&'t str],
}

impl<'t> Dialog<'t> {
    /// A dialog titled `title` with an OK button.
    pub const fn new(title: &'t str) -> Self {
        Self {
            title,
            message: "",
            buttons: &["OK"],
        }
    }

    /// The body text.
    #[must_use]
    pub const fn message(mut self, message: &'t str) -> Self {
        self.message = message;
        self
    }

    /// The buttons, left to right. The last has the keyboard first.
    #[must_use]
    pub const fn buttons(mut self, buttons: &'t [&'t str]) -> Self {
        self.buttons = buttons;
        self
    }

    /// Shows it centred on the canvas. `id_of` names each button.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id_of: impl Fn(usize) -> Id,
    ) -> AureaResult<DialogAnswer> {
        let message = self.message;
        let height = if message.is_empty() { 0.0 } else { BODY };
        self.draw(ui, &id_of, height, |ui, text| {
            Label::new(message, ui.theme.palette.dim)
                .overflow(Overflow::Wrap(ui.theme.text_size * 1.4))
                .paint(ui, text)
        })
    }

    /// Shows it centred on the canvas, with `body` drawn where the message goes, in a strip
    /// `body_height` tall. The keyboard stays among `body` and the buttons, so a field in `body`
    /// keeps it. Enter in a field takes the last button, and Escape dismisses. `id_of` names each
    /// button.
    pub fn show_with<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id_of: impl Fn(usize) -> Id,
        body_height: f32,
        body: impl FnOnce(&mut Ui<'_, Id>, Rect) -> AureaResult<()>,
    ) -> AureaResult<DialogAnswer> {
        self.draw(ui, &id_of, body_height, body)
    }

    fn draw<Id: WidgetId, B>(
        self,
        ui: &mut Ui<'_, Id>,
        id_of: &impl Fn(usize) -> Id,
        body_height: f32,
        body: B,
    ) -> AureaResult<DialogAnswer>
    where
        B: FnOnce(&mut Ui<'_, Id>, Rect) -> AureaResult<()>,
    {
        let id = id_of(0);
        let first = ui.state.order.len();
        ui.state.text_submitted = false;
        ui.state.text_cancelled = false;
        let (width, height) = (ui.ctx.width() as f32, ui.ctx.height() as f32);
        let canvas = Rect::new(0.0, 0.0, width, height);
        ui.interact(id, BACKDROP, canvas, Sense::Hover, true);
        ui.take_wheel();
        ui.fill(canvas, ui.theme.palette.shadow)?;

        let padding = ui.theme.padding * 1.6;
        let row = ui.theme.control_height;
        let title = ui.theme.text_size * 1.5;
        let gap = if body_height > 0.0 {
            ui.theme.padding
        } else {
            0.0
        };
        let tall = padding * 2.0 + title + body_height + gap + row + ui.theme.padding;
        let at = canvas.centered(WIDTH.min(width - 32.0).max(0.0), tall);
        ui.interact(id, CARD, at, Sense::Hover, true);
        card(ui.theme).paint(ui, at)?;
        let inner = at.inset(padding);
        let (top, rest) = inner.split_top(title, ui.theme.padding);
        Label::new(self.title, ui.theme.palette.text)
            .strong()
            .paint(ui, top)?;
        let (text, foot) = rest.split_top(body_height, gap);
        body(ui, text)?;

        let mut answer = DialogAnswer::Open;
        let last = self.buttons.len().saturating_sub(1);
        let mut right = foot.right();
        for (i, label) in self.buttons.iter().enumerate().rev() {
            let button = if i == last {
                Button::new(label).primary()
            } else {
                Button::new(label)
            };
            let wide = button.width(ui);
            let cell = Rect::new(right - wide, foot.bottom() - row, wide, row);
            right = cell.x - ui.theme.padding / 2.0;
            if button.show(ui, id_of(i), cell)?.clicked {
                answer = DialogAnswer::Button(i);
            }
        }
        if ui.state.text_submitted && !self.buttons.is_empty() {
            answer = DialogAnswer::Button(last);
        }
        if ui.state.escaped || ui.state.text_cancelled {
            answer = DialogAnswer::Dismissed;
        }
        // the keyboard stays here, not with what is under the dialog.
        ui.state.order.drain(..first);
        let ours = ui.state.focus.is_some_and(|f| ui.state.order.contains(&f));
        if !ours && !ui.state.popup_open() && !self.buttons.is_empty() {
            ui.state.give_focus(Some(Key::new(id_of(last), 0)));
        }
        if answer != DialogAnswer::Open {
            ui.mark_changed();
        }
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;
    use crate::ui::theme::Theme;
    use crate::ui::widget::button::Button;
    use crate::ui::{TextArea, TextField};
    use crate::window::{KeyCode, WindowEvent};
    use std::cell::Cell;

    fn show(rig: &mut Rig) -> (DialogAnswer, bool) {
        rig.frame(|ui| {
            let under = Button::new("Under")
                .show(ui, 50, Rect::new(0.0, 0.0, 400.0, 300.0))
                .expect("draw")
                .clicked;
            let answer = Dialog::new("Sure?")
                .message("Really.")
                .buttons(&["No", "Yes"])
                .show(ui, |i| u8::try_from(i).unwrap_or(0))
                .expect("draw");
            (answer, under)
        })
    }

    #[test]
    fn a_click_answers_and_nothing_under_it_gets_one() {
        let mut rig = Rig::new(400, 300);
        show(&mut rig);
        rig.click(10, 10);
        assert_eq!(show(&mut rig), (DialogAnswer::Open, false));
        show(&mut rig);
        // the main button sits at the card's lower right.
        let at = Rect::new(0.0, 0.0, 400.0, 300.0).centered(380.0, 10.0);
        let mut seen = DialogAnswer::Open;
        for y in (150..240).step_by(6) {
            rig.click(at.right() - 30.0, y as f32);
            seen = show(&mut rig).0;
            if seen != DialogAnswer::Open {
                break;
            }
        }
        assert_eq!(seen, DialogAnswer::Button(1));
    }

    #[test]
    fn escape_dismisses_and_enter_takes_the_main_button() {
        let mut rig = Rig::new(400, 300);
        show(&mut rig);
        show(&mut rig);
        rig.key(KeyCode::Escape);
        assert_eq!(show(&mut rig).0, DialogAnswer::Dismissed);
        rig.key(KeyCode::Enter);
        assert_eq!(show(&mut rig).0, DialogAnswer::Button(1));
    }

    fn ask(rig: &mut Rig, text: &mut String, strip: &Cell<Rect>) -> DialogAnswer {
        rig.frame(|ui| {
            Dialog::new("Name?")
                .buttons(&["Cancel", "Save"])
                .show_with(
                    ui,
                    |i| u8::try_from(i).unwrap_or(0),
                    40.0,
                    |ui, rect| {
                        strip.set(rect);
                        TextField::new(text).show(ui, 9, rect).map(drop)
                    },
                )
                .expect("dialog")
        })
    }

    #[test]
    fn a_field_in_the_body_keeps_the_focus_it_is_given() {
        let mut rig = Rig::new(400, 300);
        let mut text = String::new();
        let strip = Cell::new(Rect::default());
        ask(&mut rig, &mut text, &strip);
        assert_eq!(
            rig.state.focus,
            Some(Key::new(1, 0)),
            "buttons take it first"
        );
        let at = strip.get();
        rig.click(at.x + 20.0, at.y + at.height / 2.0);
        assert_eq!(ask(&mut rig, &mut text, &strip), DialogAnswer::Open);
        assert_eq!(rig.state.focus, Some(Key::new(9, 0)));
        assert_eq!(ask(&mut rig, &mut text, &strip), DialogAnswer::Open);
        assert_eq!(rig.state.focus, Some(Key::new(9, 0)));
    }

    fn over_a_field(
        rig: &mut Rig,
        under: &mut String,
        text: &mut String,
        strip: &Cell<Rect>,
    ) -> DialogAnswer {
        rig.frame(|ui| {
            TextField::new(under)
                .show(ui, 8, Rect::new(10.0, 10.0, 120.0, 30.0))
                .expect("under");
            Dialog::new("Name?")
                .buttons(&["Cancel", "Save"])
                .show_with(
                    ui,
                    |i| u8::try_from(i).unwrap_or(0),
                    40.0,
                    |ui, rect| {
                        strip.set(rect);
                        TextField::new(text).show(ui, 9, rect).map(drop)
                    },
                )
                .expect("dialog")
        })
    }

    fn typing(text: &str) -> WindowEvent {
        WindowEvent::TextInput { text: text.into() }
    }

    #[test]
    fn a_field_under_the_dialog_loses_the_keyboard() {
        let mut rig = Rig::new(400, 300);
        let (mut under, mut text) = (String::new(), String::new());
        let strip = Cell::new(Rect::default());
        rig.frame(|ui| {
            TextField::new(&mut under)
                .show(ui, 8, Rect::new(10.0, 10.0, 120.0, 30.0))
                .expect("under");
        });
        rig.click(20.0, 20.0);
        rig.frame(|ui| {
            TextField::new(&mut under)
                .show(ui, 8, Rect::new(10.0, 10.0, 120.0, 30.0))
                .expect("under");
        });
        assert_eq!(rig.state.focus, Some(Key::new(8, 0)));
        over_a_field(&mut rig, &mut under, &mut text, &strip);
        assert_eq!(rig.state.focus, Some(Key::new(1, 0)));
        rig.send(typing("x"));
        over_a_field(&mut rig, &mut under, &mut text, &strip);
        assert_eq!(under, "");
    }

    #[test]
    fn tab_stays_inside_the_dialog() {
        let mut rig = Rig::new(400, 300);
        let (mut under, mut text) = (String::new(), String::new());
        let strip = Cell::new(Rect::default());
        over_a_field(&mut rig, &mut under, &mut text, &strip);
        for _ in 0..7 {
            rig.key(KeyCode::Tab);
            over_a_field(&mut rig, &mut under, &mut text, &strip);
            let focus = rig.state.focus.expect("focus");
            assert_ne!(focus, Key::new(8, 0), "tab reached the field under");
        }
    }

    #[test]
    fn escape_in_a_body_field_dismisses_in_one_press() {
        let mut rig = Rig::new(400, 300);
        let mut text = String::new();
        let strip = Cell::new(Rect::default());
        ask(&mut rig, &mut text, &strip);
        let at = strip.get();
        rig.click(at.x + 20.0, at.y + at.height / 2.0);
        ask(&mut rig, &mut text, &strip);
        rig.key(KeyCode::Escape);
        assert_eq!(ask(&mut rig, &mut text, &strip), DialogAnswer::Dismissed);
    }

    #[test]
    fn enter_in_a_body_field_takes_the_main_button_with_the_text() {
        let mut rig = Rig::new(400, 300);
        let mut text = String::new();
        let strip = Cell::new(Rect::default());
        ask(&mut rig, &mut text, &strip);
        let at = strip.get();
        rig.click(at.x + 20.0, at.y + at.height / 2.0);
        ask(&mut rig, &mut text, &strip);
        rig.send(typing("a"));
        rig.key(KeyCode::Enter);
        assert_eq!(ask(&mut rig, &mut text, &strip), DialogAnswer::Button(1));
        assert_eq!(text, "a");
    }

    #[test]
    fn enter_in_a_text_area_body_stays_a_new_line() {
        let mut rig = Rig::new(400, 300);
        let mut text = String::new();
        let strip = Cell::new(Rect::default());
        let draw = |rig: &mut Rig, text: &mut String| {
            rig.frame(|ui| {
                Dialog::new("Note")
                    .buttons(&["Cancel", "Save"])
                    .show_with(
                        ui,
                        |i| u8::try_from(i).unwrap_or(0),
                        60.0,
                        |ui, rect| {
                            strip.set(rect);
                            TextArea::new(text).show(ui, 9, rect).map(drop)
                        },
                    )
                    .expect("dialog")
            })
        };
        draw(&mut rig, &mut text);
        let at = strip.get();
        rig.click(at.x + 20.0, at.y + at.height / 2.0);
        draw(&mut rig, &mut text);
        rig.key(KeyCode::Enter);
        assert_eq!(draw(&mut rig, &mut text), DialogAnswer::Open);
        assert_eq!(text, "\n");
    }

    #[test]
    fn the_buttons_stand_clear_of_the_body() {
        let mut rig = Rig::new(400, 300);
        let mut text = String::new();
        let strip = Cell::new(Rect::default());
        ask(&mut rig, &mut text, &strip);
        ask(&mut rig, &mut text, &strip);
        let at = strip.get();
        let gap = Theme::dark().padding;
        rig.click(at.right() - 20.0, at.bottom() + gap / 2.0);
        assert_eq!(ask(&mut rig, &mut text, &strip), DialogAnswer::Open);
    }
}
