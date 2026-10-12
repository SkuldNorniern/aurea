//! A list that opens over everything: a dropdown's options or a context
//! menu. One is open at a time, drawn as the frame ends so it covers what
//! was drawn before it and takes the pointer first.
//!
//! The rows are copied into buffers the state keeps, so an open popup does
//! not allocate frame after frame. What gets picked is handed back to the
//! widget that opened it on the next frame.

use std::mem;

use crate::AureaResult;
use crate::render::{Point, Rect};
use crate::ui::component::card;
use crate::ui::frame::{Align, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::window::KeyCode;

/// Parts a popup takes on its owner's id: the rows from here up, and the
/// backdrop just under them.
const ROW_PART: u16 = 0xF000;
const BACKDROP_PART: u16 = 0xEFFF;
/// Where an open popup's fade is kept.
const FADE: Channel = Channel::Custom(0xF0F0);
/// The tallest a popup grows before it scrolls.
const TALLEST: f32 = 360.0;

/// One row of a menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuItem<'a> {
    pub label: &'a str,
    /// Shown dimmed at the right, like `Ctrl+S`.
    pub shortcut: Option<&'a str>,
    pub enabled: bool,
    /// A tick in front: the option in use.
    pub checked: bool,
    /// A line between groups instead of a row.
    pub separator: bool,
}

impl<'a> MenuItem<'a> {
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            shortcut: None,
            enabled: true,
            checked: false,
            separator: false,
        }
    }

    pub const fn separator() -> Self {
        Self {
            label: "",
            shortcut: None,
            enabled: false,
            checked: false,
            separator: true,
        }
    }

    #[must_use]
    pub const fn shortcut(mut self, shortcut: &'a str) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    #[must_use]
    pub const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    #[must_use]
    pub const fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

impl<'a> From<&'a str> for MenuItem<'a> {
    fn from(label: &'a str) -> Self {
        Self::new(label)
    }
}

/// A row as the state keeps it, its strings reused between openings.
#[derive(Debug, Default)]
struct Row {
    label: String,
    shortcut: String,
    enabled: bool,
    checked: bool,
    separator: bool,
}

/// The open popup, if any, and the last pick.
#[derive(Debug)]
pub(crate) struct Popup<Id> {
    owner: Option<Key<Id>>,
    anchor: Rect,
    /// At least this wide: a dropdown's popup is as wide as the dropdown.
    min_width: f32,
    rows: Vec<Row>,
    len: usize,
    /// The row the keyboard is on.
    highlight: Option<usize>,
    scroll: f32,
    /// Where the keyboard goes back to once it closes.
    return_focus: Option<Key<Id>>,
    picked: Option<(Key<Id>, usize)>,
}

impl<Id> Default for Popup<Id> {
    fn default() -> Self {
        Self {
            owner: None,
            anchor: Rect::default(),
            min_width: 0.0,
            rows: Vec::new(),
            len: 0,
            highlight: None,
            scroll: 0.0,
            return_focus: None,
            picked: None,
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
        self.owner = Some(owner);
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
            row.label.clear();
            row.label.push_str(item.label);
            row.shortcut.clear();
            row.shortcut.push_str(item.shortcut.unwrap_or(""));
            row.enabled = item.enabled && !item.separator;
            row.checked = item.checked;
            row.separator = item.separator;
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

    pub(crate) fn close(&mut self) -> Option<Key<Id>> {
        self.owner = None;
        self.return_focus.take()
    }

    /// The row picked from `owner`'s popup, once.
    pub(crate) fn take_pick(&mut self, owner: Key<Id>) -> Option<usize> {
        match self.picked {
            Some((key, row)) if key == owner => {
                self.picked = None;
                Some(row)
            }
            _ => None,
        }
    }

    fn pick(&mut self, row: usize) -> Option<Key<Id>> {
        let owner = self.owner?;
        if !self.rows.get(row).is_some_and(|r| r.enabled) || row >= self.len {
            return None;
        }
        self.picked = Some((owner, row));
        self.close()
    }

    /// The next row the keyboard can land on from `from`, `step` at a time.
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

    /// A key while the popup is open. Returns whether it used the key and
    /// where the keyboard should go if the popup closed.
    pub(crate) fn key(&mut self, key: KeyCode) -> (bool, Option<Option<Key<Id>>>) {
        match key {
            KeyCode::Down => self.highlight = self.step(self.highlight, true),
            KeyCode::Up => self.highlight = self.step(self.highlight, false),
            KeyCode::Home => self.highlight = self.step(None, true),
            KeyCode::End => self.highlight = self.step(None, false),
            KeyCode::Enter | KeyCode::Space => {
                let focus = self.highlight.and_then(|row| self.pick(row));
                return (true, Some(focus.or(self.return_focus)));
            }
            KeyCode::Escape => return (true, Some(self.close())),
            KeyCode::Tab => return (false, Some(self.close())),
            _ => return (false, None),
        }
        (true, None)
    }
}

/// Sizes a popup's rows are laid out with.
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

/// Draws the open popup over the frame and follows the pointer on it.
pub(crate) fn draw<Id: WidgetId>(ui: &mut Ui<'_, Id>) -> AureaResult<()> {
    let Some(owner) = ui.state.popup.owner else {
        return Ok(());
    };
    #[allow(clippy::cast_precision_loss)]
    let canvas = Rect::new(0.0, 0.0, ui.ctx.width() as f32, ui.ctx.height() as f32);
    // Under the rows and over everything else, so a press outside closes
    // the popup instead of landing on what is behind it.
    ui.interact(owner.id, BACKDROP_PART, canvas, Sense::Hover, true);
    if ui.press(owner.id, BACKDROP_PART).is_some() {
        if let Some(focus) = ui.state.popup.close() {
            ui.state.give_focus(Some(focus));
        }
        ui.mark_changed();
        return Ok(());
    }
    let m = Metrics::of(ui.theme.control_height);
    let rect = place(ui, canvas, &m);
    let rate = ui.theme.motion.focus * 1.5;
    let shown = ui.state.tweens.from_to((owner, FADE), 0.0, 1.0, rate);
    card(ui.theme).fade(shown).paint(ui, rect)?;
    scroll(ui, rect, &m);
    ui.ctx.save()?;
    ui.ctx.clip_rect(rect.inset(1.0))?;
    let drawn = rows(ui, owner, rect, &m, shown);
    ui.ctx.restore()?;
    drawn
}

/// Where the popup goes: under its anchor, or over it when there is no
/// room below, inside the canvas.
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
        // Taken out and put back, so measuring does not copy the strings.
        let label = mem::take(&mut ui.state.popup.rows[i].label);
        let shortcut = mem::take(&mut ui.state.popup.rows[i].shortcut);
        let mut width = ui.text_width(&label, &fonts.text);
        if !shortcut.is_empty() {
            width += ui.text_width(&shortcut, &fonts.small) + pad * 2.0;
        }
        let row = &mut ui.state.popup.rows[i];
        (row.label, row.shortcut) = (label, shortcut);
        widest = widest.max(width);
    }
    let check = m.row * 0.75;
    let width = (widest + check + pad * 2.0)
        .max(min_width)
        .min(canvas.width - 8.0);
    let height = height.min(TALLEST).min(canvas.height - 8.0);
    let x = anchor.x.clamp(4.0, (canvas.right() - width - 4.0).max(4.0));
    let below = anchor.bottom() + 4.0;
    let y = if below + height <= canvas.bottom() - 4.0 {
        below
    } else {
        (anchor.y - 4.0 - height).max(4.0)
    };
    Rect::new(x, y, width, height)
}

/// Wheel over the popup scrolls it.
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
}

fn rows<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    owner: Key<Id>,
    rect: Rect,
    m: &Metrics,
    shown: f32,
) -> AureaResult<()> {
    let p = ui.theme.palette;
    let pad = ui.theme.padding;
    let mut y = rect.y + m.pad - ui.state.popup.scroll;
    for i in 0..ui.state.popup.len {
        let row = &ui.state.popup.rows[i];
        let (separator, enabled, checked) = (row.separator, row.enabled, row.checked);
        let h = m.height(separator);
        let cell = Rect::new(rect.x + m.pad, y, rect.width - m.pad * 2.0, h);
        y += h;
        if separator {
            let line = Rect::new(cell.x + pad / 2.0, cell.center().y, cell.width - pad, 1.0);
            ui.fill(line, p.border.fade(shown))?;
            continue;
        }
        if cell.bottom() < rect.y || cell.y > rect.bottom() {
            continue;
        }
        let part = ROW_PART.saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
        let input = ui.interact(owner.id, part, cell, Sense::Click, enabled);
        if input.hovered {
            ui.state.popup.highlight = Some(i);
        }
        if input.clicked {
            let focus = ui.state.popup.pick(i);
            ui.state.give_focus(focus);
            ui.mark_changed();
            return Ok(());
        }
        let lit = ui.state.popup.highlight == Some(i) && enabled;
        if lit {
            ui.round(cell, ui.theme.radius.min(5.0), p.hover.fade(shown))?;
        }
        let ink = if enabled { p.text } else { p.faint };
        let check = Rect::new(cell.x, cell.y, m.row * 0.75, h);
        if checked {
            Mark::new(MarkShape::Check, p.accent.fade(shown)).paint(ui, check.inset(4.0))?;
        }
        let text = Rect::new(
            check.right(),
            cell.y,
            cell.right() - check.right() - pad / 2.0,
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
    }
    Ok(())
}

impl<Id: WidgetId> Ui<'_, Id> {
    /// Opens a context menu for `id` at `at`, usually where the secondary
    /// button went down. The row picked comes back from
    /// [`Ui::menu_picked`] on a later frame.
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

    /// The row picked from `id`'s context menu, once.
    pub fn menu_picked(&mut self, id: Id) -> Option<usize> {
        self.state.popup.take_pick(Key::new(id, MENU_PART))
    }

    /// Whether `id`'s context menu is open.
    pub fn menu_open(&self, id: Id) -> bool {
        self.state.popup.is_open_for(Key::new(id, MENU_PART))
    }

    /// Closes whatever popup is open.
    pub fn close_popup(&mut self) {
        if let Some(focus) = self.state.popup.close() {
            self.state.give_focus(Some(focus));
        }
    }
}

/// The part a context menu is opened on, kept clear of a widget's own.
const MENU_PART: u16 = 0xFFFE;
