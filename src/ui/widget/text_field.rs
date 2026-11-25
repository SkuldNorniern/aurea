//! A one-line text field: a [`Surface`], a [`FocusRing`] and [`EditableText`].

use super::Widget;
use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::accessibility::{Action, Role};
use crate::ui::component::{EditableText, FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Mark, MarkShape, Surface};
use crate::ui::state::{Channel, Interaction, Key, Sense, WidgetId};
use crate::ui::text_edit::Rules;
use crate::ui::theme::Theme;

/// How a text field looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldStyle {
    /// Fill colours by state.
    pub face: StateColors,
    /// Border colour.
    pub border: Color,
    /// The border while typing.
    pub active_border: Color,
    /// Corner radius.
    pub radius: f32,
    /// Space between the box and the text.
    pub padding: f32,
}

impl FieldStyle {
    /// A field style from the theme.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            face: StateColors::new(p.raised, p.raised.mix(p.hover, 0.6), p.raised),
            border: p.border,
            active_border: p.accent,
            radius: theme.radius,
            padding: theme.padding,
        }
    }
}

/// What a [`TextField`] draws, from [`TextField::look`]: the chrome and the clear button, which the
/// app draws, then the text, which [`Look::paint_text`] edits and draws after them.
pub struct TextFieldParts<'v> {
    /// Whether it has focus, after this frame's input.
    pub focused: bool,
    /// How far focus has lit the border, 0 unfocused to 1 focused.
    pub lit: f32,
    /// The placeholder, shown while the field is empty.
    pub placeholder: &'v str,
    /// Whether the field is empty, so the placeholder shows; after this frame's clear.
    pub empty: bool,
    /// Whether it shows a dot for each character.
    pub password: bool,
    /// Where the text goes: inside the padding and left of the clear button.
    pub text: Rect,
    /// The clear button's rect, while it is shown.
    pub clear: Option<Rect>,
    /// How far the clear button's round hover is lit, while the pointer is on it or it is held.
    pub clear_hover: Option<f32>,
    /// Whether this frame's click on the clear button emptied the field.
    pub cleared: bool,
    /// The style it draws with, the theme's unless one was given.
    pub style: FieldStyle,
    /// What [`Look::paint_text`] types into and what it takes.
    edit: Editing<'v>,
}

/// The text a field edits, and how: kept from [`TextField::look`] for [`Look::paint_text`].
struct Editing<'v> {
    text: &'v mut String,
    rules: Rules<'v>,
    commit_on_blur: bool,
    pass_vertical_keys: bool,
    select_all_on_focus: bool,
    input: Interaction,
}

/// A field to type one line into.
///
/// ```rust,no_run
/// # use aurea::ui::{TextField, Ui};
/// # fn f(ui: &mut Ui<'_, u8>, name: &mut String) -> aurea::AureaResult<()> {
/// let at = aurea::Rect::new(0.0, 0.0, 220.0, 30.0);
/// let field = TextField::new(name).placeholder("Name").max_chars(40);
/// if field.show(ui, 1, at)?.submitted {
///     // Enter
/// }
/// # Ok(())
/// # }
/// ```
pub struct TextField<'v> {
    text: &'v mut String,
    placeholder: &'v str,
    password: bool,
    rules: Rules<'v>,
    style: Option<FieldStyle>,
    enabled: bool,
    commit_on_blur: bool,
    pass_vertical_keys: bool,
    select_all_on_focus: bool,
    clearable: bool,
    keeps_tab: bool,
}

impl<'v> TextField<'v> {
    /// A field editing `text`.
    pub fn new(text: &'v mut String) -> Self {
        Self {
            text,
            placeholder: "",
            password: false,
            rules: Rules::default(),
            style: None,
            enabled: true,
            commit_on_blur: false,
            pass_vertical_keys: false,
            select_all_on_focus: false,
            clearable: false,
            keeps_tab: false,
        }
    }

    /// Hands Tab and Shift+Tab to the field, read through `Ui::keys`, instead of moving focus.
    pub fn keeps_tab(mut self, keeps: bool) -> Self {
        self.keeps_tab = keeps;
        self
    }

    /// Shown, faint, while the field is empty.
    pub fn placeholder(mut self, placeholder: &'v str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Shows a dot for each character, and keeps the text off the clipboard.
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    /// Most characters the field takes; a longer paste is cut to fit.
    pub fn max_chars(mut self, max: usize) -> Self {
        self.rules.max_chars = Some(max);
        self
    }

    /// Only characters `accept` says yes to go in, as for a number field.
    pub fn accept(mut self, accept: &'v dyn Fn(char) -> bool) -> Self {
        self.rules.accept = Some(accept);
        self
    }

    /// Sets the look.
    pub fn style(mut self, style: FieldStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Emit `committed` when dirty text loses focus.
    pub fn commit_on_blur(mut self, commit: bool) -> Self {
        self.commit_on_blur = commit;
        self
    }

    /// Sets whether it takes input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Leaves Up, Down, PageUp and PageDown to the app, read through `Ui::keys`.
    pub fn pass_vertical_keys(mut self, pass: bool) -> Self {
        self.pass_vertical_keys = pass;
        self
    }

    /// Selects all text when the field gains focus.
    pub fn select_all_on_focus(mut self, select: bool) -> Self {
        self.select_all_on_focus = select;
        self
    }

    /// Shows a × at the right end while there is text; clicking it empties the field and keeps
    /// it focused, as in a find or filter box.
    pub fn clearable(mut self, clearable: bool) -> Self {
        self.clearable = clearable;
        self
    }

    /// Draws it in `rect` and reports the input.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

/// The part of the clear button.
const CLEAR_PART: u16 = 1;

impl<'v> TextField<'v> {
    /// Takes the input for the field and its clear button, and works out what it would draw,
    /// without drawing: the app draws the chrome and clear button, then [`Look::paint_text`]
    /// draws and edits the text, as [`TextField::show`] does. The response's editing fields are
    /// set by `paint_text`.
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<TextFieldParts<'v>> {
        let input = ui.interact(id, 0, rect, Sense::Text, self.enabled);
        ui.state.keep_tab(Key::new(id, 0), self.keeps_tab);
        let state = ui.eased(id, 0, input);
        let style = self.style.unwrap_or_else(|| FieldStyle::new(ui.theme));
        let rate = ui.theme.motion.focus;
        let lit = ui.animate(
            id,
            0,
            Channel::Value,
            f32::from(u8::from(input.focused)),
            rate,
        );
        let mut inner = rect.inset_xy(style.padding, 0.0);
        let mut clear = None;
        let mut clear_hover = None;
        let mut cleared = false;
        if self.clearable && !self.text.is_empty() && self.enabled {
            let side = (rect.height - 8.0).clamp(0.0, (rect.width / 2.0).max(0.0));
            let (button, rest) = inner.split_right(side, 4.0);
            let button = Rect::new(button.x, rect.y + (rect.height - side) / 2.0, side, side);
            inner = rest;
            let hit = ui.interact(id, CLEAR_PART, button, Sense::Click, true);
            if hit.clicked {
                self.text.clear();
                ui.state.focus(id, 0);
                cleared = true;
            }
            let eased = ui.eased(id, CLEAR_PART, hit);
            if hit.hovered || hit.held {
                clear_hover = Some(eased.hover);
            }
            clear = Some(button);
        }
        let mut response = Response::new(rect, input);
        response.changed = cleared;
        Look {
            rect,
            state,
            held_at: ui.held_at(id, 0),
            response,
            parts: TextFieldParts {
                focused: input.focused,
                lit,
                placeholder: self.placeholder,
                empty: self.text.is_empty(),
                password: self.password,
                text: inner,
                clear,
                clear_hover,
                cleared,
                style,
                edit: Editing {
                    text: self.text,
                    rules: self.rules,
                    commit_on_blur: self.commit_on_blur,
                    pass_vertical_keys: self.pass_vertical_keys,
                    select_all_on_focus: self.select_all_on_focus,
                    input,
                },
            },
        }
    }
}

impl Look<TextFieldParts<'_>> {
    /// The field's own look: its chrome, then the clear button. The text is drawn by
    /// [`Look::paint_text`], after it.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_chrome(ui)?;
        self.paint_clear(ui)
    }

    /// Types into the field and draws its text, caret, selection and preedit, after the chrome as
    /// [`TextField::show`] does: call it once a frame, after the chrome is drawn. It sets the
    /// response's editing fields.
    pub fn paint_text<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, id: Id) -> AureaResult<()> {
        let parts = &mut self.parts;
        let edit = &mut parts.edit;
        let outcome = EditableText::new(&mut *edit.text)
            .placeholder(parts.placeholder)
            .password(parts.password)
            .rules(edit.rules)
            .commit_on_blur(edit.commit_on_blur)
            .pass_vertical_keys(edit.pass_vertical_keys)
            .select_all_on_focus(edit.select_all_on_focus)
            .show(ui, (id, 0), parts.text, edit.input)?;
        {
            ui.accessible_description(
                id,
                0,
                if parts.password {
                    Role::PasswordInput
                } else {
                    Role::TextInput
                },
                parts.placeholder,
                (!parts.password).then_some(edit.text.as_str()),
            );
            ui.with_accessible(id, 0, |node| {
                node.add_action(Action::SetValue);
                if parts.password {
                    node.clear_value();
                }
            });
        }
        let response = &mut self.response;
        response.changed = outcome.changed || parts.cleared;
        response.submitted = outcome.submitted;
        response.cancelled = outcome.cancelled;
        response.editing_started = outcome.editing_started;
        response.focus_lost = outcome.focus_lost;
        response.committed = outcome.committed;
        response.focused = edit.input.focused && !outcome.cancelled;
        Ok(())
    }

    /// The focus ring, the face and its border.
    pub fn paint_chrome<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let (state, style) = (self.state, &self.parts.style);
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, self.rect)?;
        let border = style
            .border
            .mix(style.active_border, self.parts.lit)
            .fade(state.enabled);
        let width = ui.theme.border_width.max(self.parts.lit * 1.5);
        Surface::new(style.face.at(state))
            .radius(style.radius)
            .border(width, border)
            .paint(ui, self.rect)
    }

    /// The clear button's × in its round hover, if it is shown.
    pub fn paint_clear<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let Some(button) = self.parts.clear else {
            return Ok(());
        };
        if let Some(hover) = self.parts.clear_hover {
            ui.round(
                button,
                button.width / 2.0,
                ui.theme.palette.hover.fade(hover),
            )?;
        }
        Mark::new(MarkShape::Cross, ui.theme.palette.dim)
            .paint(ui, button.inset(button.width * 0.3))
    }
}

impl<Id: WidgetId> Widget<Id> for TextField<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let mut look = self.look(ui, id, rect);
        look.paint_default(ui)?;
        look.paint_text(ui, id)?;
        Ok(look.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;
    use crate::ui::state::Key;
    use crate::window::KeyCode;

    fn field(
        ui: &mut Ui<'_, u8>,
        text: &mut String,
        pass: bool,
        select: bool,
        seen: &mut Vec<KeyCode>,
    ) -> Response {
        seen.extend(ui.keys().map(|(key, _)| key));
        TextField::new(text)
            .pass_vertical_keys(pass)
            .select_all_on_focus(select)
            .show(ui, 1, Rect::new(0.0, 0.0, 160.0, 40.0))
            .expect("field")
    }

    #[test]
    fn look_paints_the_chrome_and_still_clears() {
        let mut rig = Rig::new(240, 160);
        let mut text = "abc".to_string();
        let rect = Rect::new(0.0, 0.0, 160.0, 40.0);
        let show = |rig: &mut Rig, text: &mut String| {
            rig.frame(|ui| {
                let mut field = TextField::new(text).clearable(true).look(ui, 1, rect);
                ui.fill(field.rect, Color::rgb(0, 200, 0)).expect("chrome");
                field.paint_clear(ui).expect("clear");
                field.paint_text(ui, 1).expect("text");
                field.response
            })
        };
        show(&mut rig, &mut text);
        assert_eq!(
            rig.pixel(120, 20),
            [0, 200, 0, 255],
            "the app's look, not the default"
        );
        let x = f64::from(rect.right() - FieldStyle::new(&rig.theme).padding - 16.0);
        rig.at(x, 20.0);
        show(&mut rig, &mut text);
        rig.button(x, 20.0, true);
        show(&mut rig, &mut text);
        rig.button(x, 20.0, false);
        let response = show(&mut rig, &mut text);
        assert!(text.is_empty() && response.changed);
        assert_eq!(rig.state.focus, Some(Key::new(1, 0)));
    }

    #[test]
    fn the_clear_button_empties_the_field_and_keeps_it_focused() {
        let mut rig = Rig::new(240, 160);
        let mut text = "abc".to_string();
        let rect = Rect::new(0.0, 0.0, 160.0, 40.0);
        let show = |ui: &mut Ui<'_, u8>, text: &mut String| {
            TextField::new(text)
                .clearable(true)
                .show(ui, 1, rect)
                .expect("field")
        };
        rig.frame(|ui| show(ui, &mut text));
        let x = f64::from(rect.right() - FieldStyle::new(&rig.theme).padding - 16.0);
        rig.at(x, 20.0);
        rig.frame(|ui| show(ui, &mut text));
        rig.button(x, 20.0, true);
        rig.frame(|ui| show(ui, &mut text));
        rig.button(x, 20.0, false);
        let response = rig.frame(|ui| show(ui, &mut text));
        assert!(text.is_empty() && response.changed);
        assert_eq!(rig.state.focus, Some(Key::new(1, 0)));
    }

    #[test]
    fn pass_vertical_keys_leaves_them_to_the_app() {
        let mut rig = Rig::new(240, 160);
        let mut text = "abc".to_string();
        let mut seen = Vec::new();
        rig.state.focus(1, 0);
        rig.frame(|ui| field(ui, &mut text, true, false, &mut seen));
        rig.key(KeyCode::Home);
        rig.frame(|ui| field(ui, &mut text, true, false, &mut seen));
        rig.key(KeyCode::Right);
        rig.frame(|ui| field(ui, &mut text, true, false, &mut seen));
        assert_eq!(rig.state.edit.caret(), 1);
        for key in [
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::PageUp,
            KeyCode::PageDown,
        ] {
            seen.clear();
            rig.key(key);
            rig.frame(|ui| field(ui, &mut text, true, false, &mut seen));
            assert_eq!(seen, [key]);
            assert_eq!(rig.state.edit.caret(), 1, "{key:?} moved the caret");
        }
    }

    #[test]
    fn down_moves_the_caret_without_pass_vertical_keys() {
        let mut rig = Rig::new(240, 160);
        let mut text = "abc".to_string();
        let mut seen = Vec::new();
        rig.state.focus(1, 0);
        rig.frame(|ui| field(ui, &mut text, false, false, &mut seen));
        rig.key(KeyCode::Home);
        rig.frame(|ui| field(ui, &mut text, false, false, &mut seen));
        rig.key(KeyCode::Down);
        rig.frame(|ui| field(ui, &mut text, false, false, &mut seen));
        assert_eq!(rig.state.edit.caret(), 3);
    }

    #[test]
    fn keeps_tab_leaves_tab_to_the_field() {
        let mut rig = Rig::new(240, 160);
        let mut text = String::new();
        let mut seen = Vec::new();
        let show = |ui: &mut Ui<'_, u8>, text: &mut String, seen: &mut Vec<KeyCode>| {
            seen.extend(ui.keys().map(|(key, _)| key));
            TextField::new(text)
                .keeps_tab(true)
                .show(ui, 1, Rect::new(0.0, 0.0, 160.0, 40.0))
                .expect("field");
        };
        rig.state.focus(1, 0);
        rig.frame(|ui| show(ui, &mut text, &mut seen));
        rig.key(KeyCode::Tab);
        rig.frame(|ui| show(ui, &mut text, &mut seen));
        assert_eq!(seen, [KeyCode::Tab]);
        assert_eq!(rig.state.focus, Some(Key::new(1, 0)));
    }

    #[test]
    fn focus_at_end_puts_the_caret_last_and_selects_nothing() {
        let mut rig = Rig::new(240, 160);
        let mut text = "abc".to_string();
        let mut seen = Vec::new();
        rig.frame(|ui| field(ui, &mut text, false, false, &mut seen));
        rig.key(KeyCode::F1);
        rig.state.focus(1, 0);
        rig.frame(|ui| field(ui, &mut text, false, false, &mut seen));
        assert_eq!(rig.state.edit.selected(&text), "abc", "plain focus selects");
        rig.state.focus_at_end(1, 0);
        rig.frame(|ui| field(ui, &mut text, false, false, &mut seen));
        assert_eq!(rig.state.edit.caret(), 3);
        assert!(!rig.state.edit.has_selection());
        rig.type_text("d");
        rig.frame(|ui| field(ui, &mut text, false, false, &mut seen));
        assert_eq!(text, "abcd");
    }

    #[test]
    fn select_all_on_focus_selects_everything_when_editing_starts() {
        let mut rig = Rig::new(240, 160);
        let mut text = "hello".to_string();
        let mut seen = Vec::new();
        rig.frame(|ui| field(ui, &mut text, false, true, &mut seen));
        rig.state.give_focus(Some(Key::new(1, 0)));
        let response = rig.frame(|ui| field(ui, &mut text, false, true, &mut seen));
        assert!(response.editing_started);
        assert_eq!(rig.state.edit.selected(&text), "hello");
        rig.key(KeyCode::Left);
        rig.frame(|ui| field(ui, &mut text, false, true, &mut seen));
        assert!(
            !rig.state.edit.has_selection(),
            "selection survived later keys"
        );
    }
}
