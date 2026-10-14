//! A list that opens over everything: a dropdown's options or a context menu.

use std::iter;
use std::mem;

use crate::AureaResult;
use crate::render::{Point, Rect};
use crate::ui::component::card;
use crate::ui::frame::{Align, Ui};
use crate::ui::paint::{Component, Label, Mark, MarkShape};
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::window::KeyCode;

const ROW_PART: u16 = 0xF000;
const BACKDROP_PART: u16 = 0xEFFF;
const CARD_PART: u16 = 0xEFFE;
const FADE: Channel = Channel::Custom(0xF0F0);
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

#[derive(Debug, Default)]
struct Row {
    label: String,
    shortcut: String,
    enabled: bool,
    checked: bool,
    separator: bool,
}

#[derive(Debug)]
pub(crate) struct Popup<Id> {
    owner: Option<Key<Id>>,
    anchor: Rect,
    min_width: f32,
    rows: Vec<Row>,
    len: usize,
    highlight: Option<usize>,
    scroll: f32,
    return_focus: Option<Key<Id>>,
    picked: Option<(Key<Id>, usize)>,
    card: bool,
    card_drawn: bool,
    focus_order: Vec<Key<Id>>,
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
            card: false,
            card_drawn: false,
            focus_order: Vec::new(),
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

    pub(crate) fn is_card(&self) -> bool {
        self.is_open() && self.card
    }

    pub(crate) fn owns_focus(&self, focus: Option<Key<Id>>) -> bool {
        focus.is_some_and(|key| self.focus_order.contains(&key))
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
        self.owner = None;
        self.return_focus.take()
    }

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
