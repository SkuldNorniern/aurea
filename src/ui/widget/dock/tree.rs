//! The arrangement docked panels are in: groups of tabs, split side by side
//! or one above the other, any number deep.

use std::mem;
use std::ops::Range;

use super::plan::DockPlan;
use crate::render::{Point, Rect};

/// Where a panel goes next to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockSide {
    Left,
    Right,
    Top,
    Bottom,
    /// Into the same group, as another tab.
    Center,
}

impl DockSide {
    /// The side of `rect` a point falls in: the middle third is the centre,
    /// the rest goes to the nearest edge.
    pub fn at(rect: Rect, point: Point) -> Self {
        let u = ((point.x - rect.x) / rect.width.max(1.0)).clamp(0.0, 1.0);
        let v = ((point.y - rect.y) / rect.height.max(1.0)).clamp(0.0, 1.0);
        let middle = |t: f32| (1.0 / 3.0..=2.0 / 3.0).contains(&t);
        if middle(u) && middle(v) {
            return Self::Center;
        }
        let edges = [
            (u, Self::Left),
            (1.0 - u, Self::Right),
            (v, Self::Top),
            (1.0 - v, Self::Bottom),
        ];
        edges
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(Self::Center, |(_, side)| side)
    }

    /// The part of `rect` a panel docked on this side takes.
    pub fn preview(self, rect: Rect) -> Rect {
        match self {
            Self::Left => rect.split_left(rect.width / 2.0, 0.0).0,
            Self::Right => rect.split_right(rect.width / 2.0, 0.0).0,
            Self::Top => rect.split_top(rect.height / 2.0, 0.0).0,
            Self::Bottom => rect.split_bottom(rect.height / 2.0, 0.0).0,
            Self::Center => rect,
        }
    }
}

/// Which way a split lays out its two parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockAxis {
    /// Side by side.
    Row,
    /// One above the other.
    Column,
}

#[derive(Clone, Debug)]
enum Node<P> {
    Tabs {
        panels: Vec<P>,
        active: usize,
    },
    Split {
        axis: DockAxis,
        /// The first part's share of the room.
        ratio: f32,
        parts: Box<[Node<P>; 2]>,
    },
}

impl<P> Node<P> {
    const fn empty() -> Self {
        Self::Tabs {
            panels: Vec::new(),
            active: 0,
        }
    }
}

/// One group of tabs, as laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct DockGroup {
    /// The whole group: its tab strip and its content.
    pub rect: Rect,
    pub strip: Rect,
    pub content: Rect,
    /// Its panels' place in [`DockLayout::panels`].
    pub tabs: Range<usize>,
    /// Which of them shows, counted within the group.
    pub active: usize,
}

/// The space between the two parts of a split, which drags to resize it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockBar {
    pub rect: Rect,
    /// The room the split shares out.
    pub span: Rect,
    pub axis: DockAxis,
    /// Which split, in [`DockTree::set_ratio`]'s numbering.
    pub index: usize,
}

/// Where the groups and bars of a [`DockTree`] land in a rect. Kept and
/// filled again each frame, so laying out does not allocate once it has
/// grown to fit.
#[derive(Clone, Debug)]
pub struct DockLayout<P> {
    pub groups: Vec<DockGroup>,
    pub panels: Vec<P>,
    pub bars: Vec<DockBar>,
}

impl<P> Default for DockLayout<P> {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            panels: Vec::new(),
            bars: Vec::new(),
        }
    }
}

impl<P: Copy> DockLayout<P> {
    /// The panel each group shows and the rect to draw it in.
    pub fn visible(&self) -> impl Iterator<Item = (P, Rect)> + '_ {
        self.groups.iter().filter_map(|group| {
            let at = group.tabs.start + group.active;
            (at < group.tabs.end)
                .then(|| self.panels.get(at).map(|p| (*p, group.content)))
                .flatten()
        })
    }

    /// The group whose rect holds `point`.
    pub fn group_at(&self, point: Point) -> Option<&DockGroup> {
        self.groups.iter().find(|group| group.rect.contains(point))
    }
}

/// Sizes a layout works with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockMetrics {
    pub strip: f32,
    /// The bar between split parts.
    pub gap: f32,
    /// The least a part is squeezed to while there is room.
    pub min_part: f32,
}

/// Panels arranged in tabbed groups split across a rect. `P` names a panel:
/// usually a small `Copy` id of the app's.
#[derive(Clone, Debug)]
pub struct DockTree<P> {
    root: Option<Node<P>>,
}

impl<P> Default for DockTree<P> {
    fn default() -> Self {
        Self { root: None }
    }
}

impl<P: Copy + PartialEq> DockTree<P> {
    pub(super) fn root_plan(&self) -> Option<DockPlan<P>> {
        self.root.as_ref().map(node_plan)
    }

    /// `seen` gathers the panels placed so far, so none is placed twice.
    pub(super) fn from_root_plan(plan: &DockPlan<P>, seen: &mut Vec<P>) -> Self {
        Self {
            root: node_from_plan(plan, seen),
        }
    }

    /// One group holding `panel`.
    pub fn new(panel: P) -> Self {
        Self {
            root: Some(Node::Tabs {
                panels: vec![panel],
                active: 0,
            }),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn contains(&self, panel: P) -> bool {
        self.root
            .as_ref()
            .is_some_and(|root| group_of(root, panel).is_some())
    }

    /// Puts `panel` on `side` of the group holding `beside`, the new part
    /// taking half the room. An empty tree takes it as its one group.
    /// Returns `false`, changing nothing, when `panel` is already docked or
    /// `beside` is not.
    pub fn insert(&mut self, panel: P, beside: P, side: DockSide) -> bool {
        self.insert_sized(panel, beside, side, 0.5)
    }

    /// Like [`DockTree::insert`], the new part taking `share` of the room.
    pub fn insert_sized(&mut self, panel: P, beside: P, side: DockSide, share: f32) -> bool {
        let Some(root) = self.root.as_mut() else {
            *self = Self::new(panel);
            return true;
        };
        if group_of(root, panel).is_some() {
            return false;
        }
        let Some(node) = group_of_mut(root, beside) else {
            return false;
        };
        place(node, panel, side, share);
        true
    }

    /// Takes `panel` out. A group left empty goes, and its neighbour takes
    /// the room.
    pub fn remove(&mut self, panel: P) -> bool {
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        match remove_in(root, panel) {
            Removed::No => false,
            Removed::Yes => true,
            Removed::Emptied => {
                self.root = None;
                true
            }
        }
    }

    /// Moves `panel` to `side` of the group holding `beside`. A panel moved
    /// beside itself leaves its group for a new one next to it.
    pub fn move_to(&mut self, panel: P, beside: P, side: DockSide) -> bool {
        let Some(root) = self.root.as_ref() else {
            return false;
        };
        let target = if panel == beside {
            if side == DockSide::Center {
                return false;
            }
            match group_of(root, panel) {
                Some((panels, _)) => match panels.iter().find(|p| **p != panel) {
                    Some(other) => *other,
                    None => return false,
                },
                None => return false,
            }
        } else {
            beside
        };
        if group_of(root, target).is_none() || !self.remove(panel) {
            return false;
        }
        self.insert(panel, target, side)
    }

    /// Shows `panel` in its group.
    pub fn activate(&mut self, panel: P) -> bool {
        let found = self
            .root
            .as_mut()
            .and_then(|root| group_of_mut(root, panel));
        let Some(Node::Tabs { panels, active }) = found else {
            return false;
        };
        let Some(at) = panels.iter().position(|p| *p == panel) else {
            return false;
        };
        *active = at;
        true
    }

    /// Whether `panel` is the one its group shows.
    pub fn is_active(&self, panel: P) -> bool {
        let found = self.root.as_ref().and_then(|root| group_of(root, panel));
        found.is_some_and(|(panels, active)| panels.get(active) == Some(&panel))
    }

    /// Sets split `index`'s first part to `ratio` of its room. Splits count
    /// from the outermost, first part before second, as in
    /// [`DockBar::index`].
    pub fn set_ratio(&mut self, index: usize, ratio: f32) -> bool {
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        let mut seen = 0;
        split_at(root, index, &mut seen).is_some_and(|slot| {
            *slot = ratio.clamp(0.0, 1.0);
            true
        })
    }

    /// Lays the tree out in `rect` into `out`, which is cleared first.
    pub fn layout(&self, rect: Rect, metrics: DockMetrics, out: &mut DockLayout<P>) {
        out.groups.clear();
        out.panels.clear();
        out.bars.clear();
        if let Some(root) = &self.root {
            let mut splits = 0;
            lay_out(root, rect, metrics, out, &mut splits);
        }
    }
}

fn group_of<P: Copy + PartialEq>(node: &Node<P>, panel: P) -> Option<(&[P], usize)> {
    match node {
        Node::Tabs { panels, active } => panels
            .contains(&panel)
            .then_some((panels.as_slice(), *active)),
        Node::Split { parts, .. } => parts.iter().find_map(|part| group_of(part, panel)),
    }
}

fn group_of_mut<P: Copy + PartialEq>(node: &mut Node<P>, panel: P) -> Option<&mut Node<P>> {
    match node {
        Node::Tabs { panels, .. } => {
            if panels.contains(&panel) {
                Some(node)
            } else {
                None
            }
        }
        Node::Split { parts, .. } => parts.iter_mut().find_map(|part| group_of_mut(part, panel)),
    }
}

fn place<P>(node: &mut Node<P>, panel: P, side: DockSide, share: f32) {
    if let (DockSide::Center, Node::Tabs { panels, active }) = (side, &mut *node) {
        panels.push(panel);
        *active = panels.len() - 1;
        return;
    }
    let old = mem::replace(node, Node::empty());
    let new = Node::Tabs {
        panels: vec![panel],
        active: 0,
    };
    let share = share.clamp(0.05, 0.95);
    let (axis, ratio, parts) = match side {
        DockSide::Left => (DockAxis::Row, share, [new, old]),
        DockSide::Top => (DockAxis::Column, share, [new, old]),
        DockSide::Right => (DockAxis::Row, 1.0 - share, [old, new]),
        DockSide::Bottom | DockSide::Center => (DockAxis::Column, 1.0 - share, [old, new]),
    };
    *node = Node::Split {
        axis,
        ratio,
        parts: Box::new(parts),
    };
}

enum Removed {
    No,
    Yes,
    /// The node held nothing else and has to go.
    Emptied,
}

fn remove_in<P: Copy + PartialEq>(node: &mut Node<P>, panel: P) -> Removed {
    match node {
        Node::Tabs { panels, active } => {
            let Some(at) = panels.iter().position(|p| *p == panel) else {
                return Removed::No;
            };
            panels.remove(at);
            if panels.is_empty() {
                return Removed::Emptied;
            }
            if at < *active || *active >= panels.len() {
                *active = active.saturating_sub(1);
            }
            Removed::Yes
        }
        Node::Split { parts, .. } => {
            for i in 0..2 {
                match remove_in(&mut parts[i], panel) {
                    Removed::No => {}
                    Removed::Yes => return Removed::Yes,
                    Removed::Emptied => {
                        let kept = mem::replace(&mut parts[1 - i], Node::empty());
                        *node = kept;
                        return Removed::Yes;
                    }
                }
            }
            Removed::No
        }
    }
}

fn split_at<'n, P>(node: &'n mut Node<P>, index: usize, seen: &mut usize) -> Option<&'n mut f32> {
    let Node::Split { ratio, parts, .. } = node else {
        return None;
    };
    if *seen == index {
        return Some(ratio);
    }
    *seen += 1;
    let [first, second] = &mut **parts;
    if let Some(found) = split_at(first, index, seen) {
        return Some(found);
    }
    split_at(second, index, seen)
}

fn lay_out<P: Copy>(
    node: &Node<P>,
    rect: Rect,
    metrics: DockMetrics,
    out: &mut DockLayout<P>,
    splits: &mut usize,
) {
    match node {
        Node::Tabs { panels, active } => {
            let start = out.panels.len();
            out.panels.extend_from_slice(panels);
            let (strip, content) = rect.split_top(metrics.strip.min(rect.height), 0.0);
            out.groups.push(DockGroup {
                rect,
                strip,
                content,
                tabs: start..out.panels.len(),
                active: (*active).min(panels.len().saturating_sub(1)),
            });
        }
        Node::Split { axis, ratio, parts } => {
            let index = *splits;
            *splits += 1;
            let (first, bar, second) = divide(rect, *axis, *ratio, metrics);
            out.bars.push(DockBar {
                rect: bar,
                span: rect,
                axis: *axis,
                index,
            });
            lay_out(&parts[0], first, metrics, out, splits);
            lay_out(&parts[1], second, metrics, out, splits);
        }
    }
}

/// The two parts of a split and the bar between them.
fn divide(rect: Rect, axis: DockAxis, ratio: f32, metrics: DockMetrics) -> (Rect, Rect, Rect) {
    let length = match axis {
        DockAxis::Row => rect.width,
        DockAxis::Column => rect.height,
    };
    let room = (length - metrics.gap).max(0.0);
    let least = metrics.min_part.min(room / 2.0);
    let first = (room * ratio).clamp(least, room - least);
    match axis {
        DockAxis::Row => {
            let (a, rest) = rect.split_left(first, 0.0);
            let (bar, b) = rest.split_left(metrics.gap.min(rest.width), 0.0);
            (a, bar, b)
        }
        DockAxis::Column => {
            let (a, rest) = rect.split_top(first, 0.0);
            let (bar, b) = rest.split_top(metrics.gap.min(rest.height), 0.0);
            (a, bar, b)
        }
    }
}

fn node_plan<P: Copy>(node: &Node<P>) -> DockPlan<P> {
    match node {
        Node::Tabs { panels, active } => DockPlan::Tabs {
            panels: panels.clone(),
            active: *active,
        },
        Node::Split { axis, ratio, parts } => DockPlan::Split {
            axis: *axis,
            ratio: *ratio,
            first: Box::new(node_plan(&parts[0])),
            second: Box::new(node_plan(&parts[1])),
        },
    }
}

fn node_from_plan<P: Copy + PartialEq>(plan: &DockPlan<P>, seen: &mut Vec<P>) -> Option<Node<P>> {
    match plan {
        DockPlan::Tabs { panels, active } => {
            let start = seen.len();
            for &p in panels {
                if !seen.contains(&p) {
                    seen.push(p);
                }
            }
            let panels = seen[start..].to_vec();
            let active = (*active).min(panels.len().checked_sub(1)?);
            Some(Node::Tabs { panels, active })
        }
        DockPlan::Split {
            axis,
            ratio,
            first,
            second,
        } => match (node_from_plan(first, seen), node_from_plan(second, seen)) {
            (Some(a), Some(b)) => Some(Node::Split {
                axis: *axis,
                ratio: if ratio.is_nan() {
                    0.5
                } else {
                    ratio.clamp(0.0, 1.0)
                },
                parts: Box::new([a, b]),
            }),
            (part, None) | (None, part) => part,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const METRICS: DockMetrics = DockMetrics {
        strip: 20.0,
        gap: 4.0,
        min_part: 40.0,
    };

    fn layout(tree: &DockTree<u8>) -> DockLayout<u8> {
        let mut out = DockLayout::default();
        tree.layout(Rect::new(0.0, 0.0, 404.0, 300.0), METRICS, &mut out);
        out
    }

    #[test]
    fn insert_splits_and_tabs() {
        let mut tree = DockTree::new(1);
        assert!(tree.insert(2, 1, DockSide::Right));
        assert!(tree.insert(3, 2, DockSide::Center));
        let out = layout(&tree);
        assert_eq!(out.groups.len(), 2);
        assert_eq!(out.groups[0].rect, Rect::new(0.0, 0.0, 200.0, 300.0));
        assert_eq!(out.groups[1].rect.x, 204.0);
        assert_eq!(out.bars[0].rect, Rect::new(200.0, 0.0, 4.0, 300.0));
        let visible: Vec<_> = out.visible().map(|(p, _)| p).collect();
        assert_eq!(visible, [1, 3], "the newest tab shows");
        assert_eq!(out.groups[1].content.y, 20.0);
    }

    #[test]
    fn insert_refuses_what_it_cannot_place() {
        let mut tree = DockTree::new(1);
        assert!(!tree.insert(1, 1, DockSide::Left), "already docked");
        assert!(!tree.insert(4, 9, DockSide::Left), "nothing to go beside");
        assert_eq!(layout(&tree).groups.len(), 1);
    }

    #[test]
    fn removing_the_last_tab_gives_the_room_back() {
        let mut tree = DockTree::new(1);
        tree.insert(2, 1, DockSide::Bottom);
        tree.insert(3, 2, DockSide::Left);
        assert!(tree.remove(2));
        assert!(!tree.remove(2));
        assert_eq!(layout(&tree).groups.len(), 2);
        tree.remove(1);
        let out = layout(&tree);
        assert_eq!(out.groups.len(), 1);
        assert_eq!(out.groups[0].rect, Rect::new(0.0, 0.0, 404.0, 300.0));
        tree.remove(3);
        assert!(tree.is_empty());
        assert!(tree.insert(5, 0, DockSide::Left), "an empty tree takes any");
    }

    #[test]
    fn moving_a_tab_out_of_its_group() {
        let mut tree = DockTree::new(1);
        tree.insert(2, 1, DockSide::Center);
        assert!(tree.is_active(2));
        assert!(tree.move_to(2, 2, DockSide::Top));
        let out = layout(&tree);
        assert_eq!(out.groups.len(), 2);
        assert_eq!(out.bars[0].axis, DockAxis::Column);
        assert!(!tree.move_to(1, 1, DockSide::Left), "alone in its group");
        assert!(tree.move_to(1, 2, DockSide::Center));
        assert_eq!(layout(&tree).groups.len(), 1);
        assert!(tree.activate(2));
        assert!(tree.is_active(2) && !tree.is_active(1));
    }

    #[test]
    fn ratios_follow_the_split_order_and_keep_a_minimum() {
        let mut tree = DockTree::new(1);
        tree.insert(2, 1, DockSide::Right);
        tree.insert(3, 2, DockSide::Bottom);
        assert!(tree.set_ratio(1, 0.25));
        assert!(!tree.set_ratio(2, 0.5));
        let out = layout(&tree);
        assert_eq!(out.bars[1].index, 1);
        assert_eq!(out.groups[1].rect.height, 74.0);
        tree.set_ratio(0, 0.0);
        assert_eq!(layout(&tree).groups[0].rect.width, 40.0);
    }

    #[test]
    fn drop_sides_come_from_where_the_pointer_is() {
        let r = Rect::new(0.0, 0.0, 300.0, 300.0);
        assert_eq!(DockSide::at(r, Point::new(150.0, 150.0)), DockSide::Center);
        assert_eq!(DockSide::at(r, Point::new(10.0, 150.0)), DockSide::Left);
        assert_eq!(DockSide::at(r, Point::new(150.0, 290.0)), DockSide::Bottom);
        assert_eq!(
            DockSide::Right.preview(r),
            Rect::new(150.0, 0.0, 150.0, 300.0)
        );
    }
}
