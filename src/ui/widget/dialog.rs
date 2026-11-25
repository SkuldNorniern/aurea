//! A message drawn on the canvas, over everything, until a button answers it.

use std::sync::LazyLock;

use crate::AureaResult;
use crate::dialog::MessageKind;
use crate::render::{Color, Rect, VectorImage};
use crate::ui::Palette;
use crate::ui::accessibility::Role;
use crate::ui::component::card;
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{Component, IconMark, Label, Overflow};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::widget::button::{Button, ButtonParts};
use crate::ui::widget::look::Look;
use crate::ui::widget::scroll::ScrollArea;

const BACKDROP: u16 = 0xEF00;
const CARD: u16 = 0xEF01;
const TITLE: u16 = 0xEF02;
const MESSAGE: u16 = 0xEF03;
/// The offset of a message that scrolls, kept between frames under the scroll's own id.
const SCROLL: Channel = Channel::Custom(0xEF04);
const WIDTH: f32 = 380.0;
/// The clear space kept above and below the card.
const MARGIN: f32 = 16.0;
const GRID: f32 = 24.0;

const INFO: &str = "M12 3a9 9 0 1 1 0 18a9 9 0 1 1 0-18Z M12 10.5 V17.5 M12 6.5 V7.5";
const WARNING: &str = "M12 3 L22 20 H2Z M12 9.5 V14 M12 16.5 V17.5";
const ERROR: &str = "M12 3a9 9 0 1 1 0 18a9 9 0 1 1 0-18Z M8.5 8.5 L15.5 15.5 M15.5 8.5 L8.5 15.5";
const QUESTION: &str = "M12 3a9 9 0 1 1 0 18a9 9 0 1 1 0-18Z M9.5 9.5 a2.5 2.5 0 1 1 3.5 2.3 \
                        c-0.7 0.3 -1 1 -1 1.7 V14.5 M12 17.5 V18";

/// What a [`Dialog`] is about: its icon, the tone of it, and how it is announced. `Plain` has no
/// icon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogKind {
    /// No icon, announced as a dialog.
    #[default]
    Plain,
    /// Information, in the accent colour.
    Info,
    /// Needs a look, in the warning colour, announced as an alert.
    Warning,
    /// Something went wrong, in the danger colour, announced as an alert.
    Error,
    /// A choice to make, in the accent colour.
    Question,
}

impl From<MessageKind> for DialogKind {
    fn from(kind: MessageKind) -> Self {
        match kind {
            MessageKind::Info => Self::Info,
            MessageKind::Warning => Self::Warning,
            MessageKind::Error => Self::Error,
        }
    }
}

impl DialogKind {
    fn image(self) -> Option<&'static VectorImage> {
        static IMAGES: LazyLock<[VectorImage; 4]> = LazyLock::new(|| {
            let grid = Rect::new(0.0, 0.0, GRID, GRID);
            [INFO, WARNING, ERROR, QUESTION].map(|d| {
                VectorImage::stroked_path_data(d, grid, 2.0)
                    .unwrap_or_else(|_| VectorImage::empty(grid))
            })
        });
        let i = match self {
            Self::Plain => return None,
            Self::Info => 0,
            Self::Warning => 1,
            Self::Error => 2,
            Self::Question => 3,
        };
        Some(&IMAGES[i])
    }

    fn tone(self, palette: &Palette) -> Color {
        match self {
            Self::Plain => palette.text,
            Self::Info | Self::Question => palette.accent,
            Self::Warning => palette.warning,
            Self::Error => palette.danger,
        }
    }

    fn role(self) -> Role {
        match self {
            Self::Warning | Self::Error => Role::AlertDialog,
            _ => Role::Dialog,
        }
    }
}

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
    kind: DialogKind,
    icon: Option<&'t VectorImage>,
}

impl<'t> Dialog<'t> {
    /// A dialog titled `title` with an OK button.
    pub const fn new(title: &'t str) -> Self {
        Self {
            title,
            message: "",
            buttons: &["OK"],
            kind: DialogKind::Plain,
            icon: None,
        }
    }

    /// The body text. A message too long for the screen scrolls in the card.
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

    /// What the dialog is about: its built-in icon, its tone and how it is announced. `Plain` by
    /// default.
    #[must_use]
    pub const fn kind(mut self, kind: DialogKind) -> Self {
        self.kind = kind;
        self
    }

    /// Draws `icon` in place of the kind's built-in one. Its colour is the kind's tone.
    #[must_use]
    pub const fn icon(mut self, icon: &'t VectorImage) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Shows it centred on the canvas. `id_of` names each button, and once more, past the last,
    /// the message when it scrolls.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id_of: impl Fn(usize) -> Id,
    ) -> AureaResult<DialogAnswer> {
        let message = self.message;
        let text = Label::new(message, ui.theme.palette.dim)
            .overflow(Overflow::Wrap(ui.theme.text_size * 1.4));
        let width = inner_width(ui);
        let full = if message.is_empty() {
            0.0
        } else {
            text.height(ui, width)
        };
        let id = id_of(0);
        let scroll = id_of(self.buttons.len());
        self.draw(ui, &id_of, full, |ui, rect| {
            if message.is_empty() {
                return Ok(());
            }
            ui.accessible_text(id, MESSAGE, rect, message);
            if full <= rect.height {
                return text.paint(ui, rect);
            }
            let mut offset = ui.animated(scroll, MESSAGE, SCROLL).unwrap_or(0.0);
            ScrollArea::new(&mut offset, full)
                .show(ui, scroll, rect, |ui, content, _| text.paint(ui, content))?;
            ui.set_animated(scroll, MESSAGE, SCROLL, offset);
            Ok(())
        })
    }

    /// Shows it centred on the canvas, with `body` drawn where the message goes, in a strip
    /// `body_height` tall. The strip is cut short when the card would not fit the screen. The
    /// keyboard stays among `body` and the buttons, so a field in `body` keeps it. Enter in a
    /// field takes the last button, and Escape dismisses. `id_of` names each button.
    pub fn show_with<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id_of: impl Fn(usize) -> Id,
        body_height: f32,
        body: impl FnOnce(&mut Ui<'_, Id>, Rect) -> AureaResult<()>,
    ) -> AureaResult<DialogAnswer> {
        self.draw(ui, &id_of, body_height, body)
    }

    /// The icon this dialog draws: the one given, else the kind's built-in one.
    fn mark(&self, palette: &Palette) -> Option<IconMark<'t>> {
        let image = self.icon.or(self.kind.image())?;
        Some(IconMark::new(image, self.kind.tone(palette)))
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
        let mark = self.mark(&ui.theme.palette);
        let at = layout(ui, body_height, mark.is_some());
        ui.interact(id, BACKDROP, at.canvas, Sense::Hover, true);
        ui.take_wheel();
        ui.fill(at.canvas, ui.theme.palette.shadow)?;
        ui.accessible_modal();

        ui.interact(id, CARD, at.card, Sense::Hover, true);
        ui.accessible_dialog(id, CARD, self.kind.role(), self.title, self.message);
        card(ui.theme).paint(ui, at.card)?;
        if let Some(mark) = mark {
            Component::paint(&mark, ui, at.icon)?;
        }
        ui.accessible_text(id, TITLE, at.title, self.title);
        Label::new(self.title, ui.theme.palette.text)
            .strong()
            .paint(ui, at.title)?;
        body(ui, at.body)?;

        let mut answer = DialogAnswer::Open;
        let last = self.buttons.len().saturating_sub(1);
        let row = ui.theme.control_height;
        let mut right = at.foot.right();
        for (i, label) in self.buttons.iter().enumerate().rev() {
            let button = if i == last {
                Button::new(label).primary()
            } else {
                Button::new(label)
            };
            let wide = button.width(ui);
            let cell = Rect::new(right - wide, at.foot.bottom() - row, wide, row);
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

/// The space around and inside the card, from the theme.
fn inset<Id: WidgetId>(ui: &Ui<'_, Id>) -> f32 {
    ui.theme.padding * 1.6
}

fn title_height<Id: WidgetId>(ui: &Ui<'_, Id>) -> f32 {
    ui.theme.text_size * 1.5
}

/// The gap above the buttons, which is there only with a body.
fn gap<Id: WidgetId>(ui: &Ui<'_, Id>, body: f32) -> f32 {
    if body > 0.0 { ui.theme.padding } else { 0.0 }
}

fn card_width<Id: WidgetId>(ui: &Ui<'_, Id>) -> f32 {
    WIDTH.min(ui.ctx.width() as f32 - 32.0).max(0.0)
}

/// The width a message is set in: the card's, less its inset.
fn inner_width<Id: WidgetId>(ui: &Ui<'_, Id>) -> f32 {
    (card_width(ui) - inset(ui) * 2.0).max(0.0)
}

/// The tallest a body strip can be with the card's other rows and the margins kept.
fn room<Id: WidgetId>(ui: &Ui<'_, Id>, body: f32) -> f32 {
    let chrome = inset(ui) * 2.0
        + title_height(ui)
        + ui.theme.padding
        + gap(ui, body)
        + ui.theme.control_height;
    (ui.ctx.height() as f32 - MARGIN * 2.0 - chrome).max(0.0)
}

/// The rects a dialog lays out on the canvas. `icon` is empty without one.
struct Layout {
    canvas: Rect,
    card: Rect,
    icon: Rect,
    title: Rect,
    body: Rect,
    foot: Rect,
}

/// Where the dialog's card, icon, title, body strip and button row go, for a body `body_height`
/// tall, cut short to fit.
fn layout<Id: WidgetId>(ui: &Ui<'_, Id>, body_height: f32, icon: bool) -> Layout {
    let (width, height) = (ui.ctx.width() as f32, ui.ctx.height() as f32);
    let canvas = Rect::new(0.0, 0.0, width, height);
    let body_height = body_height.min(room(ui, body_height));
    let gap = gap(ui, body_height);
    let line = title_height(ui);
    let tall =
        inset(ui) * 2.0 + line + ui.theme.padding + body_height + gap + ui.theme.control_height;
    let card = canvas.centered(card_width(ui), tall);
    let (head, rest) = card.inset(inset(ui)).split_top(line, ui.theme.padding);
    let (body, foot) = rest.split_top(body_height, gap);
    let (icon_at, title) = if icon {
        let side = line + ui.theme.padding;
        (
            Rect::new(head.x, head.y, line, line),
            Rect::new(
                head.x + side,
                head.y,
                (head.width - side).max(0.0),
                head.height,
            ),
        )
    } else {
        (Rect::default(), head)
    };
    Layout {
        canvas,
        card,
        icon: icon_at,
        title,
        body,
        foot,
    }
}

/// What a [`Dialog`] draws, from [`Dialog::look`]: the backdrop, the card with its icon, title and
/// buttons, and the strip `body` is for, which the app draws its body in.
/// [`Look::paint_default`] draws the first four.
#[derive(Clone, Debug)]
pub struct DialogParts<'t, Id: WidgetId> {
    /// The title, strong, at the top of the card.
    pub title: &'t str,
    /// Where the title goes, beside the icon when there is one.
    pub title_at: Rect,
    /// The icon beside the title, in the kind's tone, when the dialog has one.
    pub icon: Option<IconMark<'t>>,
    /// Where the icon goes.
    pub icon_at: Rect,
    /// The whole canvas, which the shade covers.
    pub backdrop: Rect,
    /// The strip under the title for the app's body, the height it was given, or less when the
    /// card would not fit the screen.
    pub body: Rect,
    /// The buttons, left to right, each as its own look. The last is the primary one.
    pub buttons: Vec<Look<ButtonParts<'t>>>,
    /// The keyboard's fallback when nothing of the dialog has it: the last button.
    keyboard: Option<Key<Id>>,
    /// Where this dialog's keys begin in the keyboard order, so [`Look::answer`] takes them out.
    first: usize,
}

impl<'t> Dialog<'t> {
    /// Takes the input as [`Dialog::show_with`] does and works out what it would draw, without
    /// drawing. `body_height` is the strip under the title that `parts.body` is: draw the body
    /// in it after this call, then [`Look::answer`] last, once the body has taken its input.
    ///
    /// The buttons take their input before the body, so the Tab order comes to them first, where
    /// `show_with` puts the body first.
    ///
    /// ```rust,no_run
    /// # use aurea::ui::{Dialog, DialogAnswer, TextField, Ui};
    /// # fn f(ui: &mut Ui<'_, u8>, name: &mut String) -> aurea::AureaResult<DialogAnswer> {
    /// let dialog = Dialog::new("Name?")
    ///     .buttons(&["Cancel", "Save"])
    ///     .look(ui, |i| u8::try_from(i).unwrap_or(0), 40.0);
    /// dialog.paint_default(ui)?;
    /// TextField::new(name).show(ui, 9, dialog.parts.body)?;
    /// Ok(dialog.answer(ui))
    /// # }
    /// ```
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id_of: impl Fn(usize) -> Id,
        body_height: f32,
    ) -> Look<DialogParts<'t, Id>> {
        let id = id_of(0);
        let first = ui.state.order.len();
        ui.state.text_submitted = false;
        ui.state.text_cancelled = false;
        let mark = self.mark(&ui.theme.palette);
        let at = layout(ui, body_height, mark.is_some());
        ui.interact(id, BACKDROP, at.canvas, Sense::Hover, true);
        ui.take_wheel();
        ui.accessible_modal();
        let input = ui.interact(id, CARD, at.card, Sense::Hover, true);
        ui.accessible_dialog(id, CARD, self.kind.role(), self.title, self.message);
        ui.accessible_text(id, TITLE, at.title, self.title);

        let last = self.buttons.len().saturating_sub(1);
        let row = ui.theme.control_height;
        let mut right = at.foot.right();
        let mut buttons = Vec::with_capacity(self.buttons.len());
        for (i, label) in self.buttons.iter().enumerate().rev() {
            let button = if i == last {
                Button::new(label).primary()
            } else {
                Button::new(label)
            };
            let wide = button.width(ui);
            let cell = Rect::new(right - wide, at.foot.bottom() - row, wide, row);
            right = cell.x - ui.theme.padding / 2.0;
            buttons.push(button.look(ui, id_of(i), cell));
        }
        buttons.reverse();

        Look {
            rect: at.card,
            state: Eased {
                enabled: 1.0,
                ..Eased::default()
            },
            held_at: None,
            response: Response::new(at.card, input),
            parts: DialogParts {
                title: self.title,
                title_at: at.title,
                icon: mark,
                icon_at: at.icon,
                backdrop: at.canvas,
                body: at.body,
                buttons,
                keyboard: (!self.buttons.is_empty()).then(|| Key::new(id_of(last), 0)),
                first,
            },
        }
    }
}

impl<Id: WidgetId> Look<DialogParts<'_, Id>> {
    /// The backdrop, the card, the title and the buttons, in the order [`Dialog::show_with`]
    /// draws them around its body.
    pub fn paint_default(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_backdrop(ui)?;
        self.paint_card(ui)?;
        self.paint_title(ui)?;
        self.paint_buttons(ui)
    }

    /// The shade over the canvas.
    pub fn paint_backdrop(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        ui.fill(self.parts.backdrop, ui.theme.palette.shadow)
    }

    /// The card.
    pub fn paint_card(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        card(ui.theme).paint(ui, self.rect)
    }

    /// The icon, if any, and the title, strong, at the top of the card.
    pub fn paint_title(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        if let Some(icon) = self.parts.icon {
            Component::paint(&icon, ui, self.parts.icon_at)?;
        }
        Label::new(self.parts.title, ui.theme.palette.text)
            .strong()
            .paint(ui, self.parts.title_at)
    }

    /// The buttons, the last first, as [`Dialog::show_with`] draws them.
    pub fn paint_buttons(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        for button in self.parts.buttons.iter().rev() {
            button.paint_default(ui)?;
        }
        Ok(())
    }

    /// The answer, once the body has taken its input: the button clicked, Enter in a field, or
    /// Escape. It takes the keyboard back into the dialog when nothing of it has it, and clears
    /// this dialog's keys from the order.
    pub fn answer(&self, ui: &mut Ui<'_, Id>) -> DialogAnswer {
        let parts = &self.parts;
        let mut answer = DialogAnswer::Open;
        for (i, button) in parts.buttons.iter().enumerate().rev() {
            if button.response.clicked {
                answer = DialogAnswer::Button(i);
            }
        }
        let last = parts.buttons.len().saturating_sub(1);
        if ui.state.text_submitted && !parts.buttons.is_empty() {
            answer = DialogAnswer::Button(last);
        }
        if ui.state.escaped || ui.state.text_cancelled {
            answer = DialogAnswer::Dismissed;
        }
        // the keyboard stays here, not with what is under the dialog.
        ui.state.order.drain(..parts.first);
        let ours = ui.state.focus.is_some_and(|f| ui.state.order.contains(&f));
        if let Some(key) = parts.keyboard
            && !ours
            && !ui.state.popup_open()
        {
            ui.state.give_focus(Some(key));
        }
        if answer != DialogAnswer::Open {
            ui.mark_changed();
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Color;
    #[cfg(feature = "ui-text-edit")]
    use crate::ui::TextArea;
    use crate::ui::TextField;
    use crate::ui::rig::Rig;
    use crate::ui::theme::Theme;
    use crate::ui::widget::button::Button;
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
    #[cfg(feature = "ui-text-edit")]
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

    #[test]
    fn the_app_draws_its_own_card_and_the_buttons_still_answer() {
        let mut rig = Rig::new(400, 300);
        let draw = |rig: &mut Rig| {
            rig.frame(|ui| {
                let dialog = Dialog::new("Sure?").buttons(&["No", "Yes"]).look(
                    ui,
                    |i| u8::try_from(i).unwrap_or(0),
                    0.0,
                );
                ui.fill(dialog.rect, Color::rgb(0, 200, 0)).expect("card");
                (dialog.answer(ui), dialog.parts.buttons[1].rect)
            })
        };
        let (answer, yes) = draw(&mut rig);
        assert_eq!(answer, DialogAnswer::Open);
        assert_eq!(rig.pixel(200, 150), [0, 200, 0, 255], "the app's card");
        rig.click(yes.center().x, yes.center().y);
        assert_eq!(draw(&mut rig).0, DialogAnswer::Button(1));
    }

    #[test]
    fn a_kind_names_its_icon_and_how_it_is_announced() {
        assert_eq!(DialogKind::from(MessageKind::Error), DialogKind::Error);
        assert_eq!(DialogKind::default(), DialogKind::Plain);
        assert!(DialogKind::Plain.image().is_none());
        for kind in [
            DialogKind::Info,
            DialogKind::Warning,
            DialogKind::Error,
            DialogKind::Question,
        ] {
            assert!(kind.image().is_some(), "{kind:?} has an icon");
        }
        assert_eq!(DialogKind::Warning.role(), Role::AlertDialog);
        assert_eq!(DialogKind::Error.role(), Role::AlertDialog);
        assert_eq!(DialogKind::Question.role(), Role::Dialog);
    }

    #[test]
    fn a_tall_body_is_cut_to_fit_and_the_buttons_stay_on_the_canvas() {
        let mut rig = Rig::new(400, 300);
        let parts = rig.frame(|ui| {
            let dialog = Dialog::new("Long")
                .kind(DialogKind::Warning)
                .buttons(&["No", "Yes"])
                .look(ui, |i| u8::try_from(i).unwrap_or(0), 1000.0);
            (dialog.rect, dialog.parts.body, dialog.parts.buttons[1].rect)
        });
        let (card, body, yes) = parts;
        assert!(body.height < 1000.0, "the body is cut short");
        assert!(card.y >= MARGIN - 0.5 && card.bottom() <= 300.0 - MARGIN + 0.5);
        assert!(yes.bottom() <= card.bottom() && yes.y > body.bottom());
    }

    #[test]
    fn a_long_message_scrolls_and_the_buttons_still_answer() {
        let mut rig = Rig::new(400, 300);
        let long = "a long line of text that wraps and wraps again. ".repeat(40);
        let show_long = |rig: &mut Rig| {
            rig.frame(|ui| {
                Dialog::new("Terms")
                    .message(&long)
                    .buttons(&["No", "Yes"])
                    .show(ui, |i| u8::try_from(i).unwrap_or(0))
                    .expect("draw")
            })
        };
        assert_eq!(show_long(&mut rig), DialogAnswer::Open);
        let yes = rig.frame(|ui| {
            Dialog::new("Terms")
                .buttons(&["No", "Yes"])
                .look(ui, |i| u8::try_from(i).unwrap_or(0), 1000.0)
                .parts
                .buttons[1]
                .rect
        });
        rig.click(yes.center().x, yes.center().y);
        assert_eq!(show_long(&mut rig), DialogAnswer::Button(1));
    }
}
