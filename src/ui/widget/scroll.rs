//! Scrolling: [`ScrollArea`] moves taller content through a rect, and
//! [`ListView`] draws only the rows of a long list that are in sight.
//!
//! The wheel over an area scrolls it and is taken, so an area inside
//! another scrolls first. A thin bar shows while the content moves or the
//! pointer is near, and drags. What scrolls out is clipped both on screen
//! and for the pointer.

use std::ops::Range;

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::Component;
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;

/// Parts of an area: the bar's thumb, and where its fade is kept.
const THUMB_PART: u16 = 0xE000;
const SHOWN: Channel = Channel::Custom(0xE001);
const OFFSET: Channel = Channel::Custom(0xE002);
/// Seconds the bar stays after the content stops moving.
const LINGER: f32 = 0.9;

/// How an area's bar looks and how far the wheel moves it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollStyle {
    pub thumb: Color,
    pub thumb_hover: Color,
    pub width: f32,
    /// Pixels a wheel notch scrolls.
    pub step: f32,
    /// Scrolling eases instead of jumping.
    pub smooth: bool,
}

impl ScrollStyle {
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            thumb: p.faint.fade(0.55),
            thumb_hover: p.dim.fade(0.85),
            width: 6.0,
            step: if cfg!(target_os = "macos") { 1.0 } else { 48.0 },
            smooth: true,
        }
    }
}

/// Content `content_height` tall, shown through a rect from `offset` down.
pub struct ScrollArea<'v> {
    offset: &'v mut f32,
    content: f32,
    style: Option<ScrollStyle>,
}

impl<'v> ScrollArea<'v> {
    pub fn new(offset: &'v mut f32, content_height: f32) -> Self {
        Self {
            offset,
            content: content_height.max(0.0),
            style: None,
        }
    }

    #[must_use]
    pub fn style(mut self, style: ScrollStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Scrolls the area, then draws `body` clipped to it. `body` gets where
    /// the content's top left is now, its full height, and the part in
    /// sight, in canvas coordinates: draw at `content` and leave out what
    /// does not meet `view`.
    pub fn show<Id: WidgetId, R>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        body: impl FnOnce(&mut Ui<'_, Id>, Rect, Rect) -> AureaResult<R>,
    ) -> AureaResult<(Response, R)> {
        let style = self.style.unwrap_or_else(|| ScrollStyle::new(ui.theme));
        let room = (self.content - rect.height).max(0.0);
        let input = ui.interact(id, 0, rect, Sense::Hover, true);
        let over = ui.pointer().is_some_and(|at| rect.contains(at));
        let before = *self.offset;
        if over && room > 0.0 {
            let (_, dy) = ui.take_wheel();
            *self.offset -= dy * style.step;
        }
        let dragged = thumb_drag(ui, id, rect, room, self.content, self.offset);
        *self.offset = self.offset.clamp(0.0, room);
        let track = rect.split_right(14.0, 0.0).0;
        let moved = (*self.offset - before).abs() > f32::EPSILON;
        let shown = if style.smooth && !dragged {
            ui.animate(id, 0, OFFSET, *self.offset, ui.theme.motion.slide * 1.4)
        } else {
            ui.set_animated(id, 0, OFFSET, *self.offset);
            *self.offset
        };
        let content = Rect::new(rect.x, rect.y - shown, rect.width, self.content);
        let out = ui.clipped(rect, |ui| body(ui, content, rect))?;
        // After the body, so the bar is over what scrolls under it.
        if room > 0.0 {
            ui.interact(id, THUMB_PART, track, Sense::Hover, true);
        }
        bar(
            ui,
            id,
            rect,
            room,
            shown,
            self.content,
            (moved || dragged, over),
            style,
        )?;
        let mut response = Response::new(rect, input);
        response.changed = moved;
        response.held = dragged;
        Ok((response, out))
    }
}

/// Dragging the thumb, or pressing the track to jump there. The track's
/// hit is put down after the body; whether it is held is known already.
fn thumb_drag<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    rect: Rect,
    room: f32,
    content: f32,
    offset: &mut f32,
) -> bool {
    if room <= 0.0 || ui.state.active != Some(Key::new(id, THUMB_PART)) {
        return false;
    }
    let Some(at) = ui.pointer() else {
        return false;
    };
    let thumb = (rect.height * rect.height / content).max(24.0);
    let t = ((at.y - rect.y - thumb / 2.0) / (rect.height - thumb).max(1.0)).clamp(0.0, 1.0);
    *offset = t * room;
    true
}

#[allow(clippy::too_many_arguments)]
fn bar<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    rect: Rect,
    room: f32,
    shown: f32,
    content: f32,
    (moving, over): (bool, bool),
    style: ScrollStyle,
) -> AureaResult<()> {
    if room <= 0.0 {
        return Ok(());
    }
    // Lit while moving, near the pointer, or for a while after.
    let since = ui.animate(id, 0, Channel::Custom(0xE003), 0.0, 1.0 / LINGER);
    if moving {
        ui.set_animated(id, 0, Channel::Custom(0xE003), 1.0);
    }
    let near = over && ui.pointer().is_some_and(|at| at.x > rect.right() - 24.0);
    let target = if moving || near || since > 0.05 {
        1.0
    } else {
        0.0
    };
    let lit = ui.animate(id, 0, SHOWN, target, ui.theme.motion.hover);
    if lit <= 0.01 {
        return Ok(());
    }
    let h = (rect.height * rect.height / content)
        .max(24.0)
        .min(rect.height);
    let y = rect.y + (rect.height - h) * (shown / room).clamp(0.0, 1.0);
    let w = style.width * if near { 1.4 } else { 1.0 };
    let thumb = Rect::new(rect.right() - w - 3.0, y + 2.0, w, h - 4.0);
    let color = if near { style.thumb_hover } else { style.thumb };
    ui.round(thumb, w / 2.0, color.fade(lit))
}

/// Brings `item`, a span of the content from `top` to `bottom`, into a view
/// `height` tall, moving `offset` as little as it can.
pub fn scroll_into_view(offset: &mut f32, top: f32, bottom: f32, height: f32) {
    if top < *offset {
        *offset = top;
    } else if bottom > *offset + height {
        *offset = bottom - height;
    }
}

/// One row of a [`ListView`], as it is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListRow {
    pub index: usize,
    pub rect: Rect,
    /// It is the list's pick.
    pub picked: bool,
    pub hovered: bool,
}

/// A long list of rows the same height, only those in sight drawn. With
/// [`ListView::selected`] it picks one: a click, or Up and Down while it
/// has the keyboard, which keep the pick in sight.
pub struct ListView<'v> {
    offset: &'v mut f32,
    count: usize,
    row: f32,
    selected: Option<&'v mut Option<usize>>,
    style: Option<ScrollStyle>,
}

impl<'v> ListView<'v> {
    pub fn new(offset: &'v mut f32, count: usize, row_height: f32) -> Self {
        Self {
            offset,
            count,
            row: row_height.max(1.0),
            selected: None,
            style: None,
        }
    }

    #[must_use]
    pub fn selected(mut self, selected: &'v mut Option<usize>) -> Self {
        self.selected = Some(selected);
        self
    }

    #[must_use]
    pub fn style(mut self, style: ScrollStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Draws each row in sight with `row`.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        mut row: impl FnMut(&mut Ui<'_, Id>, ListRow) -> AureaResult<()>,
    ) -> AureaResult<Response> {
        let Self {
            offset,
            count,
            row: height,
            mut selected,
            style,
        } = self;
        // The list itself takes the keyboard, under its rows.
        let input = ui.interact(id, 1, rect, Sense::Adjust, selected.is_some());
        let mut changed = false;
        if input.focused
            && let Some(picked) = selected.as_deref_mut()
        {
            changed |= list_keys(ui, picked, count, rect.height / height);
            if changed && let Some(at) = *picked {
                #[allow(clippy::cast_precision_loss)]
                let top = at as f32 * height;
                scroll_into_view(offset, top, top + height, rect.height);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let content = count as f32 * height;
        let mut area = ScrollArea::new(offset, content);
        if let Some(style) = style {
            area = area.style(style);
        }
        let picked_now = selected.as_deref().copied().flatten();
        let ring = ui.eased(id, 1, input).focus;
        let (mut response, clicked) = area.show(ui, id, rect, |ui, content, view| {
            let mut clicked = None;
            for i in visible_rows(content, view, height, count) {
                #[allow(clippy::cast_precision_loss)]
                let at = Rect::new(
                    content.x,
                    content.y + i as f32 * height,
                    content.width,
                    height,
                );
                let part = 2 + u16::try_from(i % 0xD000).unwrap_or(0);
                let hit = ui.interact(id, part, at, Sense::Hover, true);
                if ui.press(id, part).is_some() {
                    clicked = Some(i);
                }
                let state = ListRow {
                    index: i,
                    rect: at,
                    picked: picked_now == Some(i),
                    hovered: hit.hovered,
                };
                row(ui, state)?;
            }
            Ok(clicked)
        })?;
        FocusRing::new(ui.theme, ui.theme.radius, ring).paint(ui, rect)?;
        if let (Some(i), Some(picked)) = (clicked, selected) {
            changed |= *picked != Some(i);
            *picked = Some(i);
            ui.state.give_focus(Some(Key::new(id, 1)));
        }
        if changed {
            ui.mark_changed();
        }
        response.changed |= changed;
        response.focused = input.focused;
        Ok(response)
    }
}

/// Up, Down, Page Up, Page Down, Home and End move the pick.
fn list_keys<Id: WidgetId>(
    ui: &Ui<'_, Id>,
    picked: &mut Option<usize>,
    count: usize,
    page: f32,
) -> bool {
    if count == 0 {
        return false;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let page = (page.floor() as usize).max(1);
    let before = *picked;
    for (key, _) in ui.keys() {
        let at = picked.unwrap_or(0);
        *picked = Some(match key {
            KeyCode::Down if picked.is_none() => 0,
            KeyCode::Down => (at + 1).min(count - 1),
            KeyCode::Up => at.saturating_sub(1),
            KeyCode::PageDown => (at + page).min(count - 1),
            KeyCode::PageUp => at.saturating_sub(page),
            KeyCode::Home => 0,
            KeyCode::End => count - 1,
            _ => continue,
        });
    }
    *picked != before
}

/// The rows of a list `height` apart, its top at `content`, that meet
/// `view`.
pub fn visible_rows(content: Rect, view: Rect, height: f32, count: usize) -> Range<usize> {
    if count == 0 || height <= 0.0 {
        return 0..0;
    }
    let first = ((view.y - content.y) / height).floor().max(0.0);
    let last = ((view.bottom() - content.y) / height).ceil().max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (first, last) = (first as usize, last as usize);
    first.min(count)..last.min(count)
}

impl<Id: WidgetId> Widget<Id> for ScrollArea<'_> {
    /// An area with nothing drawn in it: for content drawn some other way,
    /// only the scrolling.
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        ScrollArea::show(self, ui, id, rect, |_, _, _| Ok(())).map(|(r, ())| r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_rows_in_sight_are_visited() {
        let content = Rect::new(0.0, -95.0, 100.0, 2000.0);
        let view = Rect::new(0.0, 0.0, 100.0, 50.0);
        assert_eq!(visible_rows(content, view, 20.0, 100), 4..8);
        assert_eq!(visible_rows(content, view, 20.0, 5), 4..5);
        assert_eq!(visible_rows(content, view, 20.0, 0), 0..0);
    }

    #[test]
    fn bringing_into_view_moves_the_least() {
        let mut offset = 100.0;
        scroll_into_view(&mut offset, 120.0, 140.0, 50.0);
        assert_eq!(offset, 100.0, "already in sight");
        scroll_into_view(&mut offset, 160.0, 180.0, 50.0);
        assert_eq!(offset, 130.0, "bottom edge lines up");
        scroll_into_view(&mut offset, 40.0, 60.0, 50.0);
        assert_eq!(offset, 40.0, "top edge lines up");
    }

    use crate::Rect;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::{Button, ListView, ScrollArea, ScrollStyle, Theme, UiState};
    use crate::window::{KeyCode, Modifiers, MouseButton, MouseButtons, WindowEvent};

    const LIST: Rect = Rect::new(0.0, 0.0, 200.0, 100.0);
    const AREA: Rect = Rect::new(220.0, 0.0, 100.0, 100.0);
    const ROW: f32 = 20.0;

    struct Rig {
        raster: CpuRasterizer,
        ui: UiState<u8>,
        theme: Theme,
        list: f32,
        area: f32,
        picked: Option<usize>,
        pressed: u32,
    }

    impl Rig {
        fn new() -> Self {
            let mut ui = UiState::new();
            ui.set_reduced_motion(true);
            let mut rig = Self {
                raster: CpuRasterizer::new(340, 200),
                ui,
                theme: Theme::dark(),
                list: 0.0,
                area: 0.0,
                picked: None,
                pressed: 0,
            };
            rig.frame();
            rig
        }

        fn frame(&mut self) {
            let mut ctx = self.raster.begin_frame().expect("frame");
            {
                let mut ui = self.ui.frame(&mut *ctx, &self.theme);
                let style = ScrollStyle {
                    smooth: false,
                    step: ROW,
                    ..ScrollStyle::new(&self.theme)
                };
                ListView::new(&mut self.list, 1000, ROW)
                    .selected(&mut self.picked)
                    .style(style)
                    .show(&mut ui, 1, LIST, |ui, row| {
                        ui.fill(row.rect, ui.theme.palette.surface)
                    })
                    .expect("list");
                let pressed = &mut self.pressed;
                ScrollArea::new(&mut self.area, 300.0)
                    .style(style)
                    .show(&mut ui, 2, AREA, |ui, content, _| {
                        let at = Rect::new(content.x, content.y, 80.0, 30.0);
                        if Button::new("Top").show(ui, 3, at)?.clicked {
                            *pressed += 1;
                        }
                        Ok(())
                    })
                    .expect("area");
            }
            drop(ctx);
            self.raster.end_frame().expect("end");
        }

        fn at(&mut self, x: f32, y: f32) {
            self.ui.handle(&WindowEvent::MouseMove {
                x: f64::from(x),
                y: f64::from(y),
                buttons: MouseButtons::default(),
                modifiers: Modifiers::default(),
            });
        }

        fn button(&mut self, x: f32, y: f32, pressed: bool) {
            self.at(x, y);
            self.ui.handle(&WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                modifiers: Modifiers::default(),
                x: f64::from(x),
                y: f64::from(y),
                click_count: 1,
            });
            self.frame();
        }

        fn wheel(&mut self, x: f32, y: f32, notches: f64) {
            self.at(x, y);
            self.ui.handle(&WindowEvent::MouseWheel {
                delta_x: 0.0,
                delta_y: notches,
                modifiers: Modifiers::default(),
            });
            self.frame();
        }

        fn key(&mut self, key: KeyCode) {
            self.ui.handle(&WindowEvent::KeyInput {
                key,
                pressed: true,
                modifiers: Modifiers::default(),
            });
            self.frame();
        }
    }

    #[test]
    fn the_wheel_scrolls_what_is_under_the_pointer() {
        let mut rig = Rig::new();
        rig.wheel(50.0, 50.0, -3.0);
        assert_eq!(rig.list, 60.0);
        assert_eq!(rig.area, 0.0, "the other area stays");
        rig.wheel(50.0, 50.0, 10.0);
        assert_eq!(rig.list, 0.0, "not past the top");
        rig.wheel(250.0, 50.0, -100.0);
        assert_eq!(rig.area, 200.0, "not past the bottom");
    }

    #[test]
    fn a_click_picks_the_row_under_it_as_scrolled() {
        let mut rig = Rig::new();
        rig.wheel(50.0, 50.0, -5.0);
        rig.button(50.0, 30.0, true);
        rig.button(50.0, 30.0, false);
        assert_eq!(rig.picked, Some(6), "100 px scrolled, then the second row");
    }

    #[test]
    fn what_scrolled_out_takes_no_clicks() {
        let mut rig = Rig::new();
        rig.wheel(250.0, 50.0, -1.0);
        rig.button(240.0, 5.0, true);
        rig.button(240.0, 5.0, false);
        assert_eq!(rig.pressed, 1, "still partly in sight");
        rig.wheel(250.0, 50.0, -2.0);
        rig.button(240.0, 5.0, true);
        rig.button(240.0, 5.0, false);
        assert_eq!(rig.pressed, 1, "gone above the area");
    }

    #[test]
    fn keys_move_the_pick_and_keep_it_in_sight() {
        let mut rig = Rig::new();
        rig.button(50.0, 10.0, true);
        rig.button(50.0, 10.0, false);
        assert_eq!(rig.picked, Some(0));
        for _ in 0..7 {
            rig.key(KeyCode::Down);
        }
        assert_eq!(rig.picked, Some(7));
        assert_eq!(rig.list, 60.0, "row 7 lines up with the bottom");
        rig.key(KeyCode::End);
        assert_eq!(rig.picked, Some(999));
        assert_eq!(rig.list, 19_900.0);
    }

    #[test]
    fn the_bar_drags() {
        let mut rig = Rig::new();
        rig.button(196.0, 2.0, true);
        rig.button(196.0, 2.0, true);
        rig.at(196.0, 100.0);
        rig.frame();
        rig.button(196.0, 100.0, false);
        assert_eq!(rig.list, 19_900.0, "dragged to the end");
    }
}
