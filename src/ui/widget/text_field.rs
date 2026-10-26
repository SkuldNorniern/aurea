//! A one-line text field: a [`Surface`], a [`FocusRing`] and [`EditableText`].

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::{Action, Role};
use crate::ui::component::{EditableText, FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Mark, MarkShape, Surface};
#[cfg(feature = "accessibility")]
use crate::ui::state::Key;
use crate::ui::state::{Channel, Sense, WidgetId};
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
        }
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

impl<Id: WidgetId> Widget<Id> for TextField<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let input = ui.interact(id, 0, rect, Sense::Text, self.enabled);
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
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
        let border = style
            .border
            .mix(style.active_border, lit)
            .fade(state.enabled);
        let width = ui.theme.border_width.max(lit * 1.5);
        Surface::new(style.face.at(state))
            .radius(style.radius)
            .border(width, border)
            .paint(ui, rect)?;
        let mut inner = rect.inset_xy(style.padding, 0.0);
        let mut cleared = false;
        if self.clearable && !self.text.is_empty() && self.enabled {
            let side = (rect.height - 8.0).clamp(0.0, rect.width / 2.0);
            let (button, rest) = inner.split_right(side, 4.0);
            let button = Rect::new(button.x, rect.y + (rect.height - side) / 2.0, side, side);
            inner = rest;
            let clear = ui.interact(id, CLEAR_PART, button, Sense::Click, true);
            if clear.clicked {
                self.text.clear();
                ui.state.focus(id, 0);
                cleared = true;
            }
            let eased = ui.eased(id, CLEAR_PART, clear);
            if clear.hovered || clear.held {
                ui.round(button, side / 2.0, ui.theme.palette.hover.fade(eased.hover))?;
            }
            Mark::new(MarkShape::Cross, ui.theme.palette.dim)
                .paint(ui, button.inset(side * 0.3))?;
        }
        let outcome = EditableText::new(self.text)
            .placeholder(self.placeholder)
            .password(self.password)
            .rules(self.rules)
            .commit_on_blur(self.commit_on_blur)
            .pass_vertical_keys(self.pass_vertical_keys)
            .select_all_on_focus(self.select_all_on_focus)
            .show(ui, (id, 0), inner, input)?;
        #[cfg(feature = "accessibility")]
        {
            ui.accessible_description(
                id,
                0,
                if self.password {
                    Role::PasswordInput
                } else {
                    Role::TextInput
                },
                self.placeholder,
                (!self.password).then_some(self.text.as_str()),
            );
            if let Some(bridge) = &mut ui.state.accessibility
                && let Some(node) = bridge.node(Key::new(id, 0))
            {
                node.add_action(Action::SetValue);
                if self.password {
                    node.clear_value();
                }
            }
        }
        let mut response = Response::new(rect, input);
        response.changed = outcome.changed || cleared;
        response.submitted = outcome.submitted;
        response.cancelled = outcome.cancelled;
        response.editing_started = outcome.editing_started;
        response.focus_lost = outcome.focus_lost;
        response.committed = outcome.committed;
        response.focused = input.focused && !outcome.cancelled;
        Ok(response)
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
