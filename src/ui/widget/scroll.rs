//! Scrolling: [`ScrollArea`] moves taller content through a rect, and [`ListView`] draws only the
//! rows of a long list that are in sight.

use std::ops::Range;

use super::Widget;
use crate::render::{Color, Rect};
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::Component;
use crate::ui::state::{Channel, Key, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::KeyCode;
use crate::{AureaResult, WheelScroll, wheel_scroll};

const THUMB_PART: u16 = 0xE000;
const SHOWN: Channel = Channel::Custom(0xE001);
const OFFSET: Channel = Channel::Custom(0xE002);
const LINGER: f32 = 0.9;

/// How an area's bar looks and how far the wheel moves it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollStyle {
    /// Bar colour.
    pub thumb: Color,
    /// Bar colour under the pointer.
    pub thumb_hover: Color,
    /// Bar width.
    pub width: f32,
    /// Pixels a wheel notch scrolls; infinite scrolls a whole view, 0 not at all.
    pub step: f32,
    /// Scrolling eases instead of jumping.
    pub smooth: bool,
}

impl ScrollStyle {
    /// Default style from the theme.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            thumb: p.faint.fade(0.55),
            thumb_hover: p.dim.fade(0.85),
            width: 6.0,
            step: wheel_step(),
            smooth: true,
        }
    }
}

/// The default notch from the user's wheel setting: 16 px a line, or a whole view. macOS wheel
/// events already arrive in points.
fn wheel_step() -> f32 {
    if cfg!(target_os = "macos") {
        return 1.0;
    }
    match wheel_scroll() {
        WheelScroll::Lines(_) => wheel_line_pixels(),
        WheelScroll::Page => f32::INFINITY,
    }
}

/// Logical pixels a notch moves under the user's line setting: 16 a line, 48 when it is a page.
pub(crate) fn wheel_line_pixels() -> f32 {
    match wheel_scroll() {
        WheelScroll::Lines(lines) => 16.0 * f32::from(u16::try_from(lines).unwrap_or(u16::MAX)),
        WheelScroll::Page => 48.0,
    }
}

/// Content `content_height` tall, shown through a rect from `offset` down.
pub struct ScrollArea<'v> {
    offset: &'v mut f32,
    content: f32,
    style: Option<ScrollStyle>,
    jump: bool,
}

impl<'v> ScrollArea<'v> {
    /// Creates an area showing `offset` of content `content_height` tall.
    pub fn new(offset: &'v mut f32, content_height: f32) -> Self {
        Self {
            offset,
            content: content_height.max(0.0),
            style: None,
            jump: false,
        }
    }

    /// Sets the look; defaults to the theme's.
    #[must_use]
    pub fn style(mut self, style: ScrollStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// The offset lands this frame with no easing, for a go-to-line.
    #[must_use]
    pub fn jump(mut self) -> Self {
        self.jump = true;
        self
    }

    /// Moves the offset by `by` and its eased value with it, so what is on screen stays put, for
    /// content that scrolls off the top. Call before [`Self::show`] with the same `id`.
    pub fn shift<Id: WidgetId>(ui: &mut Ui<'_, Id>, id: Id, offset: &mut f32, by: f32) {
        *offset += by;
        if let Some(now) = ui.state.tweens.get((Key::new(id, 0), OFFSET)) {
            ui.set_animated(id, 0, OFFSET, now + by);
        }
    }

    /// Creates an area whose content height will be supplied by [`Self::show_measured`].
    pub fn measured(offset: &'v mut f32) -> Self {
        Self::new(offset, 0.0)
    }

    /// Measures at the available width before drawing.
    ///
    /// ```rust,no_run
    /// # use aurea::ui::{ScrollArea, Ui};
    /// # use aurea::render::{Font, LayoutOptions, Paint, Point, Rect, layout_text};
    /// # fn paragraph(ui: &mut Ui<'_, u8>, offset: &mut f32, text: &str) -> aurea::AureaResult<()> {
    /// let font = Font::new("sans-serif", 14.0);
    /// let color = ui.theme.palette.text;
    /// ScrollArea::measured(offset).show_measured(
    ///     ui, 1, Rect::new(0.0, 0.0, 200.0, 100.0),
    ///     |_, width| {
    ///         let layout = layout_text(text, &font, LayoutOptions {
    ///             max_width: Some(width), ..LayoutOptions::default()
    ///         })?;
    ///         Ok((layout.height, layout))
    ///     },
    ///     |ui, content, _, layout| ui.ctx.draw_text_layout(
    ///         &layout, Point::new(content.x, content.y), &font, &Paint { color, ..Paint::new() },
    ///     ),
    /// )?;
    /// # Ok(()) }
    /// ```
    pub fn show_measured<Id: WidgetId, L, R>(
        mut self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        measure: impl FnOnce(&mut Ui<'_, Id>, f32) -> AureaResult<(f32, L)>,
        body: impl FnOnce(&mut Ui<'_, Id>, Rect, Rect, L) -> AureaResult<R>,
    ) -> AureaResult<(Response, R)> {
        let (height, layout) = measure(ui, rect.width)?;
        self.content = height.max(0.0);
        self.show(ui, id, rect, |ui, content, view| {
            body(ui, content, view, layout)
        })
    }

    /// Draws `body` clipped to the area, then routes unused wheel movement.
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
        let dragged = thumb_drag(ui, id, rect, room, self.content, self.offset);
        *self.offset = self.offset.clamp(0.0, room);
        let track = rect.split_right(14.0, 0.0).0;
        let mut moved = (*self.offset - before).abs() > f32::EPSILON;
        let shown = if style.smooth && !dragged && !self.jump {
            // Farther than two views behind, the eased value first closes to one view short.
            if let Some(now) = ui.state.tweens.get((Key::new(id, 0), OFFSET)) {
                let gap = *self.offset - now;
                if gap.abs() > 2.0 * rect.height {
                    ui.set_animated(id, 0, OFFSET, *self.offset - gap.signum() * rect.height);
                }
            }
            ui.animate(id, 0, OFFSET, *self.offset, ui.theme.motion.slide * 1.4)
        } else {
            ui.set_animated(id, 0, OFFSET, *self.offset);
            *self.offset
        };
        let content = Rect::new(rect.x, rect.y - shown, rect.width, self.content);
        let out = ui.clipped(rect, |ui| body(ui, content, rect))?;
        let step = if style.step == f32::INFINITY {
            rect.height
        } else {
            style.step
        };
        if over && room > 0.0 && step.is_finite() && step > 0.0 {
            let previous = *self.offset;
            *self.offset = (previous - ui.wheel().1 * step).clamp(0.0, room);
            let distance = *self.offset - previous;
            ui.consume_wheel((0.0, -distance / step));
            if distance.abs() > f32::EPSILON {
                moved = true;
                ui.mark_changed();
            }
        }
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

/// Brings `item`, a span of the content from `top` to `bottom`, into a view `height` tall, moving
/// `offset` as little as it can.
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
    /// Index of the item.
    pub index: usize,
    /// Where the row is drawn.
    pub rect: Rect,
    /// It is the list's pick.
    pub picked: bool,
    /// The pointer is over the row.
    pub hovered: bool,
}

/// A long list of rows the same height, only those in sight drawn.
pub struct ListView<'v> {
    offset: &'v mut f32,
    count: usize,
    row: f32,
    cell_width: Option<f32>,
    selected: Option<&'v mut Option<usize>>,
    style: Option<ScrollStyle>,
}

impl<'v> ListView<'v> {
    /// Creates a list of `count` rows, each `row_height` tall.
    pub fn new(offset: &'v mut f32, count: usize, row_height: f32) -> Self {
        Self {
            offset,
            count,
            row: row_height.max(1.0),
            cell_width: None,
            selected: None,
            style: None,
        }
    }

    /// Lays the items out in cells at least `cell_width` wide, as many to a row as fit, stretched
    /// to fill it.
    #[must_use]
    pub fn grid(mut self, cell_width: f32) -> Self {
        self.cell_width = Some(cell_width.max(1.0));
        self
    }

    /// Sets the variable that holds the pick.
    #[must_use]
    pub fn selected(mut self, selected: &'v mut Option<usize>) -> Self {
        self.selected = Some(selected);
        self
    }

    /// Sets the look; defaults to the theme's.
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
            cell_width,
            mut selected,
            style,
        } = self;
        let columns = columns(cell_width, rect.width);
        let rows = count.div_ceil(columns);
        let input = ui.interact(id, 1, rect, Sense::Adjust, selected.is_some());
        let mut changed = false;
        let mut activated = false;
        if input.focused
            && let Some(picked) = selected.as_deref_mut()
        {
            let layout = Layout {
                count,
                columns,
                page: rect.height / height,
            };
            changed |= list_keys(ui, picked, layout);
            activated |= picked.is_some() && ui.keys().any(|(k, _)| k == KeyCode::Enter);
            if changed && let Some(at) = *picked {
                #[allow(clippy::cast_precision_loss)]
                let top = (at / columns) as f32 * height;
                scroll_into_view(offset, top, top + height, rect.height);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let content = rows as f32 * height;
        let mut area = ScrollArea::new(offset, content);
        if let Some(style) = style {
            area = area.style(style);
        }
        let picked_now = selected.as_deref().copied().flatten();
        let ring = ui.eased(id, 1, input).focus;
        // Rows get no hits of their own: a u16 part cannot tell every row apart. The scroll area's
        // part 0 covers the list and is hit over it, so presses and the pointer are read here and
        // matched to rows by position.
        let press = ui.press(id, 0);
        let pointer = ui.pointer();
        let (mut response, clicked) = area.show(ui, id, rect, |ui, content, view| {
            let mut clicked = None;
            #[allow(clippy::cast_precision_loss)]
            let width = content.width / columns as f32;
            for r in visible_rows(content, view, height, rows) {
                for c in 0..columns {
                    let i = r * columns + c;
                    if i >= count {
                        break;
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let at = Rect::new(
                        content.x + c as f32 * width,
                        content.y + r as f32 * height,
                        width,
                        height,
                    );
                    if let Some(press) = press
                        && at.contains(press.at)
                    {
                        clicked = Some((i, press.clicks));
                    }
                    let state = ListRow {
                        index: i,
                        rect: at,
                        picked: picked_now == Some(i),
                        hovered: pointer.is_some_and(|p| rect.contains(p) && at.contains(p)),
                    };
                    row(ui, state)?;
                }
            }
            Ok(clicked)
        })?;
        FocusRing::new(ui.theme, ui.theme.radius, ring).paint(ui, rect)?;
        let row_clicked = clicked.map(|(i, _)| i);
        if let Some((i, clicks)) = clicked
            && let Some(picked) = selected
        {
            changed |= *picked != Some(i);
            *picked = Some(i);
            activated |= clicks >= 2;
            ui.state.give_focus(Some(Key::new(id, 1)));
        }
        response.row_clicked = row_clicked;
        if changed {
            ui.mark_changed();
        }
        response.changed |= changed;
        response.activated = activated;
        response.focused = input.focused;
        Ok(response)
    }
}

#[derive(Clone, Copy)]
struct Layout {
    count: usize,
    columns: usize,
    page: f32,
}

fn columns(cell_width: Option<f32>, width: f32) -> usize {
    match cell_width {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(cell) => ((width / cell).floor() as usize).max(1),
        None => 1,
    }
}

fn list_keys<Id: WidgetId>(ui: &Ui<'_, Id>, picked: &mut Option<usize>, layout: Layout) -> bool {
    let Layout {
        count,
        columns,
        page,
    } = layout;
    if count == 0 {
        return false;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let page = (page.floor() as usize).max(1) * columns;
    let before = *picked;
    let last = count - 1;
    for (key, _) in ui.keys() {
        let at = picked.unwrap_or(0);
        *picked = Some(match key {
            KeyCode::Down | KeyCode::Right if picked.is_none() => 0,
            KeyCode::Down => (at + columns).min(last),
            KeyCode::Up => at.saturating_sub(columns),
            KeyCode::Right if columns > 1 => (at + 1).min(last),
            KeyCode::Left if columns > 1 => at.saturating_sub(1),
            KeyCode::PageDown => (at + page).min(last),
            KeyCode::PageUp => at.saturating_sub(page),
            KeyCode::Home => 0,
            KeyCode::End => last,
            _ => continue,
        });
    }
    *picked != before
}

/// The rows of a list `height` apart, its top at `content`, that meet `view`.
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

    struct Grid {
        raster: CpuRasterizer,
        ui: UiState<u8>,
        theme: Theme,
        offset: f32,
        picked: Option<usize>,
        activated: bool,
    }

    impl Grid {
        fn new() -> Self {
            let mut grid = Self {
                raster: CpuRasterizer::new(200, 100),
                ui: UiState::new(),
                theme: Theme::dark(),
                offset: 0.0,
                picked: None,
                activated: false,
            };
            grid.frame();
            grid
        }

        fn frame(&mut self) {
            let mut ctx = self.raster.begin_frame().expect("frame");
            {
                let mut ui = self.ui.frame(&mut *ctx, &self.theme);
                let r = ListView::new(&mut self.offset, 10, ROW)
                    .grid(50.0)
                    .selected(&mut self.picked)
                    .show(&mut ui, 1, Rect::new(0.0, 0.0, 200.0, 100.0), |_, _| Ok(()))
                    .expect("grid");
                self.activated = r.activated;
            }
            drop(ctx);
            self.raster.end_frame().expect("end");
        }

        fn click(&mut self, x: f64, y: f64, clicks: u8) {
            for pressed in [true, false] {
                self.ui.handle(&WindowEvent::MouseButton {
                    button: MouseButton::Left,
                    pressed,
                    modifiers: Modifiers::default(),
                    x,
                    y,
                    click_count: clicks,
                    time: Default::default(),
                });
                self.frame();
                if pressed && self.activated {
                    return;
                }
            }
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
    fn a_grid_picks_by_cell_and_walks_rows_and_columns() {
        let mut g = Grid::new();
        g.click(130.0, 30.0, 1);
        assert_eq!(g.picked, Some(6), "second row, third cell");
        assert!(!g.activated, "a single click only picks");
        g.key(KeyCode::Right);
        assert_eq!(g.picked, Some(7));
        g.key(KeyCode::Down);
        assert_eq!(g.picked, Some(9), "clamped to the last item");
        g.key(KeyCode::Up);
        assert_eq!(g.picked, Some(5));
    }

    #[test]
    fn a_double_click_or_enter_activates() {
        let mut g = Grid::new();
        g.click(30.0, 10.0, 2);
        assert!(g.activated, "double click");
        assert_eq!(g.picked, Some(0));
        g.frame();
        assert!(!g.activated, "once");
        g.key(KeyCode::Enter);
        assert!(g.activated, "Enter on the pick");
    }

    const LIST: Rect = Rect::new(0.0, 0.0, 200.0, 100.0);
    const AREA: Rect = Rect::new(220.0, 0.0, 100.0, 100.0);
    const ROW: f32 = 20.0;

    struct Rig {
        raster: CpuRasterizer,
        ui: UiState<u8>,
        theme: Theme,
        list: f32,
        area: f32,
        count: usize,
        picked: Option<usize>,
        pressed: u32,
        clicks: Vec<(usize, bool)>,
        step: f32,
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
                count: 1000,
                picked: None,
                pressed: 0,
                clicks: Vec::new(),
                step: ROW,
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
                    step: self.step,
                    ..ScrollStyle::new(&self.theme)
                };
                let list = ListView::new(&mut self.list, self.count, ROW)
                    .selected(&mut self.picked)
                    .style(style)
                    .show(&mut ui, 1, LIST, |ui, row| {
                        ui.fill(row.rect, ui.theme.palette.surface)
                    })
                    .expect("list");
                if let Some(row) = list.row_clicked {
                    self.clicks.push((row, list.changed));
                }
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
                time: Default::default(),
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
                time: Default::default(),
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
    fn a_page_setting_scrolls_a_view_a_notch() {
        let mut rig = Rig::new();
        rig.step = f32::INFINITY;
        rig.wheel(250.0, 50.0, -1.0);
        assert_eq!(rig.area, AREA.height);
        rig.step = 0.0;
        rig.wheel(250.0, 50.0, -1.0);
        assert_eq!(rig.area, AREA.height, "0 lines does not scroll");
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
    fn rows_far_apart_are_told_apart() {
        let mut rig = Rig::new();
        rig.count = 100_000;
        rig.list = 53_248.0 * ROW;
        rig.frame();
        rig.button(50.0, 10.0, true);
        rig.button(50.0, 10.0, false);
        assert_eq!(rig.picked, Some(53_248), "the top row, not row 0");
        rig.list = 0.0;
        rig.button(50.0, 10.0, true);
        rig.button(50.0, 10.0, false);
        assert_eq!(rig.picked, Some(0));
    }

    #[test]
    fn clicking_the_pick_reports_the_row_without_a_change() {
        let mut rig = Rig::new();
        rig.picked = Some(2);
        rig.button(50.0, 50.0, true);
        rig.button(50.0, 50.0, false);
        assert_eq!(rig.clicks, vec![(2, false)]);
        assert_eq!(rig.picked, Some(2));
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

    const CONTENT: f32 = 10_000.0;

    struct Eased {
        raster: CpuRasterizer,
        ui: UiState<u8>,
        theme: Theme,
        offset: f32,
    }

    impl Eased {
        fn new() -> Self {
            let mut eased = Self {
                raster: CpuRasterizer::new(120, 120),
                ui: UiState::new(),
                theme: Theme::dark(),
                offset: 0.0,
            };
            eased.draw(false, |_, _| {});
            eased
        }

        /// Draws one frame after `edit`, with the area jumping if asked, and returns the offset shown.
        fn draw(&mut self, jump: bool, edit: impl FnOnce(&mut Ui<'_, u8>, &mut f32)) -> f32 {
            let mut ctx = self.raster.begin_frame().expect("frame");
            let mut shown = 0.0;
            {
                let mut ui = self.ui.frame(&mut *ctx, &self.theme);
                edit(&mut ui, &mut self.offset);
                let mut area = ScrollArea::new(&mut self.offset, CONTENT);
                if jump {
                    area = area.jump();
                }
                area.show(&mut ui, 1, VIEW, |_, content, _| {
                    shown = VIEW.y - content.y;
                    Ok(())
                })
                .expect("area");
            }
            drop(ctx);
            self.raster.end_frame().expect("end");
            shown
        }
    }

    const VIEW: Rect = Rect::new(0.0, 0.0, 100.0, 100.0);

    #[test]
    fn a_jump_lands_at_once() {
        let mut rig = Eased::new();
        let eased = rig.draw(false, |_, o| *o = 60.0);
        assert!(eased > 0.0 && eased < 60.0, "an ease starts: {eased}");
        let mut rig = Eased::new();
        assert_eq!(rig.draw(true, |_, o| *o = 60.0), 60.0);
    }

    #[test]
    fn a_shift_keeps_the_eased_and_target_values_equal() {
        let mut rig = Eased::new();
        rig.draw(true, |_, o| *o = 300.0);
        let shown = rig.draw(false, |ui, o| ScrollArea::shift(ui, 1, o, -120.0));
        assert_eq!(rig.offset, 180.0);
        assert_eq!(shown, 180.0, "no ease from the old place");
    }

    #[test]
    fn a_long_change_snaps_to_within_one_view() {
        let mut rig = Eased::new();
        let shown = rig.draw(false, |_, o| *o = 5000.0);
        assert!(
            (5000.0 - shown).abs() <= VIEW.height,
            "within a view of the target: {shown}"
        );
        assert!(shown < 5000.0, "the rest still eases");
    }
}
