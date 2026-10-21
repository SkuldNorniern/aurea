//! A one-line text field: a [`Surface`], a [`FocusRing`] and [`EditableText`].

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::{Action, Role};
use crate::ui::component::{EditableText, FocusRing, StateColors};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Surface};
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
        let inner = rect.inset_xy(style.padding, 0.0);
        let outcome = EditableText::new(self.text)
            .placeholder(self.placeholder)
            .password(self.password)
            .rules(self.rules)
            .commit_on_blur(self.commit_on_blur)
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
        response.changed = outcome.changed;
        response.submitted = outcome.submitted;
        response.cancelled = outcome.cancelled;
        response.editing_started = outcome.editing_started;
        response.focus_lost = outcome.focus_lost;
        response.committed = outcome.committed;
        response.focused = input.focused && !outcome.cancelled;
        Ok(response)
    }
}
