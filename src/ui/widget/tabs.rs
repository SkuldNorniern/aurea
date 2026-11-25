//! A tab bar: labels in a row, each sized to its text, and a line under the picked one that slides
//! to the next. Tabs that do not fit follow the bar's [`TabOverflow`].

use super::Widget;
use super::look::Look;
use super::scroll::{scroll_into_view, wheel_axis};
use crate::AureaResult;
use crate::render::{Color, Point, Rect};
use crate::ui::MenuItem;
use crate::ui::accessibility::Role;
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{Component, Label, Surface, TextSize};
use crate::ui::state::{Channel, Interaction, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

const LINE_X: u16 = u16::MAX;
const LINE_W: u16 = u16::MAX - 1;
const MORE_PART: u16 = u16::MAX - 2;
const LEFT_PART: u16 = u16::MAX - 3;
const RIGHT_PART: u16 = u16::MAX - 4;
/// Tabs take parts up to here; the bar's own state sits past them.
const TABS_END: u16 = u16::MAX - 5;
/// The bar's own stored values: scroll, drag anchor and the pick last seen.
const STATE_PART: u16 = u16::MAX - 6;
const SCROLL: Channel = Channel::Custom(0x7AB0);
const ANCHOR: Channel = Channel::Custom(0x7AB1);
const SEEN: Channel = Channel::Custom(0x7AB2);
const MORE_LABEL: &str = "More";
/// Logical pixels a pointer must move before a press on a tab scrolls the row.
const DRAG_SLOP: f32 = 6.0;
/// Width of each edge zone that scrolls the row one step.
const EDGE: f32 = 16.0;
const EDGE_STEP: f32 = 96.0;

/// What a tab bar does with tabs that do not fit its width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabOverflow {
    /// Tabs narrow to [`TabStyle::min_width`] and their labels end in an ellipsis. When the
    /// narrowest still do not fit, the row scrolls as with [`Self::Scroll`].
    Shrink,
    /// The row scrolls: the wheel, a drag or a touch moves it, and the edge zones step it. A move
    /// from the keyboard, or a pick the app changes, scrolls the pick into view.
    #[default]
    Scroll,
    /// The tabs that do not fit go behind a trailing "More" button, which opens a menu of them.
    /// Picking one shows it in place of the last tab shown.
    Menu,
}

/// How a tab bar looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabStyle {
    /// Tab fill by state.
    pub tab: StateColors,
    /// Unpicked label colour.
    pub text: Color,
    /// Picked label colour.
    pub picked_text: Color,
    /// Line under the picked tab.
    pub line: Color,
    /// Under the whole bar, across its width.
    pub rule: Color,
    /// Line thickness.
    pub line_width: f32,
    /// Space either side of a label.
    pub padding: f32,
    /// Space between tabs.
    pub gap: f32,
    /// Corner radius of each tab.
    pub radius: f32,
    /// Label size.
    pub text_size: TextSize,
    /// The narrowest a tab shrinks to under [`TabOverflow::Shrink`], padding included.
    pub min_width: f32,
}

impl TabStyle {
    /// Default style from the theme.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            tab: StateColors::new(p.hover.with_alpha(0), p.hover, p.pressed),
            text: p.dim,
            picked_text: p.text,
            line: p.accent,
            rule: p.border,
            line_width: 2.0,
            padding: theme.padding * 1.2,
            gap: 2.0,
            radius: theme.radius.min(6.0),
            text_size: TextSize::Strong,
            min_width: 56.0,
        }
    }
}

/// One tab of a [`TabBar`], for its painter.
#[derive(Clone, Copy, Debug)]
pub struct Tab<'v> {
    /// The tab's text.
    pub label: &'v str,
    /// Its visible part, which the face and label are drawn in: empty when it is not shown, and cut
    /// at the bar's edges when it is partly scrolled off.
    pub rect: Rect,
    /// Its hover, press and focus, eased.
    pub state: Eased,
    /// How far it is picked, 0 unpicked to 1 picked, eased.
    pub picked: f32,
}

/// What a [`TabBar`] draws, from [`TabBar::look`].
///
/// The tabs are one [`Look`] between them: the line under the picked tab slides across the bar,
/// so it is shared, and each tab is a [`Tab`] in `tabs`, from the left.
#[derive(Clone, Debug)]
pub struct TabBarParts<'v> {
    /// The tabs, from the left, shown or not.
    pub tabs: Vec<Tab<'v>>,
    /// The line under the picked tab, when it is shown wide enough and in the bar.
    pub line: Option<Rect>,
    /// The rule across the bottom of the bar.
    pub rule: Rect,
    /// The More button, under [`TabOverflow::Menu`] while tabs are behind it.
    pub more: Option<Tab<'v>>,
    /// The zones at the left and right edge that step a scrolled row, when it can go that way.
    pub edges: [Option<Rect>; 2],
    /// Whether tabs take input; the default look fades when not.
    pub enabled: bool,
    /// The style it draws with, the theme's unless one was given.
    pub style: TabStyle,
}

/// `labels` in a row from the rect's left, the one at `selected` picked.
///
/// Tabs that do not fit follow the [`TabOverflow`] set with [`TabBar::overflow`]; the default
/// scrolls the row. Whatever the bar, the arrow keys and Home and End never leave the pick on a tab
/// that is not shown: it is scrolled in, or, under `Menu`, swapped in for the last tab shown. A
/// tab hit or drawn is only its visible part, and the accessibility tree lists every tab, shown or
/// not, with the pick set on the one picked.
#[derive(Debug)]
pub struct TabBar<'v> {
    labels: &'v [&'v str],
    selected: &'v mut usize,
    style: Option<TabStyle>,
    enabled: bool,
    overflow: TabOverflow,
}

impl<'v> TabBar<'v> {
    /// Creates a tab bar for `labels`, scrolling when they do not fit.
    pub fn new(labels: &'v [&'v str], selected: &'v mut usize) -> Self {
        Self {
            labels,
            selected,
            style: None,
            enabled: true,
            overflow: TabOverflow::default(),
        }
    }

    /// Sets the look; defaults to the theme's.
    #[must_use]
    pub fn style(mut self, style: TabStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Whether tabs take input.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// What happens to tabs that do not fit; see [`TabOverflow`].
    #[must_use]
    pub fn overflow(mut self, overflow: TabOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// Draws the tab bar in `rect`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }
}

impl<'v> TabBar<'v> {
    /// Takes the input as [`TabBar::show`] does and works out what it would draw, without drawing:
    /// the app draws it, or [`Look::paint_default`] does, or its pieces [`Look::paint_rule`],
    /// [`Look::paint_tab`], [`Look::paint_line`], [`Look::paint_more`] and [`Look::paint_edges`].
    /// The picked tab and the line are as the bar stood when the frame began; a click picks from
    /// the next frame's line.
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> Look<TabBarParts<'v>> {
        let style = self.style.unwrap_or_else(|| TabStyle::new(ui.theme));
        let n = self.labels.len();
        let nat: Vec<f32> = self
            .labels
            .iter()
            .map(|&label| {
                Label::new(label, style.text)
                    .size(style.text_size)
                    .width(ui)
                    + style.padding * 2.0
            })
            .collect();
        let more_w = Label::new(MORE_LABEL, style.text)
            .size(style.text_size)
            .width(ui)
            + style.padding * 2.0;
        let mut response = Response {
            rect,
            ..Response::default()
        };

        let mut selected = *self.selected;
        if pick_by_keys(ui, id, n, &mut selected) {
            response.changed = true;
            ui.mark_changed();
        }
        // a pick that moved since the last frame, by keys or by the app, is scrolled to
        let seen = ui.animated(id, STATE_PART, SEEN);
        let moved = seen != Some(index_f32(selected));
        ui.set_animated(id, STATE_PART, SEEN, index_f32(selected));

        let strip = strip(
            self.overflow,
            &nat,
            selected,
            rect.width,
            style.gap,
            style.min_width,
            more_w + style.gap,
        );
        let max = if strip.scrolls {
            (strip.width - rect.width).max(0.0)
        } else {
            0.0
        };
        let to = strip.cells.get(selected).copied().flatten();
        let (offset, scrolling) = scroll(ui, id, n, rect, max, moved.then_some(to).flatten());

        let (states, clicked, flags) = tab_states(
            ui,
            id,
            self.labels,
            &strip,
            (rect, offset),
            selected,
            self.enabled,
        );
        response.hovered |= flags.0;
        response.focused |= flags.1;
        // a drag that scrolled the row is not a click on the tab it began on
        if let Some(i) = clicked.filter(|_| !scrolling)
            && i != selected
        {
            selected = i;
            response.changed = true;
            ui.mark_changed();
            if response.focused {
                ui.state.give_focus(Some(Key::new(id, tab_part(i))));
            }
        }

        let mut more = None;
        if strip.more {
            let at = Rect::new(rect.right() - more_w, rect.y, more_w, rect.height);
            let shown = more_button(ui, id, at, self.enabled, self.labels, &strip);
            response.hovered |= shown.1.hovered;
            response.focused |= shown.1.focused;
            more = Some(shown.0);
        }
        if let Some(i) = ui
            .menu_picked_tag(id)
            .and_then(|tag| usize::try_from(tag).ok())
            .filter(|&i| i < n && i != selected)
        {
            selected = i;
            response.changed = true;
            ui.mark_changed();
        }
        if selected != *self.selected {
            *self.selected = selected;
        }

        let (edges, offset) = edge_zones(ui, id, rect, offset, max, self.enabled);
        ui.set_animated(id, STATE_PART, SCROLL, offset);
        let line = slide_line(
            ui,
            id,
            rect,
            strip.cells.get(selected).copied().flatten(),
            offset,
            style,
        );

        let tabs = states
            .into_iter()
            .enumerate()
            .map(|(i, (label, state, picked))| Tab {
                label,
                rect: visible(rect, strip.cells[i], offset),
                state,
                picked,
            })
            .collect();
        Look {
            rect,
            // no input of its own: the bar is as enabled as it was asked to be
            state: Eased {
                enabled: f32::from(u8::from(self.enabled)),
                ..Eased::default()
            },
            held_at: None,
            response,
            parts: TabBarParts {
                tabs,
                line,
                rule: Rect::new(rect.x, rect.bottom() - 1.0, rect.width, 1.0),
                more,
                edges,
                enabled: self.enabled,
                style,
            },
        }
    }
}

/// Moves `selected` by the arrow keys, Home and End from the focused tab, and gives the keyboard to
/// the tab it lands on. Whether it moved.
fn pick_by_keys<Id: WidgetId>(ui: &mut Ui<'_, Id>, id: Id, n: usize, selected: &mut usize) -> bool {
    let last = n.saturating_sub(1);
    let Some(focus) = (0..n).find(|&i| ui.state.focus == Some(Key::new(id, tab_part(i)))) else {
        return false;
    };
    let mut target = None;
    for (key, _) in ui.keys() {
        let at = target.unwrap_or(focus);
        target = match key {
            KeyCode::Right => Some((at + 1).min(last)),
            KeyCode::Left => Some(at.saturating_sub(1)),
            KeyCode::Home => Some(0),
            KeyCode::End => Some(last),
            _ => target,
        };
    }
    match target.filter(|&to| to != *selected) {
        Some(to) => {
            *selected = to;
            ui.state.give_focus(Some(Key::new(id, tab_part(to))));
            ui.mark_changed();
            true
        }
        None => false,
    }
}

/// The row's offset into its strip, moved by the wheel, by a drag from a tab, and to bring `to` into
/// view when the pick moved. Also whether a drag scrolled it, so the press is no click.
fn scroll<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    n: usize,
    rect: Rect,
    max: f32,
    to: Option<Span>,
) -> (f32, bool) {
    let mut offset = ui
        .animated(id, STATE_PART, SCROLL)
        .unwrap_or(0.0)
        .clamp(0.0, max);
    if let Some(cell) = to {
        scroll_into_view(&mut offset, cell.x, cell.right(), rect.width);
    }
    if max > 0.0 {
        let (wx, wy) = ui.wheel_in(rect);
        let (dx, dy) = wheel_axis(wx, wy);
        offset += dx - dy;
    }
    let mut scrolling = false;
    match (0..n).find_map(|i| ui.held_at(id, tab_part(i))) {
        Some(from) => {
            // the content point under the press stays under the pointer
            let anchor = ui
                .animated(id, STATE_PART, ANCHOR)
                .filter(|a| !a.is_nan())
                .unwrap_or(from.x + offset);
            ui.set_animated(id, STATE_PART, ANCHOR, anchor);
            if let Some(at) = ui.pointer()
                && max > 0.0
                && (at.x - from.x).abs() > DRAG_SLOP
            {
                offset = anchor - at.x;
                scrolling = true;
            }
        }
        None => ui.set_animated(id, STATE_PART, ANCHOR, f32::NAN),
    }
    (offset.clamp(0.0, max), scrolling)
}

/// Each tab's input and eased look, its label, with the tab a press landed on, and whether any
/// tab is hovered and focused.
/// A tab's label, eased look and how far it is picked.
type TabLook<'v> = (&'v str, Eased, f32);

fn tab_states<'v, Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    labels: &[&'v str],
    strip: &Strip,
    (rect, offset): (Rect, f32),
    selected: usize,
    enabled: bool,
) -> (Vec<TabLook<'v>>, Option<usize>, (bool, bool)) {
    let mut states = Vec::with_capacity(labels.len());
    let mut clicked = None;
    let mut flags = (false, false);
    for (i, &label) in labels.iter().enumerate() {
        let part = tab_part(i);
        let cell = visible(rect, strip.cells[i], offset);
        let input = ui.interact(id, part, cell, Sense::Click, enabled);
        let on = selected == i;
        ui.accessible_description(id, part, Role::Tab, label, None);
        ui.with_accessible(id, part, |node| node.set_selected(on));
        flags.0 |= input.hovered;
        flags.1 |= input.focused;
        if input.clicked && !on {
            clicked = Some(i);
        }
        let state = ui.eased(id, part, input);
        let lit = ui.animate(
            id,
            part,
            Channel::Value,
            f32::from(u8::from(on)),
            ui.theme.motion.hover,
        );
        states.push((label, state, lit));
    }
    (states, clicked, flags)
}

/// The More button, which opens a menu of the tabs behind it, with its look and input.
fn more_button<'v, Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    at: Rect,
    enabled: bool,
    labels: &[&'v str],
    strip: &Strip,
) -> (Tab<'v>, Interaction) {
    let input = ui.interact(id, MORE_PART, at, Sense::Click, enabled);
    ui.accessible_description(id, MORE_PART, Role::Button, MORE_LABEL, None);
    if input.clicked {
        let items: Vec<MenuItem<'v>> = labels
            .iter()
            .enumerate()
            .filter(|(i, _)| strip.cells[*i].is_none())
            .map(|(i, &label)| MenuItem {
                tag: index_tag(i),
                ..MenuItem::new(label)
            })
            .collect();
        ui.open_menu(id, Point::new(at.x, at.bottom()), items);
    }
    let state = ui.eased(id, MORE_PART, input);
    let tab = Tab {
        label: MORE_LABEL,
        rect: at,
        state,
        picked: f32::from(u8::from(ui.menu_open(id))),
    };
    (tab, input)
}

/// The zones at each edge that step a scrolled row, and the offset after a press on one.
fn edge_zones<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    rect: Rect,
    mut offset: f32,
    max: f32,
    enabled: bool,
) -> ([Option<Rect>; 2], f32) {
    let mut zones = [None, None];
    if max > 0.0 {
        if offset > 0.0 {
            zones[0] = Some(Rect::new(rect.x, rect.y, EDGE.min(rect.width), rect.height));
        }
        if offset < max {
            let x = (rect.right() - EDGE).max(rect.x);
            zones[1] = Some(Rect::new(x, rect.y, EDGE.min(rect.width), rect.height));
        }
    }
    for (part, zone) in [(LEFT_PART, zones[0]), (RIGHT_PART, zones[1])] {
        let Some(zone) = zone else { continue };
        let input = ui.interact_without_focus(id, part, zone, Sense::Click, enabled);
        if input.clicked {
            offset += if part == LEFT_PART {
                -EDGE_STEP
            } else {
                EDGE_STEP
            };
        }
    }
    (zones, offset.clamp(0.0, max))
}

/// The line under the picked tab's visible part, sliding to it. None for a tab not shown, or
/// narrower than its padding.
fn slide_line<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    rect: Rect,
    cell: Option<Span>,
    offset: f32,
    style: TabStyle,
) -> Option<Rect> {
    let cell = cell.filter(|cell| cell.w >= style.padding)?;
    let left = (rect.x + cell.x - offset + style.padding * 0.5).max(rect.x);
    let right = (rect.x + cell.right() - offset - style.padding * 0.5).min(rect.right());
    if right <= left {
        return None;
    }
    let rate = ui.theme.motion.slide;
    let lx = ui
        .animate(id, LINE_X, Channel::Value, left, rate)
        .clamp(rect.x, rect.right());
    let lw = ui
        .animate(id, LINE_W, Channel::Value, right - left, rate)
        .clamp(0.0, rect.right() - lx);
    Some(Rect::new(
        lx,
        rect.bottom() - style.line_width,
        lw,
        style.line_width,
    ))
}
impl Look<TabBarParts<'_>> {
    /// The rule, each tab from the left, the line under the picked one, the More button and the
    /// edge zones.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_rule(ui)?;
        for i in 0..self.parts.tabs.len() {
            self.paint_tab(ui, i)?;
        }
        self.paint_line(ui)?;
        self.paint_more(ui)?;
        self.paint_edges(ui)
    }

    /// The rule across the bottom of the bar.
    pub fn paint_rule<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        ui.fill(self.parts.rule, self.parts.style.rule)
    }

    /// Tab `i`'s face, focus ring and label, its text picked as the line lands on it. Nothing for a
    /// tab that is not shown.
    pub fn paint_tab<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, i: usize) -> AureaResult<()> {
        match self.parts.tabs.get(i) {
            Some(tab) if tab.rect.width > 0.0 => self.paint_face(ui, tab),
            _ => Ok(()),
        }
    }

    /// The More button, when tabs are behind it.
    pub fn paint_more<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        match &self.parts.more {
            Some(tab) => self.paint_face(ui, tab),
            None => Ok(()),
        }
    }

    /// The line under the picked tab, sliding between tabs.
    pub fn paint_line<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        match self.parts.line {
            Some(line) => ui.round(
                line,
                self.parts.style.line_width / 2.0,
                self.parts.style.line,
            ),
            None => Ok(()),
        }
    }

    /// The edge zones of a scrolled row: a chevron on each side the row can scroll to.
    pub fn paint_edges<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let color = self.parts.style.text;
        for (i, zone) in self.parts.edges.iter().enumerate() {
            let Some(zone) = *zone else { continue };
            ui.fill(zone, ui.theme.palette.surface)?;
            let cx = zone.x + zone.width / 2.0;
            let cy = zone.y + zone.height / 2.0;
            let (tip, back) = if i == 0 {
                (cx - 2.5, cx + 2.5)
            } else {
                (cx + 2.5, cx - 2.5)
            };
            let points = [
                Point::new(back, cy - 4.0),
                Point::new(tip, cy),
                Point::new(back, cy + 4.0),
            ];
            ui.polyline(&points, 1.5, color)?;
        }
        Ok(())
    }

    fn paint_face<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, tab: &Tab<'_>) -> AureaResult<()> {
        let style = &self.parts.style;
        let face = Rect::new(
            tab.rect.x,
            tab.rect.y + 3.0,
            tab.rect.width,
            tab.rect.height - 6.0,
        );
        Surface::new(style.tab.at(tab.state))
            .radius(style.radius)
            .paint(ui, face)?;
        FocusRing::new(ui.theme, style.radius, tab.state.focus).paint(ui, face)?;
        let color = style
            .text
            .mix(style.picked_text, tab.picked)
            .fade(tab.state.enabled);
        Label::new(tab.label, color)
            .size(style.text_size)
            .centered()
            .paint(ui, tab.rect)
    }
}

impl<Id: WidgetId> Widget<Id> for TabBar<'_> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let look = self.look(ui, id, rect);
        look.paint_default(ui)?;
        Ok(look.response)
    }
}

/// A tab's part in its bar.
fn tab_part(i: usize) -> u16 {
    u16::try_from(i).unwrap_or(TABS_END).min(TABS_END)
}

#[allow(clippy::cast_precision_loss)]
fn index_f32(i: usize) -> f32 {
    i as f32
}

fn index_tag(i: usize) -> Option<u64> {
    u64::try_from(i).ok()
}

/// A tab's place in the strip, from the strip's left, before any scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    x: f32,
    w: f32,
}

impl Span {
    fn right(self) -> f32 {
        self.x + self.w
    }
}

/// Where each tab goes in a bar: its span in the strip, `None` when it is not shown.
#[derive(Clone, Debug, PartialEq)]
struct Strip {
    cells: Vec<Option<Span>>,
    /// The row can scroll: its content is `width` wide.
    scrolls: bool,
    /// Tabs are behind the More button, which takes room at the right.
    more: bool,
    /// Width of the whole row.
    width: f32,
}

/// The strip for tabs `nat` wide, in a bar `width` wide, the pick at `selected`. `more` is the room
/// the More button and its gap take, kept for it when tabs are behind it.
fn strip(
    overflow: TabOverflow,
    nat: &[f32],
    selected: usize,
    width: f32,
    gap: f32,
    min: f32,
    more: f32,
) -> Strip {
    let n = nat.len();
    let total = nat.iter().sum::<f32>() + gap * index_f32(n.saturating_sub(1));
    if total <= width {
        return plain(nat, gap, false);
    }
    match overflow {
        TabOverflow::Scroll => plain(nat, gap, true),
        TabOverflow::Shrink => {
            shrink_strip(nat, width, gap, min).unwrap_or_else(|| plain(nat, gap, true))
        }
        TabOverflow::Menu => menu_strip(nat, selected, width, gap, more),
    }
}

/// Every tab at its natural width in a row.
fn plain(nat: &[f32], gap: f32, scrolls: bool) -> Strip {
    let width = nat.iter().sum::<f32>() + gap * index_f32(nat.len().saturating_sub(1));
    Strip {
        cells: row(nat, gap).into_iter().map(Some).collect(),
        scrolls,
        more: false,
        width,
    }
}

/// Tabs narrowed to one cap so they total the bar, or None when even `min` each does not fit.
fn shrink_strip(nat: &[f32], width: f32, gap: f32, min: f32) -> Option<Strip> {
    let n = nat.len();
    let room = width - gap * index_f32(n.saturating_sub(1));
    if min * index_f32(n) > room {
        return None;
    }
    let cap = cap(nat, room, min);
    let widths: Vec<f32> = nat.iter().map(|w| w.min(cap)).collect();
    Some(Strip {
        cells: row(&widths, gap).into_iter().map(Some).collect(),
        scrolls: false,
        more: false,
        width: widths.iter().sum::<f32>() + gap * index_f32(n.saturating_sub(1)),
    })
}

/// The tabs that fit before the More button, with the pick swapped in for the last one shown when
/// it is among those behind.
fn menu_strip(nat: &[f32], selected: usize, width: f32, gap: f32, more: f32) -> Strip {
    let n = nat.len();
    let avail = width - more;
    let mut used = 0.0;
    let mut k = 0;
    for (i, w) in nat.iter().enumerate() {
        let add = if i == 0 { *w } else { gap + w };
        if used + add > avail {
            break;
        }
        used += add;
        k += 1;
    }
    let mut shown: Vec<bool> = (0..n).map(|i| i < k).collect();
    if selected < n && selected >= k {
        if k > 0 {
            shown[k - 1] = false;
        }
        shown[selected] = true;
    }
    let limit = width - more;
    let mut cells = vec![None; n];
    let mut x = 0.0;
    for i in (0..n).filter(|&i| shown[i]) {
        let room = limit - x;
        if room <= 0.0 {
            break;
        }
        let w = nat[i].min(room);
        cells[i] = Some(Span { x, w });
        x += w + gap;
    }
    Strip {
        cells,
        scrolls: false,
        more: true,
        width: nat.iter().sum::<f32>() + gap * index_f32(n.saturating_sub(1)),
    }
}

/// Tabs `widths` wide one after another, `gap` apart, from 0.
fn row(widths: &[f32], gap: f32) -> Vec<Span> {
    let mut x = 0.0;
    widths
        .iter()
        .map(|&w| {
            let span = Span { x, w };
            x += w + gap;
            span
        })
        .collect()
}

/// The widest cap every tab wider than it is cut to, so that the cut tabs total `room`. Never
/// under `min`, which the caller has checked fits.
fn cap(nat: &[f32], room: f32, min: f32) -> f32 {
    let widest = nat.iter().copied().fold(0.0, f32::max);
    let (mut lo, mut hi) = (min, widest);
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        let used: f32 = nat.iter().map(|w| w.min(mid)).sum();
        if used > room {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    lo
}

/// The part of tab `cell` the bar shows, at `offset` into the strip; empty when none is.
fn visible(bar: Rect, cell: Option<Span>, offset: f32) -> Rect {
    let empty = Rect::new(bar.x, bar.y, 0.0, bar.height);
    let Some(span) = cell else { return empty };
    let left = (bar.x + span.x - offset).max(bar.x);
    let right = (bar.x + span.right() - offset).min(bar.right());
    if right <= left {
        empty
    } else {
        Rect::new(left, bar.y, right - left, bar.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::ui::rig::Rig;
    use crate::window::{Modifiers, MouseButton, PointerKind, WindowEvent};

    const LONG: [&str; 6] = ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"];
    const BAR: Rect = Rect::new(0.0, 0.0, 180.0, 36.0);

    /// What a frame of the bar showed.
    struct Drawn {
        tabs: Vec<Rect>,
        line: Option<Rect>,
        more: Option<Rect>,
        edges: [Option<Rect>; 2],
        menu: bool,
    }

    fn draw(rig: &mut Rig, picked: &mut usize, overflow: TabOverflow) -> Drawn {
        rig.frame(|ui| {
            let look = TabBar::new(&LONG, picked)
                .overflow(overflow)
                .look(ui, 1, BAR);
            Drawn {
                tabs: look.parts.tabs.iter().map(|t| t.rect).collect(),
                line: look.parts.line,
                more: look.parts.more.map(|t| t.rect),
                edges: look.parts.edges,
                menu: ui.menu_open(1),
            }
        })
    }

    fn frame(raster: &mut CpuRasterizer, state: &mut UiState<u8>, picked: &mut usize) {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("frame");
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            TabBar::new(&["General", "Audio", "Video"], picked)
                .show(&mut ui, 1, Rect::new(0.0, 0.0, 300.0, 36.0))
                .expect("draw");
        }
        drop(ctx);
        raster.end_frame().expect("end");
    }

    #[test]
    fn a_click_picks_a_tab_and_the_arrows_walk_them() {
        let (mut raster, mut state, mut picked) = (CpuRasterizer::new(300, 36), UiState::new(), 0);
        frame(&mut raster, &mut state, &mut picked);
        for pressed in [true, false] {
            state.handle(&WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                modifiers: Modifiers::default(),
                x: 5.0,
                y: 18.0,
                click_count: 1,
                time: Default::default(),
                pointer: PointerKind::Mouse,
            });
            frame(&mut raster, &mut state, &mut picked);
        }
        assert_eq!(picked, 0);
        for key in [
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Left,
        ] {
            state.handle(&WindowEvent::KeyInput {
                key,
                pressed: true,
                modifiers: Modifiers::default(),
            });
            frame(&mut raster, &mut state, &mut picked);
        }
        assert_eq!(picked, 1);
    }

    #[test]
    fn look_draws_the_apps_look_and_still_picks() {
        let mut rig: Rig = Rig::new(300, 36);
        let at = Rect::new(0.0, 0.0, 300.0, 36.0);
        let mut picked = 2;
        let draw = |rig: &mut Rig, picked: &mut usize| {
            rig.frame(|ui| {
                let bar = TabBar::new(&["General", "Audio", "Video"], picked).look(ui, 1, at);
                ui.fill(bar.rect, Color::rgb(0, 200, 0)).expect("draw");
            });
        };
        draw(&mut rig, &mut picked);
        assert_eq!(rig.pixel(150, 10), [0, 200, 0, 255], "the app's look");
        rig.at(5.0, 18.0);
        draw(&mut rig, &mut picked);
        rig.button(5.0, 18.0, true);
        draw(&mut rig, &mut picked);
        rig.button(5.0, 18.0, false);
        draw(&mut rig, &mut picked);
        assert_eq!(picked, 0, "the click still picks the first tab");
    }

    #[test]
    fn scroll_keeps_every_tab_and_the_row_as_wide_as_they_are() {
        let s = strip(
            TabOverflow::Scroll,
            &[50.0, 60.0, 70.0],
            0,
            100.0,
            2.0,
            40.0,
            30.0,
        );
        assert!(s.scrolls && !s.more);
        assert!(s.cells.iter().all(Option::is_some));
        assert_eq!(s.width, 50.0 + 60.0 + 70.0 + 4.0);
    }

    #[test]
    fn shrink_narrows_the_widest_tabs_to_fit_and_keeps_the_rest() {
        let s = strip(
            TabOverflow::Shrink,
            &[40.0, 200.0, 200.0],
            0,
            200.0,
            2.0,
            50.0,
            30.0,
        );
        let w: Vec<f32> = s.cells.iter().map(|c| c.expect("shown").w).collect();
        assert_eq!(w[0], 40.0);
        assert!((w[1] - 78.0).abs() < 1e-2 && (w[2] - w[1]).abs() < 1e-3);
        assert!(w.iter().sum::<f32>() + 4.0 <= 200.0 + 1e-3);
        assert!(!s.scrolls && !s.more);
    }

    #[test]
    fn shrink_scrolls_when_even_the_minimums_do_not_fit() {
        let s = strip(TabOverflow::Shrink, &[90.0; 3], 0, 150.0, 2.0, 60.0, 30.0);
        assert!(s.scrolls);
        assert_eq!(s.width, 90.0 * 3.0 + 4.0);
    }

    #[test]
    fn menu_hides_what_does_not_fit_and_swaps_the_pick_in() {
        let nat = [50.0; 6];
        let s = strip(TabOverflow::Menu, &nat, 0, 200.0, 2.0, 40.0, 40.0);
        assert!(s.more && !s.scrolls);
        assert!(s.cells[..3].iter().all(Option::is_some));
        assert!(s.cells[3..].iter().all(Option::is_none));
        let s = strip(TabOverflow::Menu, &nat, 5, 200.0, 2.0, 40.0, 40.0);
        assert!(s.cells[5].is_some(), "the pick is shown");
        assert!(
            s.cells[2].is_none(),
            "and the tab it took the place of is not"
        );
        assert!(s.cells[5].expect("shown").right() <= 160.0);
    }

    #[test]
    fn menu_with_every_tab_fitting_has_no_more() {
        let s = strip(TabOverflow::Menu, &[50.0, 50.0], 1, 200.0, 2.0, 40.0, 40.0);
        assert!(!s.more && s.cells.iter().all(Option::is_some));
    }

    #[test]
    fn arrows_walk_the_pick_and_scroll_it_into_view() {
        let mut rig: Rig = Rig::new(180, 36).still();
        let mut picked = 0;
        draw(&mut rig, &mut picked, TabOverflow::Scroll);
        rig.key(KeyCode::Tab);
        draw(&mut rig, &mut picked, TabOverflow::Scroll);
        for _ in 0..5 {
            rig.key(KeyCode::Right);
            draw(&mut rig, &mut picked, TabOverflow::Scroll);
        }
        assert_eq!(picked, 5);
        let shown = draw(&mut rig, &mut picked, TabOverflow::Scroll).tabs[5];
        assert!(
            shown.x >= BAR.x && shown.right() <= BAR.right() + 1e-3,
            "{shown:?}"
        );
        assert!(shown.width > 0.0);
    }

    #[test]
    fn the_wheel_scrolls_the_row_and_shows_an_edge() {
        let mut rig: Rig = Rig::new(180, 36).still();
        let mut picked = 0;
        let before = draw(&mut rig, &mut picked, TabOverflow::Scroll);
        assert!(before.edges[0].is_none() && before.edges[1].is_some());
        rig.wheel(10.0, 18.0, 0.0, -3.0);
        let after = draw(&mut rig, &mut picked, TabOverflow::Scroll);
        assert!(after.edges[0].is_some(), "the row has moved on");
        assert!(after.tabs[0].width < before.tabs[0].width || after.tabs[0].width == 0.0);
        assert_eq!(picked, 0, "scrolling does not pick");
    }

    #[test]
    fn the_line_stays_in_the_bar_when_its_tab_is_cut() {
        let mut rig: Rig = Rig::new(180, 36).still();
        let mut picked = 5;
        for _ in 0..3 {
            let d = draw(&mut rig, &mut picked, TabOverflow::Menu);
            if let Some(line) = d.line {
                assert!(
                    line.x >= BAR.x && line.right() <= BAR.right() + 1e-3,
                    "{line:?}"
                );
            }
        }
    }

    #[test]
    fn the_more_button_opens_a_menu_of_the_tabs_behind_it() {
        let mut rig: Rig = Rig::new(180, 36).still();
        let mut picked = 0;
        let d = draw(&mut rig, &mut picked, TabOverflow::Menu);
        let more = d.more.expect("more");
        assert_eq!(d.tabs[5].width, 0.0, "a tab behind it is not shown");
        rig.click(more.x + more.width / 2.0, 18.0);
        let d = draw(&mut rig, &mut picked, TabOverflow::Menu);
        assert!(d.menu, "the menu opened");
    }
}
