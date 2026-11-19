//! Native accessibility for canvas controls. The types and the calls widgets make are the same in
//! every build; without the `accessibility` feature the calls do nothing.

#[cfg(all(feature = "accessibility", any(target_os = "ios", test)))]
mod flat;
#[cfg(feature = "accessibility")]
mod platform;
#[cfg(feature = "accessibility")]
mod tree;

#[cfg(not(feature = "accessibility"))]
mod off;

#[cfg(feature = "accessibility")]
pub(crate) use accesskit::Live;
#[cfg(feature = "accessibility")]
pub use accesskit::{Action, ActionData, Node, Role};
#[cfg(feature = "accessibility")]
pub(crate) use tree::{Bridge, Generation};

#[cfg(not(feature = "accessibility"))]
pub use off::{Action, ActionData, Node, Role};
#[cfg(not(feature = "accessibility"))]
pub(crate) use off::{Bridge, Generation, Live};

use crate::ui::component::editable::Typing;
use crate::ui::text_edit::{Direction, Rules, Step, TextEdit};

/// An action requested by an assistive technology for a custom control.
#[derive(Clone, Debug)]
pub struct AccessibleAction {
    /// The action requested.
    pub action: Action,
    /// Its data, if any.
    pub data: Option<ActionData>,
}

pub(crate) fn edit_value(
    edit: &mut TextEdit,
    text: &mut String,
    rules: Rules<'_>,
    actions: Vec<AccessibleAction>,
) -> Typing {
    let mut changed = false;
    for action in actions {
        if action.action == Action::SetValue
            && let Some(ActionData::Value(value)) = action.data
            && value.as_ref() != text
        {
            edit.set_preedit(text, "", None);
            edit.select_all(text);
            changed |= if value.is_empty() {
                edit.delete(text, Step::Char, Direction::Back)
            } else {
                edit.insert(text, &value, rules)
            };
        }
    }
    Typing {
        changed,
        moved: changed,
        ..Default::default()
    }
}

#[cfg(test)]
mod editing_tests {
    use super::*;

    fn value(value: &str) -> Vec<AccessibleAction> {
        vec![AccessibleAction {
            action: Action::SetValue,
            data: Some(ActionData::Value(value.into())),
        }]
    }

    #[test]
    fn assistive_value_updates_follow_field_rules_and_remain_undoable() {
        let mut text = "old".to_string();
        let mut edit = TextEdit::new();
        let rules = Rules {
            max_chars: Some(5),
            multiline: true,
            ..Rules::default()
        };
        assert!(edit_value(&mut edit, &mut text, rules, value("한국\r\nmore")).changed);
        assert_eq!(text, "한국\nmo");
        assert!(edit.undo(&mut text));
        assert_eq!(text, "old");
        assert!(edit_value(&mut edit, &mut text, rules, value("")).changed);
        assert!(text.is_empty());
        assert!(edit.undo(&mut text));
        assert_eq!(text, "old");
    }
}
