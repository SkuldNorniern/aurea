//! Docked panels: groups of tabs split across a rect, bars between them that drag to resize, and
//! tabs that drag into another group or to one of its sides.

mod plan;
mod tree;

pub use plan::DockPlan;
pub use tree::{DockAxis, DockBar, DockGroup, DockLayout, DockMetrics, DockSide, DockTree};

use super::Widget;
use crate::AureaResult;
use crate::render::{Color, Point, Rect};
use crate::ui::component::{FocusRing, StateColors};
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{
    Component, IconMark, IconSource, Label, Mark, MarkShape, Surface, TextSize,
};
use crate::ui::state::{Channel, Interaction, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::CursorIcon;
use std::mem;

const BAR_PART: u16 = 0x4000;
const CLOSE_PART: u16 = 0x8000;
const DRAG_FROM: f32 = 6.0;
const DIRTY_DOT: f32 = 6.0;
const ICON_GAP: f32 = 4.0;

/// Which tabs show their close button, when the dock has them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloseButtons {
    /// On every tab.
    #[default]
    Always,
    /// On the shown tab and the tab under the pointer.
    ActiveOrHovered,
}

/// How a dock looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockStyle {
    /// Behind the panels.
    pub background: Color,
    /// Tab strip background.
    pub strip: Color,
    /// Tab fill by state.
    pub tab: StateColors,
    /// Fill of the shown tab.
    pub active_tab: Color,
    /// Inactive tab text.
    pub text: Color,
    /// Shown tab text.
    pub active_text: Color,
    /// The line under the shown tab, and the bar being dragged.
    pub accent: Color,
    /// Divider between groups.
    pub bar: Color,
    /// Where a dragged tab would land.
    pub preview: Color,
    /// Corner radius of tabs and panels.
    pub radius: f32,
    /// Space either side of a tab's title.
    pub tab_padding: f32,
    /// Widest a tab may be.
    pub max_tab: f32,
    /// Tab title size.
    pub text_size: TextSize,
    /// Strip height, gap and minimum part size.
    pub metrics: DockMetrics,
    /// Which tabs show close buttons, when the view has them.
    pub close_on: CloseButtons,
}

impl DockStyle {
    /// Default style from the theme.
    pub fn new(theme: &Theme) -> Self {
        let p = theme.palette;
        Self {
            background: p.background,
            strip: p.background,
            tab: StateColors::new(p.hover.with_alpha(0), p.hover, p.pressed),
            active_tab: p.surface,
            text: p.dim,
            active_text: p.text,
            accent: p.accent,
            bar: p.border,
            preview: p.accent.fade(0.22),
            radius: theme.radius,
            tab_padding: 12.0,
            max_tab: 180.0,
            text_size: TextSize::Small,
            metrics: DockMetrics {
                strip: 28.0,
                gap: 4.0,
                min_part: 64.0,
            },
            close_on: CloseButtons::Always,
        }
    }
}

/// A [`DockTree`] and what its view keeps between frames.
#[derive(Clone, Debug)]
pub struct Dock<P> {
    /// The panels and how they are split.
    pub tree: DockTree<P>,
    layout: DockLayout<P>,
    press: Option<(P, Point)>,
    drag: Option<P>,
    target: Option<(P, DockSide)>,
    tab_events: Vec<DockTabEvent<P>>,
}

/// A tab action for the app to handle after drawing the dock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DockTabEvent<P> {
    /// Context menu requested for a tab.
    ContextMenu {
        /// The tab's panel.
        panel: P,
        /// Where the menu should open.
        at: Point,
    },
    /// The panel stays in the tree until the app chooses to remove it.
    CloseRequested(P),
}

impl<P: Copy + PartialEq> Dock<P> {
    /// Creates a dock showing `tree`.
    pub fn new(tree: DockTree<P>) -> Self {
        Self {
            tree,
            layout: DockLayout::default(),
            press: None,
            drag: None,
            target: None,
            tab_events: Vec::new(),
        }
    }

    /// Where everything went in the last frame.
    pub fn layout(&self) -> &DockLayout<P> {
        &self.layout
    }

    /// The tab being dragged.
    pub fn dragging(&self) -> Option<P> {
        self.drag
    }

    /// Takes this frame's tab actions.
    ///
    /// ```
    /// use aurea::ui::{Dock, DockTabEvent, DockTree};
    /// let mut dock = Dock::new(DockTree::new(1u8));
    /// for event in dock.take_tab_events() {
    ///     match event {
    ///         DockTabEvent::ContextMenu { panel, at } => {
    ///             // Open the panel menu at `at`.
    ///         }
    ///         DockTabEvent::CloseRequested(panel) => {
    ///             // Save or confirm first if needed; removal is explicit.
    ///             dock.tree.remove(panel);
    ///         }
    ///     }
    /// }
    /// ```
    pub fn take_tab_events(&mut self) -> Vec<DockTabEvent<P>> {
        mem::take(&mut self.tab_events)
    }
}

enum Change<P> {
    Activate(P),
    Ratio(usize, f32),
}

/// Per-tab answers the app gives beyond the title; both default to none.
struct TabMarks<'d, P> {
    dirty: Option<Box<dyn Fn(P) -> bool + 'd>>,
    icon: Option<Box<dyn Fn(P) -> Option<IconSource<'d>> + 'd>>,
}

impl<P: Copy> TabMarks<'_, P> {
    fn dirty(&self, panel: P) -> bool {
        self.dirty.as_ref().is_some_and(|dirty| dirty(panel))
    }

    fn icon(&self, panel: P) -> Option<IconSource<'_>> {
        self.icon.as_ref().and_then(|icon| icon(panel))
    }
}

/// Shows a [`Dock`]: `title` names each panel, `content` draws the panel each group shows, clipped
/// to its rect.
pub struct DockView<'d, P, T, C> {
    dock: &'d mut Dock<P>,
    title: T,
    content: C,
    style: Option<DockStyle>,
    close_buttons: bool,
    marks: TabMarks<'d, P>,
}

impl<'d, P, T, C> DockView<'d, P, T, C> {
    /// Creates a view that draws `dock`.
    pub fn new(dock: &'d mut Dock<P>, title: T, content: C) -> Self {
        Self {
            dock,
            title,
            content,
            style: None,
            close_buttons: false,
            marks: TabMarks {
                dirty: None,
                icon: None,
            },
        }
    }

    /// Sets the look; defaults to the theme's.
    #[must_use]
    pub fn style(mut self, style: DockStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Shows keyboard-accessible close controls.
    #[must_use]
    pub fn close_buttons(mut self, show: bool) -> Self {
        self.close_buttons = show;
        self
    }

    /// Marks each tab `dirty` says has unsaved changes: it shows a dot where its close button is.
    #[must_use]
    pub fn dirty(mut self, dirty: impl Fn(P) -> bool + 'd) -> Self {
        self.marks.dirty = Some(Box::new(dirty));
        self
    }

    /// Draws the icon `icon` gives before each tab's title.
    #[must_use]
    pub fn icon(mut self, icon: impl Fn(P) -> Option<IconSource<'d>> + 'd) -> Self {
        self.marks.icon = Some(Box::new(icon));
        self
    }

    /// Draws the dock in `rect`.
    pub fn show<'t, Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response>
    where
        P: Copy + PartialEq,
        T: Fn(P) -> &'t str,
        C: FnMut(&mut Ui<'_, Id>, P, Rect) -> AureaResult<()>,
    {
        Widget::show(self, ui, id, rect)
    }
}

impl<'t, Id, P, T, C> Widget<Id> for DockView<'_, P, T, C>
where
    Id: WidgetId,
    P: Copy + PartialEq,
    T: Fn(P) -> &'t str,
    C: FnMut(&mut Ui<'_, Id>, P, Rect) -> AureaResult<()>,
{
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let Self {
            dock,
            title,
            mut content,
            style,
            close_buttons,
            marks,
        } = self;
        dock.tab_events.clear();
        let style = style.unwrap_or_else(|| DockStyle::new(ui.theme));
        dock.tree.layout(rect, style.metrics, &mut dock.layout);
        let mut response = Response {
            rect,
            ..Response::default()
        };
        ui.fill(rect, style.background)?;
        let mut change = None;
        let mut held = false;
        for g in 0..dock.layout.groups.len() {
            held |= tabs(
                ui,
                id,
                dock,
                g,
                &title,
                &marks,
                &style,
                close_buttons,
                &mut change,
                &mut response,
            )?;
        }
        for (panel, at) in dock.layout.visible() {
            ui.ctx.save()?;
            ui.ctx.clip_rect(at)?;
            let drawn = content(ui, panel, at);
            ui.ctx.restore()?;
            drawn?;
        }
        for b in 0..dock.layout.bars.len() {
            let bar = dock.layout.bars[b];
            bar_part(ui, id, bar, &style, &mut change)?;
        }
        follow_drag(ui, dock, held, &title, &marks, &style, &mut response)?;
        let changed = match change {
            Some(Change::Activate(panel)) => {
                !dock.tree.is_active(panel) && dock.tree.activate(panel)
            }
            Some(Change::Ratio(index, ratio)) => dock.tree.set_ratio(index, ratio),
            None => false,
        };
        if changed {
            response.changed = true;
            ui.mark_changed();
        }
        Ok(response)
    }
}

/// Room a tab's icon takes before its title, none without an icon.
fn icon_room(style: &DockStyle, icon: Option<IconSource<'_>>) -> f32 {
    if icon.is_some() {
        style.metrics.strip * 0.5 + ICON_GAP
    } else {
        0.0
    }
}

/// The square an icon fills at the start of a tab.
fn icon_rect(cell: Rect, style: &DockStyle) -> Rect {
    let size = style.metrics.strip * 0.5;
    Rect::new(
        cell.x + style.tab_padding / 2.0,
        cell.center().y - size / 2.0,
        size,
        size,
    )
}

#[allow(clippy::too_many_arguments)]
fn tabs<'t, Id: WidgetId, P: Copy + PartialEq>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    dock: &mut Dock<P>,
    g: usize,
    title: &impl Fn(P) -> &'t str,
    marks: &TabMarks<'_, P>,
    style: &DockStyle,
    close_buttons: bool,
    change: &mut Option<Change<P>>,
    response: &mut Response,
) -> AureaResult<bool> {
    let group = dock.layout.groups[g].clone();
    ui.fill(group.strip, style.strip)?;
    ui.fill(group.content, style.active_tab)?;
    let mut x = group.strip.x;
    let mut held = false;
    let mut target_x = group.strip.x;
    let mut target_width = 0.0;
    for (i, at) in group.tabs.clone().enumerate() {
        let panel = dock.layout.panels[at];
        let label = Label::new(title(panel), style.text).size(style.text_size);
        let dirty = marks.dirty(panel);
        let icon = marks.icon(panel);
        let close_width = if close_buttons || dirty {
            group.strip.height * 0.75
        } else {
            0.0
        };
        let width =
            (label.width(ui) + style.tab_padding * 2.0 + close_width + icon_room(style, icon))
                .min(style.max_tab);
        let room = (group.strip.right() - x).max(0.0);
        let cell = Rect::new(x, group.strip.y, width.min(room), group.strip.height);
        x += cell.width;
        let part = u16::try_from(at).unwrap_or(BAR_PART - 1).min(BAR_PART - 1);
        let input = ui.interact(id, part, cell, Sense::Click, true);
        let state = ui.eased(id, part, input);
        response.hovered |= input.hovered;
        response.focused |= input.focused;
        if input.secondary_clicked
            && let Some(at) = ui.secondary_at(id, part)
        {
            dock.tab_events
                .push(DockTabEvent::ContextMenu { panel, at });
            response.secondary_clicked = true;
        }
        press_tab(ui, id, part, dock, panel, input, change);
        held |= input.held && dock.drag == Some(panel);
        let shown = i == group.active;
        if shown {
            target_x = cell.x + 6.0;
            target_width = (cell.width - 12.0).max(0.0);
        }
        let active = ui.animate(
            id,
            part,
            Channel::Value,
            f32::from(u8::from(shown)),
            ui.theme.motion.slide,
        );
        paint_tab(ui, cell, label, active, state, style, close_width, icon)?;
        let hot = input.hovered || ui.pointer().is_some_and(|p| cell.contains(p));
        let close_shown = close_buttons && (style.close_on == CloseButtons::Always || shown || hot);
        if (close_shown || dirty) && cell.width > 0.0 {
            let close = Rect::new(
                cell.right() - close_width.min(cell.width),
                cell.y,
                close_width.min(cell.width),
                cell.height,
            );
            let close_part = CLOSE_PART + part;
            let close_input = ui.interact(id, close_part, close, Sense::Click, true);
            if close_input.clicked {
                dock.tab_events.push(DockTabEvent::CloseRequested(panel));
            }
            if close_input.secondary_clicked
                && let Some(at) = ui.secondary_at(id, close_part)
            {
                dock.tab_events
                    .push(DockTabEvent::ContextMenu { panel, at });
                response.secondary_clicked = true;
            }
            let eased = ui.eased(id, close_part, close_input);
            if close_input.hovered || close_input.held {
                ui.round(close.inset(2.0), style.radius, style.tab.at(eased))?;
            }
            FocusRing::new(ui.theme, style.radius, eased.focus).paint(ui, close.inset(2.0))?;
            if dirty && !hot {
                let centre = close.center();
                let dot = Rect::new(
                    centre.x - DIRTY_DOT / 2.0,
                    centre.y - DIRTY_DOT / 2.0,
                    DIRTY_DOT,
                    DIRTY_DOT,
                );
                ui.round(dot, DIRTY_DOT / 2.0, style.text)?;
            } else {
                Mark::new(MarkShape::Cross, style.text).paint(ui, close.inset(5.0))?;
            }
        }
    }
    let part = BAR_PART + u16::try_from(g).unwrap_or(BAR_PART - 1).min(BAR_PART - 1);
    let rate = ui.theme.motion.slide;
    let x = ui.animate(id, part, Channel::Custom(0xD001), target_x, rate);
    let width = ui.animate(id, part, Channel::Custom(0xD002), target_width, rate);
    ui.clipped(group.strip, |ui| {
        ui.round(
            Rect::new(x, group.strip.y, width.max(0.0), 2.0),
            1.0,
            style.accent,
        )
    })?;
    Ok(held)
}

fn press_tab<Id: WidgetId, P: Copy + PartialEq>(
    ui: &Ui<'_, Id>,
    id: Id,
    part: u16,
    dock: &mut Dock<P>,
    panel: P,
    input: Interaction,
    change: &mut Option<Change<P>>,
) {
    if let Some(press) = ui.press(id, part) {
        dock.press = Some((panel, press.at));
        *change = Some(Change::Activate(panel));
    }
    if input.clicked {
        *change = Some(Change::Activate(panel));
    }
    let Some((pressed, from)) = dock.press else {
        return;
    };
    if pressed != panel {
        return;
    }
    if !input.held {
        dock.press = None;
        return;
    }
    let far = ui
        .pointer()
        .is_some_and(|at| at.distance_to(from) > DRAG_FROM);
    if dock.drag.is_none() && far {
        dock.drag = Some(panel);
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_tab<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    cell: Rect,
    label: Label<'_>,
    active: f32,
    state: Eased,
    style: &DockStyle,
    close_width: f32,
    icon: Option<IconSource<'_>>,
) -> AureaResult<()> {
    let top = Rect::new(cell.x, cell.y, cell.width, cell.height + style.radius);
    let fill = style.tab.at(state).mix(style.active_tab, active);
    ui.ctx.save()?;
    ui.ctx.clip_rect(cell)?;
    Surface::new(fill).radius(style.radius).paint(ui, top)?;
    ui.ctx.restore()?;
    FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, cell.inset(2.0))?;
    let color = style.text.mix(style.active_text, active);
    let room = icon_room(style, icon);
    if let Some(icon) = icon {
        IconMark::new(icon, color).paint(ui, icon_rect(cell, style))?;
    }
    Label { color, ..label }.centered().paint(
        ui,
        Rect::new(
            cell.x + room,
            cell.y,
            (cell.width - close_width - room).max(0.0),
            cell.height,
        )
        .inset_xy(style.tab_padding / 2.0, 0.0),
    )
}

fn bar_part<Id: WidgetId, P>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    bar: DockBar,
    style: &DockStyle,
    change: &mut Option<Change<P>>,
) -> AureaResult<()> {
    let part = BAR_PART.saturating_add(u16::try_from(bar.index).unwrap_or(u16::MAX));
    let reach = match bar.axis {
        DockAxis::Row => bar.rect.inset_xy(-3.0, 0.0),
        DockAxis::Column => bar.rect.inset_xy(0.0, -3.0),
    };
    let input = ui.interact(id, part, reach, Sense::Hover, true);
    let state = ui.eased(id, part, input);
    let (arrows, line) = match bar.axis {
        DockAxis::Row => (
            CursorIcon::ResizeHorizontal,
            Rect::new(bar.rect.center().x - 0.5, bar.rect.y, 1.0, bar.rect.height),
        ),
        DockAxis::Column => (
            CursorIcon::ResizeVertical,
            Rect::new(bar.rect.x, bar.rect.center().y - 0.5, bar.rect.width, 1.0),
        ),
    };
    if input.hovered || input.held {
        ui.set_cursor(arrows);
    }
    if input.held
        && let Some(at) = ui.pointer()
    {
        *change = Some(Change::Ratio(
            bar.index,
            ratio_at(bar, at, style.metrics.gap),
        ));
    }
    let lit = state.hover.max(state.press);
    ui.fill(line, style.bar.mix(style.accent, lit))
}

fn ratio_at(bar: DockBar, at: Point, gap: f32) -> f32 {
    let (from, length, p) = match bar.axis {
        DockAxis::Row => (bar.span.x, bar.span.width, at.x),
        DockAxis::Column => (bar.span.y, bar.span.height, at.y),
    };
    let room = (length - gap).max(1.0);
    (p - from - gap / 2.0) / room
}

fn follow_drag<'t, Id: WidgetId, P: Copy + PartialEq>(
    ui: &mut Ui<'_, Id>,
    dock: &mut Dock<P>,
    held: bool,
    title: &impl Fn(P) -> &'t str,
    marks: &TabMarks<'_, P>,
    style: &DockStyle,
    response: &mut Response,
) -> AureaResult<()> {
    let Some(panel) = dock.drag else {
        return Ok(());
    };
    if !held {
        if let Some((beside, side)) = dock.target.take()
            && dock.tree.move_to(panel, beside, side)
        {
            response.changed = true;
            ui.mark_changed();
        }
        dock.drag = None;
        dock.press = None;
        return Ok(());
    }
    response.held = true;
    ui.set_cursor(CursorIcon::Move);
    let Some(at) = ui.pointer() else {
        return Ok(());
    };
    dock.target = drop_target(&dock.layout, at);
    if let Some((_, side)) = dock.target
        && let Some(group) = dock.layout.group_at(at)
    {
        let shown = side.preview(group.content).inset(4.0);
        Surface::new(style.preview)
            .radius(style.radius)
            .border(1.0, style.accent)
            .paint(ui, shown)?;
    }
    let label = Label::new(title(panel), style.active_text).size(style.text_size);
    let icon = marks.icon(panel);
    let room = icon_room(style, icon);
    let width = (label.width(ui) + style.tab_padding * 2.0 + room).min(style.max_tab);
    let chip = Rect::new(at.x + 12.0, at.y + 8.0, width, style.metrics.strip);
    Surface::new(style.active_tab)
        .radius(style.radius)
        .border(1.0, style.accent)
        .paint(ui, chip)?;
    if let Some(icon) = icon {
        IconMark::new(icon, style.active_text).paint(ui, icon_rect(chip, style))?;
    }
    label.centered().paint(
        ui,
        Rect::new(
            chip.x + room,
            chip.y,
            (chip.width - room).max(0.0),
            chip.height,
        )
        .inset_xy(style.tab_padding / 2.0, 0.0),
    )
}

fn drop_target<P: Copy>(layout: &DockLayout<P>, at: Point) -> Option<(P, DockSide)> {
    let group = layout.group_at(at)?;
    let beside = *layout.panels.get(group.tabs.start)?;
    let side = if group.strip.contains(at) {
        DockSide::Center
    } else {
        DockSide::at(group.content, at)
    };
    Some((beside, side))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::UiState;
    use crate::window::{KeyCode, Modifiers, MouseButton, MouseButtons, PointerKind, WindowEvent};

    const AREA: Rect = Rect::new(0.0, 0.0, 404.0, 300.0);

    struct Rig {
        raster: CpuRasterizer,
        ui: UiState<u8>,
        theme: Theme,
        dock: Dock<u8>,
        drawn: Vec<(u8, Rect)>,
        tab_events: Vec<DockTabEvent<u8>>,
        close_on: CloseButtons,
        dirty: Vec<u8>,
    }

    /// Tabs are this wide, so a tab's close slot sits at a known x.
    const TAB_WIDTH: f32 = 40.0;
    /// Centre of tab 2's close slot (group 1 starts at 204).
    const TWO_CLOSE: (f32, f32) = (204.0 + TAB_WIDTH - 10.5, 14.0);

    impl Rig {
        fn new() -> Self {
            let mut tree = DockTree::new(1);
            tree.insert(2, 1, DockSide::Right);
            tree.insert(3, 2, DockSide::Center);
            let mut ui = UiState::new();
            ui.set_reduced_motion(true);
            let mut rig = Self {
                raster: CpuRasterizer::new(404, 300),
                ui,
                theme: Theme::dark(),
                dock: Dock::new(tree),
                drawn: Vec::new(),
                tab_events: Vec::new(),
                close_on: CloseButtons::Always,
                dirty: Vec::new(),
            };
            rig.frame();
            rig
        }

        fn frame(&mut self) {
            let mut ctx = self.raster.begin_frame().expect("frame");
            self.drawn.clear();
            {
                let mut style = DockStyle::new(&self.theme);
                style.max_tab = TAB_WIDTH;
                style.close_on = self.close_on;
                let mut ui = self.ui.frame(&mut *ctx, &self.theme);
                let drawn = &mut self.drawn;
                let dirty = &self.dirty;
                let title = |p: u8| ["", "One", "Two", "Three"][usize::from(p)];
                DockView::new(&mut self.dock, title, |_: &mut Ui<'_, u8>, panel, rect| {
                    drawn.push((panel, rect));
                    Ok(())
                })
                .style(style)
                .close_buttons(true)
                .dirty(move |p| dirty.contains(&p))
                .show(&mut ui, 0, AREA)
                .expect("draw");
            }
            drop(ctx);
            self.tab_events = self.dock.take_tab_events();
            self.raster.end_frame().expect("end");
        }

        /// Red channel of the pixel at `(x, y)` in the last frame.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        fn red(&self, x: f32, y: f32) -> u8 {
            let frame = self.raster.frame_output().expect("pixels");
            let (w, bytes) = (frame.width as usize, frame.bytes());
            bytes[(y as usize * w + x as usize) * 4]
        }

        fn button(&mut self, x: f32, y: f32, pressed: bool) {
            self.ui.handle(&WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                modifiers: Modifiers::default(),
                x: f64::from(x),
                y: f64::from(y),
                click_count: 1,
                time: Default::default(),
                pointer: PointerKind::Mouse,
            });
            self.frame();
        }

        fn move_to(&mut self, x: f32, y: f32) {
            self.ui.handle(&WindowEvent::MouseMove {
                x: f64::from(x),
                y: f64::from(y),
                buttons: MouseButtons::default(),
                modifiers: Modifiers::default(),
                time: Default::default(),
                pointer: PointerKind::Mouse,
            });
            self.frame();
        }

        fn shown(&self) -> Vec<u8> {
            self.drawn.iter().map(|(p, _)| *p).collect()
        }
    }

    #[test]
    fn each_group_draws_its_shown_panel_under_its_strip() {
        let rig = Rig::new();
        assert_eq!(rig.shown(), [1, 3]);
        assert_eq!(rig.drawn[0].1, Rect::new(0.0, 28.0, 200.0, 272.0));
    }

    #[test]
    fn tab_motion_requests_frames_and_reduced_motion_settles_it() {
        let mut rig = Rig::new();
        rig.ui.set_reduced_motion(false);
        rig.dock.tree.activate(2);
        rig.frame();
        assert!(rig.ui.tweens.busy());
        assert_eq!(rig.shown(), [1, 2], "only active content is called");
        rig.ui.set_reduced_motion(true);
        rig.frame();
        assert!(!rig.ui.tweens.busy());
        assert_eq!(rig.shown(), [1, 2]);
    }

    #[test]
    fn clicking_a_tab_shows_it() {
        let mut rig = Rig::new();
        rig.move_to(210.0, 14.0);
        rig.button(210.0, 14.0, true);
        rig.button(210.0, 14.0, false);
        rig.frame();
        assert_eq!(rig.shown(), [1, 2]);
    }

    #[test]
    fn tab_context_actions_identify_inactive_panels_without_activating_them() {
        let mut rig = Rig::new();
        rig.ui.handle(&WindowEvent::MouseButton {
            button: MouseButton::Right,
            pressed: true,
            modifiers: Modifiers::default(),
            x: 210.0,
            y: 14.0,
            click_count: 1,
            time: Default::default(),
            pointer: PointerKind::Mouse,
        });
        rig.frame();
        assert_eq!(
            rig.tab_events,
            [DockTabEvent::ContextMenu {
                panel: 2,
                at: Point::new(210.0, 14.0)
            }]
        );
        assert_eq!(rig.shown(), [1, 3]);
        assert!(rig.dock.take_tab_events().is_empty(), "delivered once");
        rig.frame();
        assert!(rig.tab_events.is_empty());
    }

    #[test]
    fn keyboard_close_requests_leave_the_panel_in_the_tree() {
        let mut rig = Rig::new();
        rig.ui.focus(0, CLOSE_PART);
        rig.frame();
        rig.ui.handle(&WindowEvent::KeyInput {
            key: KeyCode::Enter,
            pressed: true,
            modifiers: Modifiers::default(),
        });
        rig.frame();
        assert_eq!(rig.tab_events, [DockTabEvent::CloseRequested(1)]);
        assert_eq!(
            rig.shown(),
            [1, 3],
            "the app decides whether to remove a panel"
        );
        rig.frame();
        assert!(rig.tab_events.is_empty());
    }

    #[test]
    fn dragging_a_bar_resizes_the_split() {
        let mut rig = Rig::new();
        rig.move_to(202.0, 150.0);
        rig.button(202.0, 150.0, true);
        rig.move_to(102.0, 150.0);
        rig.button(102.0, 150.0, false);
        rig.frame();
        assert_eq!(rig.dock.layout().groups[0].rect.width, 100.0);
    }

    #[test]
    fn a_tab_dragged_to_a_side_docks_there() {
        let mut rig = Rig::new();
        rig.move_to(6.0, 14.0);
        rig.button(6.0, 14.0, true);
        rig.move_to(40.0, 30.0);
        assert_eq!(rig.dock.dragging(), Some(1));
        rig.move_to(300.0, 290.0);
        rig.button(300.0, 290.0, false);
        rig.frame();
        assert_eq!(rig.dock.dragging(), None);
        let groups = &rig.dock.layout().groups;
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].rect.width, 404.0, "1 left its place");
        assert!(
            groups[1].rect.y > groups[0].rect.y,
            "and went under 2 and 3"
        );
        assert_eq!(rig.shown(), [3, 1]);
    }

    #[test]
    fn active_or_hovered_close_buttons_leave_inactive_tabs_bare() {
        let mut rig = Rig::new();
        rig.close_on = CloseButtons::ActiveOrHovered;
        let (x, y) = TWO_CLOSE;
        rig.move_to(300.0, 290.0);
        let bare = rig.red(x - 8.0, y);
        assert_eq!(rig.red(x, y), bare, "no cross on an unhovered inactive tab");
        rig.move_to(x, y);
        assert_ne!(rig.red(x, y), bare, "hovering the tab shows its cross");
        rig.button(x, y, true);
        rig.button(x, y, false);
        assert_eq!(rig.tab_events, [DockTabEvent::CloseRequested(2)]);
    }

    #[test]
    fn dirty_tabs_show_a_dot_and_clicking_it_closes() {
        let mut rig = Rig::new();
        rig.close_on = CloseButtons::ActiveOrHovered;
        let (x, y) = TWO_CLOSE;
        rig.move_to(300.0, 290.0);
        let bare = rig.red(x - 8.0, y);
        rig.dirty.push(2);
        rig.frame();
        assert_ne!(
            rig.red(x, y),
            bare,
            "the dot sits where the close button is"
        );
        rig.move_to(x, y);
        rig.button(x, y, true);
        rig.button(x, y, false);
        assert_eq!(rig.tab_events, [DockTabEvent::CloseRequested(2)]);
    }
}
