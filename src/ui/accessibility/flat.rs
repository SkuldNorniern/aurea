//! An AccessKit tree as the flat list of elements a screen reader without trees takes (iOS
//! VoiceOver): reading order, frame, words and traits per element.

use std::collections::HashMap;

use accesskit::{Action, Node, NodeId, Role, Toggled, TreeUpdate};

/// Traits, matched to UIKit's by the native side.
pub(crate) mod traits {
    pub const BUTTON: u32 = 1;
    pub const LINK: u32 = 1 << 1;
    pub const HEADER: u32 = 1 << 2;
    pub const STATIC_TEXT: u32 = 1 << 3;
    pub const ADJUSTABLE: u32 = 1 << 4;
    pub const SELECTED: u32 = 1 << 5;
    pub const NOT_ENABLED: u32 = 1 << 6;
    pub const TEXT_FIELD: u32 = 1 << 7;
    pub const IMAGE: u32 = 1 << 8;
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Element {
    pub id: u64,
    /// x, y, width, height in the tree's coordinates.
    pub frame: [f32; 4],
    pub traits: u32,
    pub label: String,
    pub value: String,
    pub hint: String,
    /// Activating it clicks; otherwise it takes focus.
    pub clicks: bool,
}

/// The tree's elements in reading order, depth first from its root. Containers with nothing to
/// say or do are passed through, and hidden nodes are left out with what is under them.
pub(crate) fn flatten(update: &TreeUpdate) -> Vec<Element> {
    let nodes: HashMap<NodeId, &Node> = update.nodes.iter().map(|(id, n)| (*id, n)).collect();
    let mut out = Vec::new();
    if let Some(root) = update.tree.as_ref().map(|t| t.root)
        && let Some(node) = nodes.get(&root)
    {
        // the root is the screen itself, not something to read
        for child in node.children() {
            walk(*child, &nodes, &mut out);
        }
    }
    out
}

fn walk(id: NodeId, nodes: &HashMap<NodeId, &Node>, out: &mut Vec<Element>) {
    let Some(node) = nodes.get(&id) else {
        return;
    };
    if node.is_hidden() {
        return;
    }
    if let Some(element) = element(id, node) {
        out.push(element);
    }
    for child in node.children() {
        walk(*child, nodes, out);
    }
}

fn element(id: NodeId, node: &Node) -> Option<Element> {
    let bounds = node.bounds()?;
    let label = node.label().unwrap_or_default().to_owned();
    let value = value(node);
    let acts = [Action::Click, Action::Focus, Action::Increment]
        .iter()
        .any(|a| node.supports_action(*a));
    if label.is_empty() && value.is_empty() && !acts {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    let frame = [
        bounds.x0 as f32,
        bounds.y0 as f32,
        (bounds.x1 - bounds.x0) as f32,
        (bounds.y1 - bounds.y0) as f32,
    ];
    Some(Element {
        id: id.0,
        frame,
        traits: traits_of(node),
        label,
        value,
        hint: node.description().unwrap_or_default().to_owned(),
        clicks: node.supports_action(Action::Click),
    })
}

/// The value as read: its text, else a number as a share of its range.
fn value(node: &Node) -> String {
    if let Some(text) = node.value() {
        return text.to_owned();
    }
    let Some(v) = node.numeric_value() else {
        return String::new();
    };
    match (node.min_numeric_value(), node.max_numeric_value()) {
        (Some(lo), Some(hi)) if hi > lo => format!("{:.0}%", (v - lo) / (hi - lo) * 100.0),
        _ => {
            let text = format!("{v:.2}");
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        }
    }
}

fn traits_of(node: &Node) -> u32 {
    use traits::*;
    let mut t = match node.role() {
        Role::Button
        | Role::DefaultButton
        | Role::CheckBox
        | Role::Switch
        | Role::RadioButton
        | Role::Tab
        | Role::MenuItem
        | Role::MenuItemCheckBox
        | Role::MenuItemRadio
        | Role::ComboBox
        | Role::DisclosureTriangle => BUTTON,
        Role::Link => LINK,
        Role::Heading => HEADER,
        Role::Label | Role::Paragraph => STATIC_TEXT,
        Role::Slider | Role::SpinButton | Role::ScrollBar => ADJUSTABLE,
        Role::TextInput
        | Role::MultilineTextInput
        | Role::SearchInput
        | Role::PasswordInput
        | Role::EmailInput
        | Role::NumberInput
        | Role::PhoneNumberInput
        | Role::UrlInput => TEXT_FIELD,
        Role::Image => IMAGE,
        _ => 0,
    };
    if node.supports_action(Action::Increment) && node.supports_action(Action::Decrement) {
        t |= ADJUSTABLE;
    }
    if node.toggled() == Some(Toggled::True) || node.is_selected() == Some(true) {
        t |= SELECTED;
    }
    if node.is_disabled() {
        t |= NOT_ENABLED;
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Rect, TreeId, TreeInfo};

    fn node(role: Role, label: &str, bounds: (f64, f64, f64, f64)) -> Node {
        let mut n = Node::new(role);
        n.set_label(label);
        n.set_bounds(Rect::new(bounds.0, bounds.1, bounds.2, bounds.3));
        n
    }

    fn update(nodes: Vec<(u64, Node)>) -> TreeUpdate {
        TreeUpdate {
            nodes: nodes.into_iter().map(|(id, n)| (NodeId(id), n)).collect(),
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(1),
        }
    }

    #[test]
    fn controls_come_in_reading_order_with_their_traits() {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(3), NodeId(2), NodeId(4)]);
        let mut tap = node(Role::Button, "Tap", (20.0, 40.0, 120.0, 80.0));
        tap.add_action(Action::Click);
        let mut level = node(Role::Slider, "Level", (20.0, 90.0, 220.0, 110.0));
        level.set_numeric_value(0.6);
        level.set_min_numeric_value(0.0);
        level.set_max_numeric_value(1.0);
        level.add_action(Action::Increment);
        level.add_action(Action::Decrement);
        let mut mute = node(Role::Switch, "Mute", (20.0, 120.0, 220.0, 150.0));
        mute.set_toggled(Toggled::True);
        mute.set_disabled();
        let got = flatten(&update(vec![(1, root), (2, tap), (3, level), (4, mute)]));
        let ids: Vec<u64> = got.iter().map(|e| e.id).collect();
        assert_eq!(ids, [3, 2, 4], "the root's order, not the ids'");
        assert_eq!(got[0].value, "60%");
        assert_eq!(got[0].traits, traits::ADJUSTABLE);
        assert_eq!(got[1].frame, [20.0, 40.0, 100.0, 40.0]);
        assert_eq!(got[1].traits, traits::BUTTON);
        assert!(got[1].clicks);
        assert_eq!(
            got[2].traits,
            traits::BUTTON | traits::SELECTED | traits::NOT_ENABLED
        );
        assert!(!got[2].clicks, "focus is what activating it does");
    }

    #[test]
    fn silent_containers_pass_their_children_through_and_hidden_ones_drop_them() {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(2), NodeId(4)]);
        let mut group = Node::new(Role::GenericContainer);
        group.set_bounds(Rect::new(0.0, 0.0, 10.0, 10.0));
        group.set_children(vec![NodeId(3)]);
        let mut hidden = node(Role::Button, "Gone", (0.0, 0.0, 5.0, 5.0));
        hidden.set_hidden();
        hidden.set_children(vec![NodeId(5)]);
        let got = flatten(&update(vec![
            (1, root),
            (2, group),
            (3, node(Role::Label, "Inside", (1.0, 1.0, 9.0, 9.0))),
            (4, hidden),
            (5, node(Role::Label, "Under gone", (0.0, 0.0, 5.0, 5.0))),
        ]));
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0].label.as_str(), got[0].traits),
            ("Inside", traits::STATIC_TEXT)
        );
    }

    #[test]
    fn plain_numbers_read_without_trailing_zeros() {
        let mut n = Node::new(Role::SpinButton);
        n.set_numeric_value(2.5);
        assert_eq!(value(&n), "2.5");
        n.set_numeric_value(3.0);
        assert_eq!(value(&n), "3");
    }
}
