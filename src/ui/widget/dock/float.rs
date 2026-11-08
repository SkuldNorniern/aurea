//! Panels floating above a dock, moved by their title bar and put back by dropping that on a
//! group's tab strip. An app wanting one in a window or a sheet of its own reads
//! [`Dock::floating`] and takes it out with [`Dock::remove`].

use super::{DockStyle, DockTabEvent};
use crate::AureaResult;
use crate::render::{Point, Rect};
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::{Component, Label, Surface};
use crate::ui::state::{Sense, WidgetId};
use crate::window::CursorIcon;

use super::Dock;
use super::tree::DockSide;

const FLOAT_PART: u16 = 0xC000;
/// A title bar keeps this much of itself inside the dock, to be caught again.
/// The parts of a floating panel: its body, title bar and close button.
const PARTS: u16 = 3;

/// A panel out of the tree, floating at `rect`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatingPanel<P> {
    /// The panel.
    pub panel: P,
    /// Where it floats.
    pub rect: Rect,
    /// Stays with the panel while the order changes, so a press survives raising it.
    pub(super) slot: u16,
}

/// A floating panel being moved, and where in its rect the pointer took hold.
pub(super) type Moving<P> = Option<(P, Point)>;

impl<P: Copy + PartialEq> Dock<P> {
    /// Takes `panel` out of the tree to float at `rect`. False if it isn't docked.
    pub fn float(&mut self, panel: P, rect: Rect) -> bool {
        if !self.tree.remove(panel) {
            return false;
        }
        self.add_floating(panel, rect);
        true
    }

    pub(super) fn add_floating(&mut self, panel: P, rect: Rect) {
        let slot = self.next_slot;
        self.next_slot = (slot + 1) % ((0xFFFF - FLOAT_PART) / PARTS);
        self.floating.push(FloatingPanel { panel, rect, slot });
    }

    /// The panels floating, the top one last.
    pub fn floating(&self) -> &[FloatingPanel<P>] {
        &self.floating
    }

    /// Puts a floating `panel` back on `side` of `beside`. False if it isn't floating or
    /// `beside` isn't docked.
    pub fn dock_floating(&mut self, panel: P, beside: P, side: DockSide) -> bool {
        let Some(i) = self.floating.iter().position(|f| f.panel == panel) else {
            return false;
        };
        if !self.tree.insert(panel, beside, side) {
            return false;
        }
        self.floating.remove(i);
        self.tree.activate(panel);
        true
    }

    /// Takes `panel` away, docked or floating. False if it is neither.
    pub fn remove(&mut self, panel: P) -> bool {
        if let Some(i) = self.floating.iter().position(|f| f.panel == panel) {
            self.floating.remove(i);
            return true;
        }
        self.tree.remove(panel)
    }
}

/// Draws the floating panels over the dock in `area`.
#[allow(clippy::too_many_arguments)]
pub(super) fn show<'t, Id: WidgetId, P: Copy + PartialEq>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    dock: &mut Dock<P>,
    area: Rect,
    title: &impl Fn(P) -> &'t str,
    style: &DockStyle,
    close_buttons: bool,
    content: &mut impl FnMut(&mut Ui<'_, Id>, P, Rect) -> AureaResult<()>,
    response: &mut Response,
) -> AureaResult<()> {
    let strip = style.metrics.strip;
    let mut raise = None;
    let mut held = false;
    for i in 0..dock.floating.len() {
        let FloatingPanel { panel, rect, slot } = dock.floating[i];
        let part = FLOAT_PART + slot * PARTS;
        ui.interact(id, part, rect, Sense::Hover, true);
        let bar = Rect::new(rect.x, rect.y, rect.width, strip.min(rect.height));
        let head = ui.interact(id, part + 1, bar, Sense::Click, true);
        let shut = Rect::new(bar.right() - strip, bar.y, strip, bar.height);
        let close = close_buttons.then(|| ui.interact(id, part + 2, shut, Sense::Click, true));
        response.hovered |= head.hovered;
        if let Some(press) = ui.press(id, part + 1) {
            dock.moving = Some((panel, Point::new(press.at.x - rect.x, press.at.y - rect.y)));
            raise = Some(i);
        }
        held |= head.held && dock.moving.is_some_and(|(moving, _)| moving == panel);

        Surface::new(style.active_tab)
            .radius(style.radius)
            .border(1.0, style.strip)
            .paint(ui, rect)?;
        ui.fill(bar, style.strip)?;
        Label::new(title(panel), style.active_text)
            .size(style.text_size)
            .paint(ui, bar.inset_xy(style.tab_padding, 0.0))?;
        if let Some(close) = close {
            let word = Label::new("×", style.text).size(style.text_size).centered();
            word.paint(ui, shut)?;
            if close.clicked {
                dock.tab_events.push(DockTabEvent::CloseRequested(panel));
            }
        }
        let body = Rect::new(
            rect.x,
            bar.bottom(),
            rect.width,
            (rect.height - bar.height).max(0.0),
        );
        ui.ctx.save()?;
        ui.ctx.clip_rect(body)?;
        let drawn = content(ui, panel, body);
        ui.ctx.restore()?;
        drawn?;
    }
    if let Some(i) = raise {
        let top = dock.floating.remove(i);
        dock.floating.push(top);
    }
    follow_move(ui, dock, area, held, style, response)
}

/// The group whose tab strip holds `at`, as a panel to dock beside.
fn strip_under<P: Copy>(dock: &Dock<P>, at: Point) -> Option<(P, Rect)> {
    dock.layout
        .groups
        .iter()
        .find(|group| group.strip.contains(at))
        .and_then(|group| {
            let panel = dock.layout.panels.get(group.tabs.start)?;
            Some((*panel, group.strip))
        })
}

fn follow_move<Id: WidgetId, P: Copy + PartialEq>(
    ui: &mut Ui<'_, Id>,
    dock: &mut Dock<P>,
    area: Rect,
    held: bool,
    style: &DockStyle,
    response: &mut Response,
) -> AureaResult<()> {
    let Some((panel, grab)) = dock.moving else {
        return Ok(());
    };
    let Some(at) = ui.pointer() else {
        return Ok(());
    };
    let under = strip_under(dock, at);
    if !held {
        dock.moving = None;
        if let Some((beside, _)) = under
            && dock.dock_floating(panel, beside, DockSide::Center)
        {
            response.changed = true;
            ui.mark_changed();
        }
        return Ok(());
    }
    response.held = true;
    ui.set_cursor(CursorIcon::Move);
    if let Some(float) = dock.floating.iter_mut().find(|f| f.panel == panel) {
        let keep = dock.feel.keep_in;
        let x = (at.x - grab.x).clamp(area.x + keep - float.rect.width, area.right() - keep);
        let y = (at.y - grab.y).clamp(area.y, (area.bottom() - keep).max(area.y));
        if (x, y) != (float.rect.x, float.rect.y) {
            float.rect.x = x;
            float.rect.y = y;
            response.changed = true;
            ui.mark_changed();
        }
    }
    if let Some((_, strip)) = under {
        Surface::new(style.preview)
            .radius(style.radius)
            .border(1.0, style.accent)
            .paint(ui, strip)?;
    }
    Ok(())
}
