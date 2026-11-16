//! A tree: a [`ListView`] of rows that open and close, for file browsers and outlines. The app owns
//! the nodes; only rows in sight are drawn, so it can load children when a key turns up in
//! `expanded`.

use std::collections::HashSet;

use super::look::Look;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape};
use crate::ui::state::WidgetId;
use crate::ui::theme::Theme;
use crate::ui::widget::scroll::{ListRow, ListView};
use crate::window::KeyCode;

/// How a tree looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeStyle {
    /// Fill of the pick.
    pub picked: Color,
    /// Fill under the pointer.
    pub hover: Color,
    /// Colour of the labels.
    pub text: Color,
    /// Colour of the chevrons.
    pub chevron: Color,
    /// How far each depth is set in.
    pub indent: f32,
    /// Row height.
    pub row: f32,
}

impl TreeStyle {
    /// Creates the default style from `theme`.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            picked: p.selection,
            hover: p.hover,
            text: p.text,
            chevron: p.dim,
            indent: 16.0,
            row: theme.control_height,
        }
    }
}

/// One row in sight of a [`TreeView`]: the node, how deep, and whether it can open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeRow<'t> {
    /// The node's key, the same every frame.
    pub key: u64,
    /// How deep it is; a root is 0.
    pub depth: u16,
    /// What it says.
    pub label: &'t str,
    /// It can open, even with no children loaded yet.
    pub branch: bool,
}

/// What a [`TreeView`] row draws, for a painter given to [`TreeView::show_painted`]. The row's
/// [`Look::rect`] is the whole row; [`Look::chevron`] and [`Look::body`] split it.
#[derive(Clone, Copy, Debug)]
pub struct TreeRowParts<'t> {
    /// The node in this row.
    pub row: TreeRow<'t>,
    /// It is the tree's pick.
    pub picked: bool,
    /// The pointer is over the row.
    pub hovered: bool,
    /// It is open, when it can open.
    pub expanded: bool,
    /// The style it draws with.
    pub style: TreeStyle,
}

/// The rows of a tree with `expanded` open. `row` gives a node's key and label, `children` its
/// children.
pub fn tree_rows<'t, N>(
    roots: &'t [N],
    expanded: &HashSet<u64>,
    row: impl Fn(&'t N) -> (u64, &'t str),
    children: impl Fn(&'t N) -> &'t [N],
) -> Vec<TreeRow<'t>> {
    let mut out = Vec::new();
    let mut stack: Vec<(&'t N, u16)> = roots.iter().rev().map(|n| (n, 0)).collect();
    while let Some((node, depth)) = stack.pop() {
        let (key, label) = row(node);
        let kids = children(node);
        out.push(TreeRow {
            key,
            depth,
            label,
            branch: !kids.is_empty(),
        });
        if expanded.contains(&key) {
            stack.extend(kids.iter().rev().map(|n| (n, depth + 1)));
        }
    }
    out
}

/// A list of [`TreeRow`]s: a click on a chevron, a double click, or the left and right arrows open
/// and close them. [`Response::activated`] is a leaf being opened.
#[derive(Debug)]
pub struct TreeView<'v, 't> {
    offset: &'v mut f32,
    rows: &'v [TreeRow<'t>],
    expanded: &'v mut HashSet<u64>,
    selected: Option<&'v mut Option<u64>>,
    style: Option<TreeStyle>,
}

impl<'v, 't> TreeView<'v, 't> {
    /// Creates a tree scrolled to `offset` showing `rows`, with `expanded` open.
    pub fn new(
        offset: &'v mut f32,
        rows: &'v [TreeRow<'t>],
        expanded: &'v mut HashSet<u64>,
    ) -> Self {
        Self {
            offset,
            rows,
            expanded,
            selected: None,
            style: None,
        }
    }

    /// Sets the variable that holds the pick, by key.
    #[must_use]
    pub fn selected(mut self, selected: &'v mut Option<u64>) -> Self {
        self.selected = Some(selected);
        self
    }

    /// Sets the style.
    #[must_use]
    pub fn style(mut self, style: TreeStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Draws it with plain labels.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        let text = self.style.map_or(ui.theme.palette.text, |style| style.text);
        self.show_with(ui, id, rect, |ui, row, at| {
            Label::new(row.label, text).paint(ui, at)
        })
    }

    /// Draws it, with `content` drawing each row's icon and text in the rect past its chevron.
    pub fn show_with<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        mut content: impl FnMut(&mut Ui<'_, Id>, &TreeRow<'t>, Rect) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        self.show_painted(ui, id, rect, |ui, look| {
            look.paint_default(ui)?;
            content(ui, &look.parts.row, look.body())
        })
    }

    /// Takes the input as [`TreeView::show`] does, then has `paint` draw each row in sight instead
    /// of its own look. [`Look::paint_default`] draws the row's fill and chevron, and
    /// [`Look::body`] is the rect past the chevron for its content.
    pub fn show_painted<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        mut paint: impl FnMut(&mut Ui<'_, Id>, &Look<TreeRowParts<'t>>) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        let Self {
            offset,
            rows,
            expanded,
            selected,
            style,
        } = self;
        let style = style.unwrap_or_else(|| TreeStyle::new(ui.theme));
        let mut own = None;
        let selected = selected.unwrap_or(&mut own);
        let mut picked = selected.and_then(|key| rows.iter().position(|r| r.key == key));

        // A press's place tells the chevron from the row; read now, the list takes it next.
        let press = ui.press(id, 0).map(|p| p.at).or(ui.state.click_at);
        let mut chevron_hit = false;
        let list = ListView::new(offset, rows.len(), style.row).selected(&mut picked);
        let mut response = list.show(ui, id, rect, |ui, state: ListRow| {
            let row = &rows[state.index];
            if row.branch {
                let (chevron, _) = parts(state.rect, row.depth, style);
                chevron_hit |= press.is_some_and(|at| chevron.contains(at));
            }
            let look = Look {
                rect: state.rect,
                state: Eased {
                    hover: f32::from(u8::from(state.hovered)),
                    enabled: 1.0,
                    ..Eased::default()
                },
                held_at: None,
                parts: TreeRowParts {
                    row: *row,
                    picked: state.picked,
                    hovered: state.hovered,
                    expanded: expanded.contains(&row.key),
                    style,
                },
            };
            paint(ui, &look)
        })?;

        let mut toggled = false;
        if let Some(i) = response.row_clicked
            && let Some(row) = rows.get(i)
            && row.branch
            && (chevron_hit || response.activated)
        {
            toggled |= flip(expanded, row.key);
        }
        if response.focused {
            toggled |= tree_keys(ui, rows, expanded, &mut picked, &mut response);
        }
        response.activated &= picked.and_then(|i| rows.get(i)).is_some_and(|r| !r.branch);
        let key = picked.and_then(|i| rows.get(i)).map(|r| r.key);
        response.changed |= toggled;
        if toggled {
            ui.mark_changed();
        }
        *selected = key;
        Ok(response)
    }
}

impl Look<TreeRowParts<'_>> {
    /// The row's own look: its fill, then its chevron.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_fill(ui)?;
        self.paint_chevron(ui)
    }

    /// The fill under the pick, or else under the pointer.
    pub fn paint_fill<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let style = self.parts.style;
        if self.parts.picked {
            ui.fill(self.rect, style.picked)
        } else if self.parts.hovered {
            ui.fill(self.rect, style.hover)
        } else {
            Ok(())
        }
    }

    /// The chevron, right when the row can open: down when open, else right.
    pub fn paint_chevron<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        if !self.parts.row.branch {
            return Ok(());
        }
        let style = self.parts.style;
        let shape = if self.parts.expanded {
            MarkShape::ChevronDown
        } else {
            MarkShape::ChevronRight
        };
        Mark::new(shape, style.chevron).paint(ui, self.chevron().inset(style.row * 0.2))
    }

    /// The rect the chevron sits in, at the row's start past the depth's indent.
    pub fn chevron(&self) -> Rect {
        parts(self.rect, self.parts.row.depth, self.parts.style).0
    }

    /// The rect past the chevron, for the row's icon and text.
    pub fn body(&self) -> Rect {
        parts(self.rect, self.parts.row.depth, self.parts.style).1
    }
}

fn parts(row: Rect, depth: u16, style: TreeStyle) -> (Rect, Rect) {
    let inset = f32::from(depth) * style.indent;
    let chevron = Rect::new(row.x + inset, row.y, style.row, row.height);
    let body = Rect::new(
        chevron.right(),
        row.y,
        (row.right() - chevron.right()).max(0.0),
        row.height,
    );
    (chevron, body)
}

fn flip(expanded: &mut HashSet<u64>, key: u64) -> bool {
    if !expanded.remove(&key) {
        expanded.insert(key);
    }
    true
}

/// Right opens, then steps in; left closes, then steps out. True when a row opened or closed.
fn tree_keys<Id: WidgetId>(
    ui: &Ui<'_, Id>,
    rows: &[TreeRow<'_>],
    expanded: &mut HashSet<u64>,
    picked: &mut Option<usize>,
    response: &mut Response,
) -> bool {
    let mut toggled = false;
    for (key, _) in ui.keys() {
        let Some(i) = *picked else { continue };
        let Some(row) = rows.get(i) else { continue };
        let open = expanded.contains(&row.key);
        match key {
            KeyCode::Right if row.branch && !open => toggled |= flip(expanded, row.key),
            KeyCode::Right if row.branch => {
                if rows.get(i + 1).is_some_and(|next| next.depth > row.depth) {
                    *picked = Some(i + 1);
                    response.changed = true;
                }
            }
            KeyCode::Left if row.branch && open => toggled |= flip(expanded, row.key),
            KeyCode::Left => {
                if let Some(parent) = rows[..i].iter().rposition(|r| r.depth < row.depth) {
                    *picked = Some(parent);
                    response.changed = true;
                }
            }
            KeyCode::Enter if row.branch => toggled |= flip(expanded, row.key),
            _ => {}
        }
    }
    toggled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::{Rig, left_button_clicks};

    struct Node {
        key: u64,
        name: &'static str,
        kids: Vec<Node>,
    }

    fn node(key: u64, name: &'static str, kids: Vec<Node>) -> Node {
        Node { key, name, kids }
    }

    fn forest() -> Vec<Node> {
        vec![
            node(
                1,
                "src",
                vec![
                    node(2, "ui", vec![node(3, "tree.rs", vec![])]),
                    node(4, "lib.rs", vec![]),
                ],
            ),
            node(5, "README", vec![]),
        ]
    }

    fn rows<'t>(forest: &'t [Node], expanded: &HashSet<u64>) -> Vec<TreeRow<'t>> {
        tree_rows(forest, expanded, |n| (n.key, n.name), |n| &n.kids)
    }

    const AREA: Rect = Rect::new(0.0, 0.0, 300.0, 400.0);

    struct Tree {
        rig: Rig,
        forest: Vec<Node>,
        expanded: HashSet<u64>,
        offset: f32,
        selected: Option<u64>,
    }

    impl Tree {
        fn new() -> Self {
            Self {
                rig: Rig::new(300, 400).still(),
                forest: forest(),
                expanded: HashSet::new(),
                offset: 0.0,
                selected: None,
            }
        }

        fn frame(&mut self) -> Response {
            let rows = rows(&self.forest, &self.expanded);
            let (offset, expanded, selected) =
                (&mut self.offset, &mut self.expanded, &mut self.selected);
            self.rig.frame(|ui| {
                TreeView::new(offset, &rows, expanded)
                    .selected(selected)
                    .show(ui, 1, AREA)
                    .expect("draw")
            })
        }

        fn row(&self) -> f32 {
            TreeStyle::new(&self.rig.theme).row
        }

        fn shown(&self) -> usize {
            rows(&self.forest, &self.expanded).len()
        }
    }

    #[test]
    fn rows_follow_what_is_open_in_order() {
        let forest = forest();
        let mut open = HashSet::new();
        let labels = |open: &HashSet<u64>| -> Vec<(u16, String)> {
            rows(&forest, open)
                .iter()
                .map(|r| (r.depth, r.label.to_owned()))
                .collect()
        };
        assert_eq!(labels(&open).len(), 2);
        open.insert(1);
        open.insert(2);
        assert_eq!(
            labels(&open),
            [
                (0, "src".to_owned()),
                (1, "ui".to_owned()),
                (2, "tree.rs".to_owned()),
                (1, "lib.rs".to_owned()),
                (0, "README".to_owned()),
            ]
        );
        open.remove(&1);
        assert_eq!(labels(&open).len(), 2, "a closed parent hides what is open");
    }

    #[test]
    fn a_chevron_click_opens_and_a_label_click_only_picks() {
        let mut tree = Tree::new();
        tree.frame();
        let y = tree.row() / 2.0;
        tree.rig.click(100, y);
        tree.frame();
        assert_eq!(tree.selected, Some(1));
        assert!(tree.expanded.is_empty(), "the label does not open it");
        tree.rig.click(y, y);
        assert!(tree.frame().changed);
        assert!(tree.expanded.contains(&1));
        assert_eq!(tree.shown(), 4);
    }

    #[test]
    fn a_double_click_opens_a_branch_and_activates_a_leaf() {
        let mut tree = Tree::new();
        let double = |tree: &mut Tree, y: f32| {
            tree.rig.send(left_button_clicks(100.0, y, true, 2));
            tree.rig.send(left_button_clicks(100.0, y, false, 2));
            tree.frame()
        };
        tree.frame();
        let row = tree.row();
        let response = double(&mut tree, row / 2.0);
        assert!(tree.expanded.contains(&1));
        assert!(!response.activated, "a branch opens instead");
        tree.frame();
        // src, ui, lib.rs, README: lib.rs is a leaf.
        let response = double(&mut tree, row * 2.5);
        assert_eq!(tree.selected, Some(4));
        assert!(response.activated);
    }

    #[test]
    fn arrow_keys_open_step_in_close_and_step_out() {
        let mut tree = Tree::new();
        tree.selected = Some(1);
        tree.frame();
        let y = tree.row() / 2.0;
        tree.rig.click(100, y);
        tree.frame();
        tree.rig.key(KeyCode::Right);
        tree.frame();
        assert!(tree.expanded.contains(&1));
        assert_eq!(tree.selected, Some(1), "opening does not move the pick");
        tree.rig.key(KeyCode::Right);
        tree.frame();
        assert_eq!(tree.selected, Some(2));
        tree.rig.key(KeyCode::Left);
        tree.frame();
        assert_eq!(tree.selected, Some(1), "left on a closed branch steps out");
        tree.rig.key(KeyCode::Left);
        tree.frame();
        assert!(!tree.expanded.contains(&1));
    }

    #[test]
    fn the_pick_stays_with_its_node_as_rows_move() {
        let mut tree = Tree::new();
        tree.selected = Some(5);
        tree.frame();
        tree.expanded.insert(1);
        tree.frame();
        assert_eq!(tree.selected, Some(5));
    }

    #[test]
    fn a_pick_whose_node_went_away_is_dropped() {
        let mut tree = Tree::new();
        tree.expanded.insert(1);
        tree.selected = Some(4);
        tree.frame();
        tree.expanded.remove(&1);
        tree.frame();
        assert_eq!(tree.selected, None);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn show_painted_draws_the_apps_rows_and_still_opens_on_a_chevron() {
        let mut rig = Rig::new(300, 400).still();
        let forest = forest();
        let mut offset = 0.0;
        let row = TreeStyle::new(&rig.theme).row;
        let y = row / 2.0;
        let mut show = |rig: &mut Rig, expanded: &mut HashSet<u64>| {
            let rows = rows(&forest, expanded);
            rig.frame(|ui| {
                TreeView::new(&mut offset, &rows, expanded)
                    .show_painted(ui, 1, AREA, |ui, look| {
                        ui.fill(look.rect, Color::rgb(0, 200, 0))
                    })
                    .expect("draw")
            })
        };
        let mut expanded = HashSet::new();
        show(&mut rig, &mut expanded);
        assert_eq!(
            rig.pixel(250, y as u32),
            [0, 200, 0, 255],
            "the app's row, not the default"
        );
        rig.click(y, y);
        show(&mut rig, &mut expanded);
        assert!(expanded.contains(&1), "the chevron still opens it");
    }
}
