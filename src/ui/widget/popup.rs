//! A list that opens over everything: a dropdown's options or a context menu.

use std::iter;
use std::mem;

use crate::AureaResult;
use crate::render::{Color, Point, Rect};
use crate::ui::component::card;
use crate::ui::frame::{Align, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape};
use crate::ui::state::{Channel, Interaction, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

const ROW_PART: u16 = 0xF000;
const BACKDROP_PART: u16 = 0xEFFF;
const CARD_PART: u16 = 0xEFFE;
const FADE: Channel = Channel::Custom(0xF0F0);
const TALLEST: f32 = 360.0;

/// Selection colors shared by a popup and its nested menus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupStyle {
    /// Check mark colour.
    pub check: Color,
    /// Highlight colour of the selected row.
    pub highlight: Color,
}

impl PopupStyle {
    /// Popup colours from the theme.
    pub fn new(theme: &Theme) -> Self {
        Self {
            check: theme.palette.accent,
            highlight: theme.palette.hover,
        }
    }
}

/// One row of a menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuItem<'a> {
    /// Text of the row.
    pub label: &'a str,
    /// Shown dimmed at the right, like `Ctrl+S`.
    pub shortcut: Option<&'a str>,
    /// Whether it can be picked.
    pub enabled: bool,
    /// A tick in front: the option in use.
    pub checked: bool,
    /// A line between groups instead of a row.
    pub separator: bool,
    /// Child rows opened with the pointer, Right, or Enter.
    pub children: &'a [MenuItem<'a>],
}

impl<'a> MenuItem<'a> {
    /// A row showing `label`.
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            shortcut: None,
            enabled: true,
            checked: false,
            separator: false,
            children: &[],
        }
    }

    /// A separator row.
    pub const fn separator() -> Self {
        Self {
            label: "",
            shortcut: None,
            enabled: false,
            checked: false,
            separator: true,
            children: &[],
        }
    }

    /// Sets the shortcut text.
    #[must_use]
    pub const fn shortcut(mut self, shortcut: &'a str) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    /// Sets whether it can be picked.
    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets whether it has a tick.
    #[must_use]
    pub const fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Adds a submenu.
    ///
    /// ```rust,no_run
    /// # use aurea::ui::{MenuItem, Ui};
    /// # fn menu(ui: &mut Ui<'_, u8>) {
    /// let export = [MenuItem::new("PNG"), MenuItem::new("SVG")];
    /// ui.open_menu(1, aurea::Point::new(20.0, 20.0), [
    ///     MenuItem::new("Export").submenu(&export),
    /// ]);
    /// if let Some(path) = ui.menu_picked_path(1) {
    ///     // [0, 1] selects Export > SVG.
    /// }
    /// # }
    /// ```
    #[must_use]
    pub const fn submenu(mut self, children: &'a [MenuItem<'a>]) -> Self {
        self.children = children;
        self
    }
}

impl<'a> From<&'a str> for MenuItem<'a> {
    fn from(label: &'a str) -> Self {
        Self::new(label)
    }
}

#[derive(Debug, Default)]
struct Row {
    label: String,
    shortcut: String,
    enabled: bool,
    checked: bool,
    separator: bool,
    children: Vec<Row>,
    has_children: bool,
    rect: Rect,
}

impl Row {
    fn assign(&mut self, item: MenuItem<'_>) {
        self.label.clear();
        self.label.push_str(item.label);
        self.shortcut.clear();
        self.shortcut.push_str(item.shortcut.unwrap_or(""));
        self.enabled = item.enabled && !item.separator;
        self.checked = item.checked;
        self.separator = item.separator;
        self.has_children = !item.children.is_empty();
        self.children
            .resize_with(item.children.len(), Self::default);
        for (row, item) in self.children.iter_mut().zip(item.children) {
            row.assign(*item);
        }
    }
}

#[derive(Debug)]
struct Parent {
    rows: Vec<Row>,
    len: usize,
    highlight: Option<usize>,
    scroll: f32,
    anchor: Rect,
    min_width: f32,
    selected: usize,
    nested: bool,
    reveal: bool,
}

#[derive(Debug)]
pub(crate) struct Popup<Id> {
    pub(crate) style: Option<PopupStyle>,
    owner: Option<Key<Id>>,
    anchor: Rect,
    min_width: f32,
    rows: Vec<Row>,
    len: usize,
    highlight: Option<usize>,
    scroll: f32,
    return_focus: Option<Key<Id>>,
    picked: Option<(Key<Id>, Vec<usize>)>,
    parents: Vec<Parent>,
    nested: bool,
    reveal: bool,
    card: bool,
    card_drawn: bool,
    focus_order: Vec<Key<Id>>,
    #[cfg(feature = "accessibility")]
    generation: u64,
}

impl<Id> Default for Popup<Id> {
    fn default() -> Self {
        Self {
            style: None,
            owner: None,
            anchor: Rect::default(),
            min_width: 0.0,
            rows: Vec::new(),
            len: 0,
            highlight: None,
            scroll: 0.0,
            return_focus: None,
            picked: None,
            parents: Vec::new(),
            nested: false,
            reveal: false,
            card: false,
            card_drawn: false,
            focus_order: Vec::new(),
            #[cfg(feature = "accessibility")]
            generation: 0,
        }
    }
}

impl<Id: WidgetId> Popup<Id> {
    pub(crate) fn open<'a>(
        &mut self,
        owner: Key<Id>,
        anchor: Rect,
        min_width: f32,
        items: impl IntoIterator<Item = MenuItem<'a>>,
        return_focus: Option<Key<Id>>,
    ) {
        #[cfg(feature = "accessibility")]
        {
            self.generation = self
                .generation
                .checked_add(1)
                .expect("menu generation exhausted");
        }
        while !self.parents.is_empty() {
            self.back();
        }
        self.nested = false;
        self.style = None;
        self.reveal = false;
        self.owner = Some(owner);
        self.card = false;
        self.card_drawn = false;
        self.focus_order.clear();
        self.anchor = anchor;
        self.min_width = min_width;
        self.scroll = 0.0;
        self.return_focus = return_focus;
        self.len = 0;
        for item in items {
            if self.rows.len() == self.len {
                self.rows.push(Row::default());
            }
            let row = &mut self.rows[self.len];
            row.assign(item);
            self.len += 1;
        }
        self.highlight = self.rows[..self.len]
            .iter()
            .position(|r| r.checked && r.enabled);
    }

    pub(crate) fn is_open_for(&self, owner: Key<Id>) -> bool {
        self.owner == Some(owner)
    }

    pub(crate) fn is_open(&self) -> bool {
        self.owner.is_some()
    }

    pub(crate) fn is_card(&self) -> bool {
        self.is_open() && self.card
    }

    pub(crate) fn owns_menu_part(&self, key: Key<Id>) -> bool {
        !self.card
            && self
                .owner
                .is_some_and(|owner| owner.id == key.id && key.part >= ROW_PART)
    }

    pub(crate) fn owns_focus(&self, focus: Option<Key<Id>>) -> bool {
        focus.is_some_and(|key| self.focus_order.contains(&key))
    }

    #[cfg(feature = "accessibility")]
    pub(crate) fn focus_order(&self) -> &[Key<Id>] {
        &self.focus_order
    }

    #[cfg(feature = "accessibility")]
    pub(crate) fn accessible_action(
        &mut self,
        owner: Key<Id>,
        generation: u64,
        path: &[usize],
        activate: bool,
    ) -> Option<Option<Key<Id>>> {
        if self.owner != Some(owner) || self.generation != generation || self.card {
            return None;
        }
        let (&leaf, ancestors) = path.split_last()?;
        while !self.parents.is_empty() {
            self.back();
        }
        for &row in ancestors {
            if !self.descend(row) {
                return None;
            }
        }
        if leaf >= self.len || !self.rows[leaf].enabled {
            return None;
        }
        self.highlight = Some(leaf);
        self.reveal = true;
        if !activate {
            return None;
        }
        let focus = self.pick(leaf);
        (!self.is_open()).then_some(focus)
    }

    pub(crate) fn next_focus(&self, focus: Option<Key<Id>>, back: bool) -> Option<Key<Id>> {
        let n = self.focus_order.len();
        if n == 0 {
            return None;
        }
        let at = focus.and_then(|key| self.focus_order.iter().position(|k| *k == key));
        let next = match (at, back) {
            (None, false) => 0,
            (None, true) => n - 1,
            (Some(at), false) => (at + 1) % n,
            (Some(at), true) => (at + n - 1) % n,
        };
        self.focus_order.get(next).copied()
    }

    pub(crate) fn begin_frame(&mut self) {
        self.card_drawn = false;
    }

    pub(crate) fn close(&mut self) -> Option<Key<Id>> {
        while !self.parents.is_empty() {
            self.back();
        }
        self.owner = None;
        self.return_focus.take()
    }

    pub(crate) fn take_pick(&mut self, owner: Key<Id>) -> Option<usize> {
        if self
            .picked
            .as_ref()
            .is_some_and(|(key, path)| *key == owner && path.len() == 1)
        {
            return self.picked.take().map(|(_, path)| path[0]);
        }
        None
    }

    fn take_path(&mut self, owner: Key<Id>) -> Option<Vec<usize>> {
        if self.picked.as_ref().is_some_and(|(key, _)| *key == owner) {
            return self.picked.take().map(|(_, path)| path);
        }
        None
    }

    fn swap_parent(&mut self, depth: usize) {
        let parent = &mut self.parents[depth];
        mem::swap(&mut self.rows, &mut parent.rows);
        mem::swap(&mut self.len, &mut parent.len);
        mem::swap(&mut self.highlight, &mut parent.highlight);
        mem::swap(&mut self.scroll, &mut parent.scroll);
        mem::swap(&mut self.anchor, &mut parent.anchor);
        mem::swap(&mut self.min_width, &mut parent.min_width);
        mem::swap(&mut self.nested, &mut parent.nested);
        mem::swap(&mut self.reveal, &mut parent.reveal);
    }

    fn back(&mut self) {
        let Some(mut parent) = self.parents.pop() else {
            return;
        };
        parent.rows[parent.selected].children = mem::take(&mut self.rows);
        self.rows = parent.rows;
        self.len = parent.len;
        self.highlight = parent.highlight;
        self.scroll = parent.scroll;
        self.anchor = parent.anchor;
        self.min_width = parent.min_width;
        self.nested = parent.nested;
        self.reveal = parent.reveal;
    }

    fn descend(&mut self, row: usize) -> bool {
        if row >= self.len || !self.rows[row].enabled || !self.rows[row].has_children {
            return false;
        }
        let children = mem::take(&mut self.rows[row].children);
        let anchor = self.rows[row].rect;
        let parent = Parent {
            rows: mem::replace(&mut self.rows, children),
            len: self.len,
            highlight: Some(row),
            scroll: self.scroll,
            anchor: self.anchor,
            min_width: self.min_width,
            selected: row,
            nested: self.nested,
            reveal: self.reveal,
        };
        self.parents.push(parent);
        self.len = self.rows.len();
        self.highlight = self.step(None, true);
        self.scroll = 0.0;
        self.anchor = anchor;
        self.min_width = 0.0;
        self.nested = true;
        self.reveal = true;
        true
    }

    fn pick(&mut self, row: usize) -> Option<Key<Id>> {
        let owner = self.owner?;
        if !self.rows.get(row).is_some_and(|r| r.enabled) || row >= self.len {
            return None;
        }
        if self.descend(row) {
            return None;
        }
        let mut path: Vec<_> = self.parents.iter().map(|parent| parent.selected).collect();
        path.push(row);
        self.picked = Some((owner, path));
        self.close()
    }

    fn step(&self, from: Option<usize>, forward: bool) -> Option<usize> {
        let rows = &self.rows[..self.len];
        let n = rows.len();
        if n == 0 {
            return None;
        }
        let mut at = from.unwrap_or(if forward { n - 1 } else { 0 });
        for _ in 0..n {
            at = if forward {
                (at + 1) % n
            } else {
                (at + n - 1) % n
            };
            if rows[at].enabled {
                return Some(at);
            }
        }
        None
    }

    pub(crate) fn key(&mut self, key: KeyCode) -> (bool, Option<Option<Key<Id>>>) {
        self.reveal = true;
        match key {
            KeyCode::Down => self.highlight = self.step(self.highlight, true),
            KeyCode::Up => self.highlight = self.step(self.highlight, false),
            KeyCode::Home => self.highlight = self.step(None, true),
            KeyCode::End => self.highlight = self.step(None, false),
            KeyCode::Enter | KeyCode::Space => {
                if self.highlight.is_some_and(|row| self.descend(row)) {
                    return (true, None);
                }
                if self.highlight.is_none() {
                    return (true, None);
                }
                let focus = self.highlight.and_then(|row| self.pick(row));
                return (true, Some(focus.or(self.return_focus)));
            }
            KeyCode::Right => {
                if let Some(row) = self.highlight {
                    self.descend(row);
                }
            }
            KeyCode::Left | KeyCode::Escape if !self.parents.is_empty() => self.back(),
            KeyCode::Escape => return (true, Some(self.close())),
            KeyCode::Tab => return (false, Some(self.close())),
            _ => return (false, None),
        }
        (true, None)
    }
}

struct Metrics {
    row: f32,
    gap: f32,
    pad: f32,
}

impl Metrics {
    fn of(height: f32) -> Self {
        Self {
            row: (height * 0.92).round(),
            gap: 9.0,
            pad: 4.0,
        }
    }

    fn height(&self, separator: bool) -> f32 {
        if separator { self.gap } else { self.row }
    }
}

pub(crate) fn draw<Id: WidgetId>(ui: &mut Ui<'_, Id>) -> AureaResult<()> {
    let Some(owner) = ui.state.popup.owner else {
        return Ok(());
    };
    if ui.state.popup.card {
        if !ui.state.popup.card_drawn {
            ui.close_popup();
        }
        return Ok(());
    }
    #[allow(clippy::cast_precision_loss)]
    let canvas = Rect::new(0.0, 0.0, ui.ctx.width() as f32, ui.ctx.height() as f32);
    ui.interact(owner.id, BACKDROP_PART, canvas, Sense::Hover, true);
    if ui.press(owner.id, BACKDROP_PART).is_some() {
        let focus = ui.state.popup.close();
        ui.state.give_focus(focus);
        ui.mark_changed();
        return Ok(());
    }
    let m = Metrics::of(ui.theme.control_height);
    let rate = ui.theme.motion.focus * 1.5;
    let shown = ui.state.tweens.from_to((owner, FADE), 0.0, 1.0, rate);
    let depth = ui.state.popup.parents.len();
    let mut action = None;
    for level in 0..=depth {
        if level > 0 {
            let parent = &ui.state.popup.parents[level - 1];
            let anchor = parent.rows[parent.selected].rect;
            if level < depth {
                ui.state.popup.parents[level].anchor = anchor;
            } else {
                ui.state.popup.anchor = anchor;
            }
        }
        if level < depth {
            ui.state.popup.swap_parent(level);
        }
        let rect = place(ui, canvas, &m);
        let drawn = (|| {
            card(ui.theme).fade(shown).paint(ui, rect)?;
            scroll(ui, rect, &m);
            let input = u16::try_from(level)
                .ok()
                .and_then(|part| ROW_PART.checked_add(part))
                .map(|part| ui.interact(owner.id, part, rect, Sense::Click, true))
                .unwrap_or_default();
            ui.clipped(rect.inset(1.0), |ui| {
                rows(ui, rect, &m, shown, input, level)
            })
        })();
        if level < depth {
            ui.state.popup.swap_parent(level);
        }
        if let Some((row, clicked)) = drawn? {
            action = Some((level, row, clicked));
        }
    }
    apply_action(ui, action, depth);
    Ok(())
}

fn apply_action<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    action: Option<(usize, usize, bool)>,
    depth: usize,
) {
    if let Some((level, row, clicked)) = action {
        let already_open = level < depth && ui.state.popup.parents[level].selected == row;
        if !already_open {
            while ui.state.popup.parents.len() > level {
                ui.state.popup.back();
            }
            ui.state.popup.highlight = Some(row);
            if clicked {
                let focus = ui.state.popup.pick(row);
                if !ui.state.popup.is_open() {
                    ui.state.give_focus(focus);
                }
            } else {
                ui.state.popup.descend(row);
            }
            ui.mark_changed();
        }
    }
}

fn place<Id: WidgetId>(ui: &mut Ui<'_, Id>, canvas: Rect, m: &Metrics) -> Rect {
    let fonts = ui.fonts();
    let p = &ui.state.popup;
    let mut height = m.pad * 2.0;
    for row in &p.rows[..p.len] {
        height += m.height(row.separator);
    }
    let (anchor, min_width) = (p.anchor, p.min_width);
    let pad = ui.theme.padding;
    let mut widest: f32 = 0.0;
    for i in 0..ui.state.popup.len {
        if ui.state.popup.rows[i].separator {
            continue;
        }
        let label = mem::take(&mut ui.state.popup.rows[i].label);
        let shortcut = mem::take(&mut ui.state.popup.rows[i].shortcut);
        let mut width = ui.text_width(&label, &fonts.text);
        if !shortcut.is_empty() {
            width += ui.text_width(&shortcut, &fonts.small) + pad * 2.0;
        }
        if ui.state.popup.rows[i].has_children {
            width += m.row;
        }
        let row = &mut ui.state.popup.rows[i];
        (row.label, row.shortcut) = (label, shortcut);
        widest = widest.max(width);
    }
    let check = m.row * 0.75;
    let width = (widest + check + pad * 2.0)
        .max(min_width)
        .min((canvas.width - 8.0).max(0.0));
    let height = height.min(TALLEST).min((canvas.height - 8.0).max(0.0));
    let margin = 4.0f32.min(canvas.width / 2.0).min(canvas.height / 2.0);
    if ui.state.popup.nested {
        let x = if anchor.right() + width <= canvas.right() - margin {
            anchor.right()
        } else {
            anchor.x - width
        };
        return Rect::new(
            x.clamp(margin, (canvas.right() - width - margin).max(margin)),
            anchor
                .y
                .clamp(margin, (canvas.bottom() - height - margin).max(margin)),
            width,
            height,
        );
    }
    let x = anchor
        .x
        .clamp(margin, (canvas.right() - width - margin).max(margin));
    let below = anchor.bottom() + 4.0;
    let y = if below + height <= canvas.bottom() - 4.0 {
        below
    } else {
        (anchor.y - 4.0 - height).max(margin)
    };
    Rect::new(x, y, width, height)
}

fn scroll<Id: WidgetId>(ui: &mut Ui<'_, Id>, rect: Rect, m: &Metrics) {
    let p = &ui.state.popup;
    let mut total = m.pad * 2.0;
    for row in &p.rows[..p.len] {
        total += m.height(row.separator);
    }
    let room = (total - rect.height).max(0.0);
    let over = ui.pointer().is_some_and(|at| rect.contains(at));
    let wheel = if over { ui.take_wheel().1 } else { 0.0 };
    let p = &mut ui.state.popup;
    p.scroll = (p.scroll - wheel * m.row).clamp(0.0, room);
    if p.reveal
        && let Some(index) = p.highlight
    {
        let top = m.pad
            + p.rows[..index]
                .iter()
                .map(|row| m.height(row.separator))
                .sum::<f32>();
        let bottom = top + m.height(p.rows[index].separator);
        if top < p.scroll + m.pad {
            p.scroll = (top - m.pad).max(0.0);
        }
        if bottom > p.scroll + rect.height - m.pad {
            p.scroll = (bottom - rect.height + m.pad).clamp(0.0, room);
        }
    }
    p.reveal = false;
}

fn rows<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    rect: Rect,
    m: &Metrics,
    shown: f32,
    input: Interaction,
    level: usize,
) -> AureaResult<Option<(usize, bool)>> {
    #[cfg(not(feature = "accessibility"))]
    let _ = level;
    let p = ui.theme.palette;
    let pad = ui.theme.padding;
    let mut y = rect.y + m.pad - ui.state.popup.scroll;
    let mut action = None;
    for i in 0..ui.state.popup.len {
        let row = &ui.state.popup.rows[i];
        let separator = row.separator;
        let h = m.height(separator);
        let cell = Rect::new(rect.x + m.pad, y, (rect.width - m.pad * 2.0).max(0.0), h);
        y += h;
        ui.state.popup.rows[i].rect = cell;
        if separator {
            let line = Rect::new(
                cell.x + pad / 2.0,
                cell.center().y,
                (cell.width - pad).max(0.0),
                1.0,
            );
            ui.fill(line, p.border.fade(shown))?;
            continue;
        }
        if cell.bottom() < rect.y || cell.y > rect.bottom() {
            continue;
        }
        if row_action(ui, rect, cell, i, input) {
            action = Some((i, input.clicked));
        }
        #[cfg(feature = "accessibility")]
        accessible_row(ui, cell, i, level);
        paint_row(ui, cell, m, shown, i)?;
    }
    Ok(action)
}

#[cfg(feature = "accessibility")]
fn accessible_row<Id: WidgetId>(ui: &mut Ui<'_, Id>, cell: Rect, i: usize, level: usize) {
    let p = &ui.state.popup;
    let row = &p.rows[i];
    let rect = cell
        .intersection(ui.state.clip.unwrap_or(cell))
        .unwrap_or_default();
    if let Some(bridge) = &mut ui.state.accessibility
        && let Some(owner) = p.owner
    {
        let mut path: Vec<_> = p.parents[..level]
            .iter()
            .map(|parent| parent.selected)
            .collect();
        path.push(i);
        bridge.menu(
            owner,
            p.generation,
            path,
            (row.label.as_str(), row.enabled, row.checked),
            rect,
            level == p.parents.len() && p.highlight == Some(i),
        );
    }
}

fn paint_row<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    cell: Rect,
    m: &Metrics,
    shown: f32,
    i: usize,
) -> AureaResult<()> {
    let p = ui.theme.palette;
    let style = ui
        .state
        .popup
        .style
        .unwrap_or_else(|| PopupStyle::new(ui.theme));
    let pad = ui.theme.padding;
    let row = &ui.state.popup.rows[i];
    let (enabled, checked, children) = (row.enabled, row.checked, row.has_children);
    let h = cell.height;
    let lit = ui.state.popup.highlight == Some(i) && enabled;
    if lit {
        ui.round(cell, ui.theme.radius.min(5.0), style.highlight.fade(shown))?;
    }
    let ink = if enabled { p.text } else { p.faint };
    let check = Rect::new(cell.x, cell.y, m.row * 0.75, h);
    if checked {
        Mark::new(MarkShape::Check, style.check.fade(shown)).paint(ui, check.inset(4.0))?;
    }
    let text = Rect::new(
        check.right(),
        cell.y,
        cell.right() - check.right() - pad / 2.0 - if children { m.row * 0.75 } else { 0.0 },
        h,
    );
    let label = mem::take(&mut ui.state.popup.rows[i].label);
    let drawn = Label::new(&label, ink.fade(shown)).paint(ui, text);
    ui.state.popup.rows[i].label = label;
    drawn?;
    let shortcut = mem::take(&mut ui.state.popup.rows[i].shortcut);
    let drawn = if shortcut.is_empty() {
        Ok(())
    } else {
        Label::new(&shortcut, p.dim.fade(shown))
            .small()
            .align(Align::End)
            .paint(ui, text)
    };
    ui.state.popup.rows[i].shortcut = shortcut;
    drawn?;
    if children {
        Label::new("›", ink.fade(shown))
            .align(Align::End)
            .paint(ui, cell)?;
    }
    Ok(())
}

fn row_action<Id: WidgetId>(
    ui: &Ui<'_, Id>,
    rect: Rect,
    cell: Rect,
    i: usize,
    input: Interaction,
) -> bool {
    let row = &ui.state.popup.rows[i];
    // A touch reports MouseExited immediately after release. Choose the row at
    // the accepted click, rather than requiring a hover pointer to remain alive.
    let at = if input.clicked {
        ui.state.click_at.or_else(|| ui.pointer())
    } else {
        ui.pointer()
    };
    let over = at.is_some_and(|point| rect.inset(1.0).contains(point) && cell.contains(point));
    row.enabled
        && over
        && (input.clicked
            || (input.hovered
                && (ui.state.popup.highlight != Some(i)
                    || (row.has_children && !row.children.is_empty()))))
}
impl<Id: WidgetId> Ui<'_, Id> {
    /// Opens a card at logical canvas coordinates.
    pub fn open_popup_card(&mut self, id: Id, rect: Rect) {
        let focus = self.state.focus;
        self.state.popup.open(
            Key::new(id, MENU_PART),
            rect,
            rect.width,
            iter::empty(),
            focus,
        );
        self.state.popup.card = true;
        self.state.give_focus(None);
        self.mark_changed();
    }

    /// Draws an open card last, with its contents clipped to its padded area.
    ///
    /// ```rust,no_run
    /// use aurea::{AureaResult, Rect};
    /// use aurea::ui::{Slider, Ui};
    /// fn brush_card(ui: &mut Ui<'_, u32>, radius: &mut f32, open: bool) -> AureaResult<()> {
    ///     if open {
    ///         ui.open_popup_card(10, Rect::new(100.0, 80.0, 240.0, 80.0));
    ///     }
    ///     ui.popup_card(10, |ui, content| {
    ///         Slider::new(radius, 1.0..=200.0).show(ui, 11, content)?;
    ///         Ok(())
    ///     })?;
    ///     Ok(())
    /// }
    /// ```
    pub fn popup_card<R>(
        &mut self,
        id: Id,
        content: impl FnOnce(&mut Self, Rect) -> AureaResult<R>,
    ) -> AureaResult<Option<R>> {
        let owner = Key::new(id, MENU_PART);
        if !self.state.popup.is_open_for(owner) || !self.state.popup.card {
            return Ok(None);
        }
        self.state.popup.card_drawn = true;
        #[allow(clippy::cast_precision_loss)]
        let canvas = Rect::new(0.0, 0.0, self.ctx.width() as f32, self.ctx.height() as f32);
        self.interact(id, BACKDROP_PART, canvas, Sense::Hover, true);
        if self.press(id, BACKDROP_PART).is_some() {
            self.close_popup();
            return Ok(None);
        }
        let requested = self.state.popup.anchor;
        let width = requested.width.max(0.0).min(canvas.width);
        let height = requested.height.max(0.0).min(canvas.height);
        let rect = Rect::new(
            requested.x.clamp(0.0, (canvas.width - width).max(0.0)),
            requested.y.clamp(0.0, (canvas.height - height).max(0.0)),
            width,
            height,
        );
        card(self.theme).paint(self, rect)?;
        self.interact(id, CARD_PART, rect, Sense::Hover, true);
        let start = self.state.order.len();
        let value = self.clipped(rect.inset(self.theme.padding), |ui| {
            content(ui, rect.inset(ui.theme.padding))
        });
        let state = &mut *self.state;
        state.popup.focus_order.clear();
        state
            .popup
            .focus_order
            .extend_from_slice(&state.order[start..]);
        if state.popup.is_card()
            && state.popup.is_open_for(owner)
            && !state.popup.owns_focus(state.focus)
        {
            let focus = state.popup.next_focus(None, false);
            if state.focus != focus {
                state.give_focus(focus);
                state.focus_visible = true;
                state.changed = true;
            }
        }
        value.map(Some)
    }
    /// Opens a context menu for `id` at `at`, usually where the secondary button went down.
    pub fn open_menu<'a>(
        &mut self,
        id: Id,
        at: Point,
        items: impl IntoIterator<Item = MenuItem<'a>>,
    ) {
        let focus = self.state.focus;
        let anchor = Rect::new(at.x, at.y - 4.0, 0.0, 0.0);
        self.state
            .popup
            .open(Key::new(id, MENU_PART), anchor, 0.0, items, focus);
        self.mark_changed();
    }

    /// Opens a context menu with explicit check and highlight colors.
    pub fn open_menu_styled<'a>(
        &mut self,
        id: Id,
        at: Point,
        items: impl IntoIterator<Item = MenuItem<'a>>,
        style: PopupStyle,
    ) {
        self.open_menu(id, at, items);
        self.state.popup.style = Some(style);
    }

    /// The row picked from `id`'s context menu, once.
    pub fn menu_picked(&mut self, id: Id) -> Option<usize> {
        self.state.popup.take_pick(Key::new(id, MENU_PART))
    }

    /// The selected leaf's indices, from the root through its submenus, once.
    pub fn menu_picked_path(&mut self, id: Id) -> Option<Vec<usize>> {
        self.state.popup.take_path(Key::new(id, MENU_PART))
    }

    /// Whether `id`'s context menu is open.
    pub fn menu_open(&self, id: Id) -> bool {
        self.state.popup.is_open_for(Key::new(id, MENU_PART))
    }

    /// Closes whatever popup is open.
    pub fn close_popup(&mut self) {
        if self.state.popup.is_open() {
            let focus = self.state.popup.close();
            self.state.give_focus(focus);
            self.mark_changed();
        }
    }
}

const MENU_PART: u16 = 0xFFFE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::{Button, TextField, Theme, UiState};
    use crate::window::{Modifiers, MouseButton, WindowEvent};

    fn menu_frame(state: &mut UiState<u8>, raster: &mut CpuRasterizer) {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().unwrap();
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            Button::new("Background")
                .show(&mut ui, 1, Rect::new(0.0, 210.0, 80.0, 30.0))
                .expect("background");
        }
        drop(ctx);
        raster.end_frame().unwrap();
    }

    #[test]
    fn touch_release_selects_menu_after_pointer_exit() {
        let mut state = UiState::<u8>::new();
        state.set_reduced_motion(true);
        let mut raster = CpuRasterizer::new(320, 240);
        let theme = Theme::dark();
        {
            let mut ctx = raster.begin_frame().expect("frame");
            let mut ui = state.frame(&mut *ctx, &theme);
            ui.open_menu(
                9,
                Point::new(10.0, 10.0),
                [MenuItem::new("Open"), MenuItem::new("Settings")],
            );
        }
        raster.end_frame().expect("present");
        let at = state.popup.rows[1].rect.center();
        for pressed in [true, false] {
            state.handle(&WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                modifiers: Modifiers::default(),
                x: f64::from(at.x),
                y: f64::from(at.y),
                click_count: 1,
                time: Default::default(),
            });
        }
        // Touch and Pencil report pointer exit immediately after the release.
        state.handle(&WindowEvent::MouseExited);
        menu_frame(&mut state, &mut raster);
        assert_eq!(state.popup.take_pick(Key::new(9, MENU_PART)), Some(1));
        assert!(!state.popup.is_open());
    }

    #[test]
    fn popup_selection_colors_are_applied_and_do_not_leak_to_the_next_menu() {
        let mut state = UiState::<u8>::new();
        state.set_reduced_motion(true);
        let mut raster = CpuRasterizer::new(320, 240);
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().unwrap();
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            ui.open_menu_styled(
                9,
                Point::new(20.0, 20.0),
                [MenuItem::new("Picked").checked(true)],
                PopupStyle {
                    check: Color::rgb(255, 0, 255),
                    highlight: Color::rgb(0, 255, 0),
                },
            );
        }
        drop(ctx);
        raster.end_frame().unwrap();
        assert!(raster.pixels().contains(&0xff00_ff00), "custom highlight");
        assert!(
            raster
                .pixels()
                .iter()
                .any(|pixel| pixel & 0x00ff_00ff == 0x00ff_00ff),
            "custom check"
        );
        state.popup.open(
            Key::new(9, MENU_PART),
            Rect::default(),
            0.0,
            [MenuItem::new("Plain")],
            None,
        );
        assert!(state.popup.style.is_none());
    }

    #[test]
    fn nested_keyboard_menus_return_leaf_paths() {
        let leaves = [
            MenuItem::new("Disabled").enabled(false),
            MenuItem::new("Leaf"),
        ];
        let children = [
            MenuItem::separator(),
            MenuItem::new("Deep").submenu(&leaves),
        ];
        let items = [
            MenuItem::new("Branch").submenu(&children),
            MenuItem::new("Root leaf"),
        ];
        let owner = Key::new(9u8, MENU_PART);
        let focus = Key::new(1, 0);
        let mut popup = Popup::default();
        popup.open(owner, Rect::default(), 0.0, items, Some(focus));
        popup.key(KeyCode::Down);
        assert_eq!(popup.key(KeyCode::Right), (true, None));
        assert_eq!(popup.highlight, Some(1));
        popup.key(KeyCode::Enter);
        assert_eq!(popup.parents.len(), 2);
        assert_eq!(popup.highlight, Some(1));
        assert_eq!(popup.key(KeyCode::Enter), (true, Some(Some(focus))));
        assert!(!popup.is_open());
        assert_eq!(
            popup.take_pick(owner),
            None,
            "legacy calls must not lose a nested pick"
        );
        assert_eq!(popup.take_path(owner), Some(vec![0, 1, 1]));
        assert_eq!(popup.take_path(owner), None);
    }

    #[test]
    fn left_and_escape_return_to_the_parent_before_dismissing() {
        let leaves = [MenuItem::new("Leaf")];
        let items = [MenuItem::new("Branch").submenu(&leaves)];
        let mut popup = Popup::default();
        popup.open(Key::new(9u8, MENU_PART), Rect::default(), 0.0, items, None);
        popup.key(KeyCode::Home);
        popup.key(KeyCode::Right);
        popup.key(KeyCode::Left);
        assert_eq!(popup.highlight, Some(0));
        popup.key(KeyCode::Enter);
        assert_eq!(popup.key(KeyCode::Escape), (true, None));
        assert!(popup.is_open());
        assert_eq!(popup.key(KeyCode::Escape), (true, Some(None)));
    }

    #[test]
    fn tab_dismissal_skips_the_closed_menu_in_the_focus_order() {
        let mut state = UiState::<u8>::new();
        state.set_reduced_motion(true);
        state.focus(1, 0);
        state.popup.open(
            Key::new(9, MENU_PART),
            Rect::default(),
            0.0,
            [MenuItem::new("Leaf")],
            Some(Key::new(1, 0)),
        );
        let mut raster = CpuRasterizer::new(320, 240);
        menu_frame(&mut state, &mut raster);
        state.handle(&WindowEvent::KeyInput {
            key: KeyCode::Tab,
            pressed: true,
            modifiers: Default::default(),
        });
        menu_frame(&mut state, &mut raster);
        assert!(!state.popup.is_open());
        assert_eq!(state.focus, Some(Key::new(1, 0)));
    }

    #[cfg(feature = "accessibility")]
    #[test]
    fn assistive_menu_actions_reject_old_openings_and_disabled_leaves() {
        let leaves = [
            MenuItem::new("Disabled").enabled(false),
            MenuItem::new("Leaf"),
        ];
        let items = [MenuItem::new("Branch").submenu(&leaves)];
        let owner = Key::new(9u8, MENU_PART);
        let mut popup = Popup::default();
        popup.open(owner, Rect::default(), 0.0, items, None);
        let old = popup.generation;
        popup.open(owner, Rect::default(), 0.0, items, None);
        assert_eq!(popup.accessible_action(owner, old, &[0, 1], true), None);
        assert_eq!(popup.take_path(owner), None);
        assert_eq!(
            popup.accessible_action(owner, popup.generation, &[0, 0], true),
            None
        );
        assert!(popup.is_open());
        assert_eq!(
            popup.accessible_action(owner, popup.generation, &[0, 1], true),
            Some(None)
        );
        assert_eq!(popup.take_path(owner), Some(vec![0, 1]));
    }

    #[test]
    fn pointer_can_open_checked_submenus_and_pick_without_clicking_through() {
        let children = [MenuItem::new("Child")];
        let items = [MenuItem::new("Branch").checked(true).submenu(&children)];
        let mut state = UiState::<u8>::new();
        state.set_reduced_motion(true);
        state.popup.open(
            Key::new(9, MENU_PART),
            Rect::new(250.0, 20.0, 0.0, 0.0),
            0.0,
            items,
            None,
        );
        let mut raster = CpuRasterizer::new(320, 240);
        menu_frame(&mut state, &mut raster);
        let point = state.popup.rows[0].rect.center();
        state.handle(&WindowEvent::MouseMove {
            x: f64::from(point.x),
            y: f64::from(point.y),
            buttons: Default::default(),
            modifiers: Default::default(),
            time: Default::default(),
        });
        menu_frame(&mut state, &mut raster);
        assert_eq!(state.popup.parents.len(), 1);
        menu_frame(&mut state, &mut raster);
        let point = state.popup.rows[0].rect.center();
        assert!(
            point.x < state.popup.parents[0].rows[0].rect.x,
            "child flips left at the edge"
        );
        for pressed in [true, false] {
            state.handle(&WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                modifiers: Default::default(),
                x: f64::from(point.x),
                y: f64::from(point.y),
                click_count: 1,
                time: Default::default(),
            });
            menu_frame(&mut state, &mut raster);
        }
        assert!(!state.popup.is_open());
        assert_eq!(
            state.popup.take_path(Key::new(9, MENU_PART)),
            Some(vec![0, 0])
        );
    }

    #[test]
    fn keyboard_reveals_scrolled_rows_and_tiny_menu_canvases_stay_valid() {
        let items = (0..80).map(|_| MenuItem::new("Row"));
        let mut state = UiState::<u8>::new();
        state.set_reduced_motion(true);
        state
            .popup
            .open(Key::new(9, MENU_PART), Rect::default(), 0.0, items, None);
        let mut raster = CpuRasterizer::new(320, 240);
        menu_frame(&mut state, &mut raster);
        state.popup.key(KeyCode::End);
        menu_frame(&mut state, &mut raster);
        assert!(state.popup.scroll > 0.0);
        let last = state.popup.rows[79].rect;
        assert!(last.bottom() <= 240.0);
        let theme = Theme::dark();
        let mut tiny = CpuRasterizer::new(2, 3);
        let mut ctx = tiny.begin_frame().unwrap();
        let mut ui = state.frame(&mut *ctx, &theme);
        let rect = place(
            &mut ui,
            Rect::new(0.0, 0.0, 2.0, 3.0),
            &Metrics::of(theme.control_height),
        );
        assert!(rect.width >= 0.0 && rect.height >= 0.0);
    }

    struct Rig {
        state: UiState<u8>,
        raster: CpuRasterizer,
        theme: Theme,
        background: String,
        text: String,
        background_clicks: usize,
        card_clicks: usize,
    }

    impl Rig {
        fn new() -> Self {
            let mut rig = Self {
                state: UiState::new(),
                raster: CpuRasterizer::new(320, 240),
                theme: Theme::dark(),
                background: "background".into(),
                text: String::new(),
                background_clicks: 0,
                card_clicks: 0,
            };
            rig.state.set_reduced_motion(true);
            rig.state.focus(1, 0);
            rig.frame(false, false);
            rig.frame(true, true);
            rig
        }

        fn frame(&mut self, open: bool, show: bool) {
            let mut ctx = self.raster.begin_frame().expect("begin");
            {
                let mut ui = self.state.frame(&mut *ctx, &self.theme);
                if Button::new("Behind")
                    .show(&mut ui, 8, Rect::new(0.0, 0.0, 320.0, 240.0))
                    .expect("background")
                    .clicked
                {
                    self.background_clicks += 1;
                }
                TextField::new(&mut self.background)
                    .show(&mut ui, 1, Rect::new(0.0, 0.0, 50.0, 35.0))
                    .expect("background field");
                if open {
                    ui.open_popup_card(9, Rect::new(60.0, 50.0, 150.0, 130.0));
                }
                if show {
                    ui.popup_card(9, |ui, rect| {
                        TextField::new(&mut self.text).show(
                            ui,
                            2,
                            Rect::new(rect.x, rect.y, rect.width, 30.0),
                        )?;
                        if Button::new("Apply")
                            .show(ui, 3, Rect::new(rect.x, rect.y + 38.0, rect.width, 30.0))?
                            .clicked
                        {
                            self.card_clicks += 1;
                        }
                        Ok(())
                    })
                    .expect("card");
                }
            }
            drop(ctx);
            self.raster.end_frame().expect("end");
        }

        fn key(&mut self, key: KeyCode, shift: bool) {
            assert!(
                self.state
                    .handle_input(&WindowEvent::KeyInput {
                        key,
                        pressed: true,
                        modifiers: Modifiers {
                            shift,
                            ..Modifiers::default()
                        }
                    })
                    .consumed
            );
            self.frame(false, true);
        }

        fn button(&mut self, x: f64, y: f64, pressed: bool) {
            self.state.handle_input(&WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                modifiers: Modifiers::default(),
                x,
                y,
                click_count: 1,
                time: Default::default(),
            });
            self.frame(false, true);
        }
    }

    #[test]
    fn card_contents_edit_and_tab_without_reaching_background_controls() {
        let mut rig = Rig::new();
        assert_eq!(rig.state.focus, Some(Key::new(2, 0)));
        assert!(
            rig.state
                .handle_input(&WindowEvent::TextInput {
                    text: "brush".into()
                })
                .consumed
        );
        rig.frame(false, true);
        assert_eq!(rig.text, "brush");
        assert_eq!(rig.background, "background");
        rig.key(KeyCode::Tab, false);
        assert_eq!(rig.state.focus, Some(Key::new(3, 0)));
        rig.key(KeyCode::Enter, false);
        assert_eq!(rig.card_clicks, 1);
        rig.key(KeyCode::Tab, false);
        assert_eq!(rig.state.focus, Some(Key::new(2, 0)));
        rig.key(KeyCode::Tab, true);
        assert_eq!(rig.state.focus, Some(Key::new(3, 0)));
    }

    #[test]
    fn card_dismissal_restores_focus_and_does_not_click_the_canvas_behind_it() {
        let mut rig = Rig::new();
        rig.button(180.0, 170.0, true);
        rig.button(180.0, 170.0, false);
        assert!(rig.state.popup_open(), "empty card space is not outside");
        rig.button(290.0, 200.0, true);
        rig.button(290.0, 200.0, false);
        assert!(!rig.state.popup_open());
        assert_eq!(rig.background_clicks, 0);
        assert_eq!(rig.state.focus, Some(Key::new(1, 0)));
    }

    #[test]
    fn escape_and_omission_close_cards_without_invisible_input_capture() {
        let mut rig = Rig::new();
        rig.key(KeyCode::Escape, false);
        assert!(!rig.state.popup_open());
        assert_eq!(rig.state.focus, Some(Key::new(1, 0)));
        rig.frame(true, true);
        rig.frame(false, false);
        assert!(!rig.state.popup_open());
        assert_eq!(rig.state.focus, Some(Key::new(1, 0)));
    }

    #[test]
    fn an_empty_card_does_not_request_frames_forever() {
        let mut state = UiState::<u8>::new();
        state.set_reduced_motion(true);
        let mut raster = CpuRasterizer::new(320, 240);
        let theme = Theme::dark();
        for frame in 0..2 {
            let mut ctx = raster.begin_frame().expect("begin");
            {
                let mut ui = state.frame(&mut *ctx, &theme);
                if frame == 0 {
                    ui.open_popup_card(9, Rect::new(10.0, 10.0, 80.0, 50.0));
                }
                ui.popup_card(9, |_, _| Ok(())).expect("card");
            }
            drop(ctx);
            raster.end_frame().expect("end");
        }
        assert!(state.popup_open());
        assert!(!state.wants_frame());
    }
}
