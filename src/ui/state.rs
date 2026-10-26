//! What canvas widgets keep between frames, and the input that reaches them.

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::mem;
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How long a field out of the frame keeps its undo and editing state.
const KEEP_EDITS_FOR: Duration = Duration::from_secs(600);
/// How many fields keep edit state at once.
const KEEP_EDITS: usize = 64;

use aurea_animation::Tweens;
use aurea_foundation::{FastBuild, FastHasher};

use super::component::editable::EditingSession;
use super::paint::TextSize;
use super::text_edit::TextEdit;
use super::theme::Theme;
use super::widget::popup::Popup;
use crate::reduced_motion;
use crate::render::{Font, Path, Point, Rect, TextMetrics};
use crate::window::{CursorIcon, KeyCode, Modifiers, MouseButton, Window, WindowEvent};

/// What identifies a widget from one frame to the next: usually an app enum, like `Hit::Save` or
/// `Hit::Volume`.
pub trait WidgetId: Copy + Eq + Hash + Debug {}

/// Event routing and repaint decisions are independent: a UI-owned event need not change any
/// pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputResponse {
    /// The widgets or popup own this event; do not forward it to app tools.
    pub consumed: bool,
    /// Whether the caller should request a repaint.
    pub redraw: bool,
}

impl<T: Copy + Eq + Hash + Debug> WidgetId for T {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Key<Id> {
    pub id: Id,
    pub part: u16,
}

impl<Id> Key<Id> {
    pub const fn new(id: Id, part: u16) -> Self {
        Self { id, part }
    }
}

/// Which eased value of a widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Pointer hover.
    Hover,
    /// Press progress.
    Press,
    /// Keyboard focus.
    Focus,
    /// What the widget shows: a switch's position, a knob, a highlight.
    Value,
    /// Widget-specific value, by number.
    Custom(u16),
}

/// What a widget does with input, which decides how the keyboard works it once it has focus, and
/// the pointer shape over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sense {
    /// Clicked: Enter and Space press it.
    Click,
    /// Dragged or stepped: the arrow keys reach it.
    Adjust,
    /// Typed into: it takes text and the editing keys.
    Text,
    /// Typed into by the app's own editor: it takes text and an input method's composing text like
    /// [`Sense::Text`], but [`UiState::text_focused`] stays false, so the app's shortcuts still
    /// work. Read what came with [`crate::ui::Ui::typed`].
    Typing,
    /// Hovered only, for a tooltip or a hint.
    Hover,
}

#[derive(Clone, Copy, Debug)]
struct Hit<Id> {
    rect: Rect,
    key: Key<Id>,
    sense: Sense,
    enabled: bool,
}

/// Hits bucketed into a grid of cells, so finding the one under the pointer looks at a cell's few
/// hits instead of all of them. Each list keeps draw order; the last match wins, as in a scan.
#[derive(Debug, Default)]
struct HitGrid {
    origin: Point,
    cell: (f32, f32),
    /// The bounds' size; points on its far edges are inside, as `Rect::contains` has them.
    extent: (f32, f32),
    cols: usize,
    rows: usize,
    /// Where each cell's hit indices start in `items`; one more entry than there are cells.
    starts: Vec<u32>,
    items: Vec<u32>,
    /// Hits spanning too many cells to list in each, or not finite; always checked.
    big: Vec<u32>,
    /// Where the next hit goes in each cell, while building.
    next: Vec<u32>,
}

impl HitGrid {
    /// Below this many hits a scan is as quick.
    const MIN_HITS: usize = 64;
    const MAX_SIDE: usize = 32;
    const MAX_CELLS_PER_HIT: usize = 16;

    fn build(&mut self, rects: impl Iterator<Item = Rect> + Clone) {
        self.cols = 0;
        self.rows = 0;
        let count = rects.clone().count();
        if count < Self::MIN_HITS {
            return;
        }
        let Some(bounds) = rects.clone().filter(|r| listable(*r)).reduce(|a, b| {
            let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
            let (x1, y1) = (a.right().max(b.right()), a.bottom().max(b.bottom()));
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        }) else {
            return;
        };
        let side = |len: f32| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = (len / 48.0).ceil() as usize;
            n.clamp(1, Self::MAX_SIDE)
        };
        let (cols, rows) = (side(bounds.width), side(bounds.height));
        #[allow(clippy::cast_precision_loss)]
        let cell = (bounds.width / cols as f32, bounds.height / rows as f32);
        if !(cell.0 > 0.0 && cell.1 > 0.0) {
            return;
        }
        (self.origin, self.cell, self.cols, self.rows) =
            (Point::new(bounds.x, bounds.y), cell, cols, rows);
        self.extent = (bounds.width, bounds.height);
        self.starts.clear();
        self.starts.resize(cols * rows + 1, 0);
        self.items.clear();
        self.big.clear();
        // Two passes: count per cell, then place, keeping each cell's hits in draw order.
        let layout = (self.origin, self.cell, cols, rows);
        let mut counts = mem::take(&mut self.starts);
        self.each_cell(rects.clone(), layout, true, |_, c| counts[c + 1] += 1);
        for c in 1..counts.len() {
            counts[c] += counts[c - 1];
        }
        let mut items = mem::take(&mut self.items);
        items.resize(counts[cols * rows] as usize, 0);
        let mut next = mem::take(&mut self.next);
        next.clear();
        next.extend_from_slice(&counts[..cols * rows]);
        self.each_cell(rects, layout, false, |i, c| {
            items[next[c] as usize] = i;
            next[c] += 1;
        });
        self.starts = counts;
        self.items = items;
        self.next = next;
    }

    /// Calls `visit` with each hit's index and every cell it is listed in, in draw order; hits
    /// spanning too many cells, or not finite, go to `big` instead, on the `first` pass.
    fn each_cell(
        &mut self,
        rects: impl Iterator<Item = Rect>,
        layout: (Point, (f32, f32), usize, usize),
        first: bool,
        mut visit: impl FnMut(u32, usize),
    ) {
        let cols = layout.2;
        for (i, rect) in rects.enumerate() {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            if rect.is_empty() {
                continue;
            }
            let span = listable(rect).then(|| Self::span(layout, rect));
            let Some((c0, r0, c1, r1)) =
                span.filter(|(c0, r0, c1, r1)| (c1 - c0) * (r1 - r0) <= Self::MAX_CELLS_PER_HIT)
            else {
                if first {
                    self.big.push(i);
                }
                continue;
            };
            for row in r0..r1 {
                for col in c0..c1 {
                    visit(i, row * cols + col);
                }
            }
        }
    }

    /// The cells `rect` touches, edges included, as column and row ranges.
    fn span(
        (origin, cell, cols, rows): (Point, (f32, f32), usize, usize),
        rect: Rect,
    ) -> (usize, usize, usize, usize) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = |v: f32, origin: f32, size: f32, n: usize| {
            (((v - origin) / size).floor().max(0.0) as usize).min(n - 1)
        };
        let c0 = index(rect.x, origin.x, cell.0, cols);
        let r0 = index(rect.y, origin.y, cell.1, rows);
        let c1 = index(rect.right(), origin.x, cell.0, cols) + 1;
        let r1 = index(rect.bottom(), origin.y, cell.1, rows) + 1;
        (c0, r0, c1, r1)
    }

    /// The hits in the cell under `at` and the big ones, or `None` when there is no grid and every
    /// hit has to be scanned.
    fn candidates(&self, at: Point) -> Option<(&[u32], &[u32])> {
        if self.cols == 0 {
            return None;
        }
        let (dx, dy) = (at.x - self.origin.x, at.y - self.origin.y);
        #[allow(clippy::cast_precision_loss)]
        let outside = !(0.0..=self.extent.0).contains(&dx) || !(0.0..=self.extent.1).contains(&dy);
        if outside {
            return Some((&[], &self.big));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (col, row) = (
            ((dx / self.cell.0) as usize).min(self.cols - 1),
            ((dy / self.cell.1) as usize).min(self.rows - 1),
        );
        let c = row * self.cols + col;
        let cell = &self.items[self.starts[c] as usize..self.starts[c + 1] as usize];
        Some((cell, &self.big))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollDrag<Id> {
    pub owner: Key<Id>,
    pub start: Point,
    pub last: Point,
    pub dragging: bool,
    pub seen: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Press<Id> {
    pub key: Key<Id>,
    pub at: Point,
    pub clicks: u8,
    pub modifiers: Modifiers,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Typed {
    Key(KeyCode, Modifiers),
    Text(String),
    Preedit(String, Option<usize>),
}

#[derive(Clone, Copy, Debug)]
enum Pointer {
    Press(Point, u8, Modifiers),
    Release(Point),
    /// The secondary button went down, for a context menu.
    Secondary(Point),
}

/// The fonts widgets draw with, made once per theme rather than per call: a [`Font`] owns its
/// family name, so making one allocates.
#[derive(Clone, Debug)]
pub struct Fonts {
    key: (String, u32, u32),
    /// Body text.
    pub text: Font,
    /// Body text, bold: button labels.
    pub strong: Font,
    /// Values, hints and tooltips.
    pub small: Font,
}

impl Fonts {
    pub(crate) fn new(theme: &Theme) -> Self {
        Self {
            key: Self::key_of(theme),
            text: theme.font(theme.text_size),
            strong: theme.bold(theme.text_size),
            small: theme.font(theme.small_size),
        }
    }

    /// The font for `size`.
    pub fn of(&self, size: TextSize) -> &Font {
        match size {
            TextSize::Body => &self.text,
            TextSize::Strong => &self.strong,
            TextSize::Small => &self.small,
        }
    }

    fn key_of(theme: &Theme) -> (String, u32, u32) {
        (
            theme.font_family.clone(),
            theme.text_size.to_bits(),
            theme.small_size.to_bits(),
        )
    }

    fn matches(&self, theme: &Theme) -> bool {
        self.key.0 == theme.font_family
            && self.key.1 == theme.text_size.to_bits()
            && self.key.2 == theme.small_size.to_bits()
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Tip {
    pub anchor: Option<Rect>,
    pub text: String,
    /// What has been hovered since when, for the delay before showing.
    pub since: Option<(Rect, Instant)>,
}

pub(crate) const BLINK_FOR: Duration = Duration::from_secs(8);
/// How long of each second the caret shows while it blinks.
pub(crate) const BLINK_ON: f32 = 0.6;

/// Whether a blinking caret shows `since` it last moved: always when blinking is off or over.
pub(crate) fn caret_on(blink: bool, since: Duration) -> bool {
    !blink || since >= BLINK_FOR || since.as_secs_f32().fract() < BLINK_ON
}

/// Whether `rect` can be listed in grid cells: finite and not empty.
fn listable(rect: Rect) -> bool {
    !rect.is_empty()
        && [rect.x, rect.y, rect.width, rect.height]
            .iter()
            .all(|v| v.is_finite())
}

/// Whether a widget with `sense` takes typed text.
fn takes_text(sense: Option<Sense>) -> bool {
    matches!(sense, Some(Sense::Text | Sense::Typing))
}

/// The state canvas widgets keep between frames.
#[derive(Debug)]
pub struct UiState<Id: WidgetId> {
    #[cfg(feature = "accessibility")]
    pub(crate) accessibility: Option<Box<super::accessibility::Bridge<Id>>>,
    pub(crate) tweens: Tweens<(Key<Id>, Channel)>,
    pointer: Option<Point>,
    window_focused: bool,
    pub(crate) modifiers: Modifiers,
    pointer_events: Vec<(usize, Pointer)>,
    pub(crate) pointer_down: bool,
    pub(crate) touch_scrolling: bool,
    pub(crate) scroll_drag: Option<ScrollDrag<Id>>,
    scroll_hits: HashMap<Key<Id>, Range<usize>>,
    last_scroll_hits: HashMap<Key<Id>, Range<usize>>,
    pub(crate) typed: Vec<Typed>,
    pub(crate) typed_targets: Vec<Option<Key<Id>>>,
    hits: Vec<Hit<Id>>,
    last_hits: Vec<Hit<Id>>,
    /// `last_hits` by where they are, for the pointer's hit test.
    hit_grid: HitGrid,
    pub(crate) order: Vec<Key<Id>>,
    last_order: Vec<Key<Id>>,
    hot: Option<Key<Id>>,
    pub(crate) active: Option<Key<Id>>,
    pub(crate) focus: Option<Key<Id>>,
    pub(crate) focus_visible: bool,
    focus_seen: bool,
    pub(crate) clicked: Vec<Key<Id>>,
    /// Accepted release position, even when a touch has since left the surface.
    pub(crate) click_at: Option<Point>,
    pub(crate) press: Option<Press<Id>>,
    pub(crate) secondary: Option<(Key<Id>, Point)>,
    pub(crate) popup: Popup<Id>,
    pub(crate) clip: Option<Rect>,
    pub(crate) edit: TextEdit,
    pub(crate) edit_for: Option<Key<Id>>,
    pub(crate) edits: HashMap<Key<Id>, TextEdit>,
    pub(crate) editing: HashMap<Key<Id>, EditingSession>,
    /// When each field with kept edit state was last drawn, so one scrolled out of a list keeps
    /// its undo and draft for a while.
    edit_seen: HashMap<Key<Id>, Instant>,
    pub(crate) blink_from: Instant,
    pub(crate) wheel: (f32, f32),
    pub(crate) path: Path,
    pub(crate) scratch: String,
    pub(crate) probe: String,
    measures: HashMap<u64, (TextMetrics, bool), FastBuild>,
    pub(crate) fonts: Option<Rc<Fonts>>,
    pub(crate) tip: Tip,
    pub(crate) cursor: CursorIcon,
    pub(crate) ime_area: Option<Rect>,
    pub(crate) changed: bool,
    wants_frame: bool,
    sent: (CursorIcon, Option<Rect>),
    /// Whether easing follows the system's reduced motion setting, until the app sets its own.
    motion_follows_system: bool,
}

impl<Id: WidgetId> Default for UiState<Id> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Id: WidgetId> UiState<Id> {
    /// Creates an empty state with nothing focused or hovered. Easing follows the system's
    /// reduced motion setting; see [`Self::set_reduced_motion`].
    pub fn new() -> Self {
        // tests ease the same on every machine
        let follow = !cfg!(test);
        let mut tweens = Tweens::new();
        tweens.set_instant(follow && reduced_motion());
        Self {
            tweens,
            #[cfg(feature = "accessibility")]
            accessibility: None,
            pointer: None,
            window_focused: true,
            modifiers: Modifiers::default(),
            pointer_events: Vec::new(),
            pointer_down: false,
            touch_scrolling: cfg!(any(target_os = "ios", target_os = "android")),
            scroll_drag: None,
            scroll_hits: HashMap::new(),
            last_scroll_hits: HashMap::new(),
            typed: Vec::new(),
            typed_targets: Vec::new(),
            hits: Vec::new(),
            last_hits: Vec::new(),
            hit_grid: HitGrid::default(),
            order: Vec::new(),
            last_order: Vec::new(),
            hot: None,
            active: None,
            focus: None,
            focus_visible: false,
            focus_seen: false,
            clicked: Vec::new(),
            click_at: None,
            press: None,
            secondary: None,
            popup: Popup::default(),
            clip: None,
            edit: TextEdit::new(),
            edit_for: None,
            edits: HashMap::new(),
            editing: HashMap::new(),
            edit_seen: HashMap::new(),
            blink_from: Instant::now(),
            wheel: (0.0, 0.0),
            path: Path::new(),
            scratch: String::new(),
            probe: String::new(),
            measures: HashMap::default(),
            fonts: None,
            tip: Tip::default(),
            cursor: CursorIcon::Default,
            ime_area: None,
            changed: false,
            wants_frame: false,
            sent: (CursorIcon::Default, None),
            motion_follows_system: follow,
        }
    }

    /// Values jump to their targets instead of easing. By default this follows
    /// [`crate::reduced_motion`], read again when the window gains focus or the system theme
    /// changes; setting it here stops that.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.motion_follows_system = false;
        self.tweens.set_instant(reduced);
    }

    fn follow_system_motion(&mut self) {
        if self.motion_follows_system {
            self.tweens.set_instant(reduced_motion());
        }
    }

    /// Takes one window event, in the canvas's coordinates.
    pub fn handle(&mut self, event: &WindowEvent) -> bool {
        self.handle_event(event)
    }

    /// Whether any canvas popup is open, including dropdowns and context menus.
    pub fn popup_open(&self) -> bool {
        self.popup.is_open()
    }

    /// Routes input to the UI and separately reports whether it needs repainting.
    ///
    /// ```
    /// use aurea::ui::UiState;
    /// use aurea::WindowEvent;
    /// let mut state = UiState::<u32>::new();
    /// let event = WindowEvent::Focused;
    /// let input = state.handle_input(&event);
    /// if input.redraw {
    ///     // Request a redraw of this canvas.
    /// }
    /// if !input.consumed {
    ///     // Route the event to application tools or window handling.
    /// }
    /// ```
    pub fn handle_input(&mut self, event: &WindowEvent) -> InputResponse {
        let focused =
            matches!(event, WindowEvent::KeyInput { .. }) && self.pending_focus().is_some();
        let text_focused = matches!(
            event,
            WindowEvent::TextInput { .. } | WindowEvent::ImePreedit { .. }
        ) && takes_text(self.pending_sense());
        let captured = self.active.is_some()
            || self.pointer_events.last().is_some_and(
                |(_, event)| matches!(event, Pointer::Press(at, ..) if self.hit_at(*at).is_some()),
            );
        let popup = self.popup_open();
        let redraw = self.handle_event(event);
        let consumed = match event {
            WindowEvent::MouseMove { .. } => popup || captured || self.hot.is_some(),
            WindowEvent::MouseButton { x, y, .. } => {
                popup || captured || self.hit_at(point(*x, *y)).is_some()
            }
            WindowEvent::MouseWheel { .. } | WindowEvent::RawMouseMotion { .. } => {
                popup || captured || self.pointer.is_some_and(|at| self.hit_at(at).is_some())
            }
            WindowEvent::KeyInput { key, modifiers, .. } => {
                self.window_focused
                    && (popup
                        || focused
                        || (*key == KeyCode::Tab
                            && !modifiers.ctrl
                            && !modifiers.alt
                            && !self.last_order.is_empty()))
            }
            WindowEvent::TextInput { .. } | WindowEvent::ImePreedit { .. } => popup || text_focused,
            WindowEvent::Pen(sample) => {
                popup || captured || self.hit_at(point(sample.x, sample.y)).is_some()
            }
            _ => false,
        };
        InputResponse { consumed, redraw }
    }

    fn handle_event(&mut self, event: &WindowEvent) -> bool {
        if !self.window_focused
            && matches!(
                event,
                WindowEvent::KeyInput { .. }
                    | WindowEvent::TextInput { .. }
                    | WindowEvent::ImePreedit { .. }
            )
        {
            return false;
        }
        #[cfg(feature = "accessibility")]
        if matches!(event, WindowEvent::Focused | WindowEvent::Unfocused)
            && let Some(bridge) = &mut self.accessibility
        {
            bridge.focused(matches!(event, WindowEvent::Focused));
        }
        match event {
            WindowEvent::MouseMove {
                x, y, modifiers, ..
            } => {
                self.modifiers = *modifiers;
                self.pointer_moved(point(*x, *y))
            }
            WindowEvent::MouseExited => {
                self.pointer_down = false;
                self.pointer = None;
                mem::take(&mut self.hot).is_some()
            }
            WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                x,
                y,
                click_count,
                modifiers,
                ..
            } => {
                let at = point(*x, *y);
                self.pointer_down = *pressed;
                self.pointer = Some(at);
                self.modifiers = *modifiers;
                self.pointer_events.push((
                    self.typed.len(),
                    if *pressed {
                        Pointer::Press(at, *click_count, *modifiers)
                    } else {
                        Pointer::Release(at)
                    },
                ));
                true
            }
            WindowEvent::MouseButton {
                button: MouseButton::Right,
                pressed: true,
                x,
                y,
                ..
            } => {
                let at = point(*x, *y);
                self.pointer = Some(at);
                self.pointer_events
                    .push((self.typed.len(), Pointer::Secondary(at)));
                true
            }
            WindowEvent::MouseWheel {
                delta_x, delta_y, ..
            } => {
                self.wheel.0 += to_f32(*delta_x);
                self.wheel.1 += to_f32(*delta_y);
                self.hot.is_some()
            }
            WindowEvent::KeyInput {
                key,
                pressed: true,
                modifiers,
            } => self.key(*key, *modifiers),
            WindowEvent::TextInput { text } => self.text_input(Typed::Text(text.clone())),
            WindowEvent::ImePreedit { text, cursor } => {
                self.text_input(Typed::Preedit(text.clone(), *cursor))
            }
            WindowEvent::Unfocused => {
                self.pointer_down = false;
                self.scroll_drag = None;
                self.window_focused = false;
                self.modifiers = Modifiers::default();
                self.active = None;
                if self.popup.is_open() {
                    self.focus = self.popup.close();
                }
                self.pointer_events.clear();
                self.typed.clear();
                self.typed_targets.clear();
                self.clicked.clear();
                self.click_at = None;
                self.wheel = (0.0, 0.0);
                self.edit.cancel_composition();
                for edit in self.edits.values_mut() {
                    edit.cancel_composition();
                }
                self.press = None;
                self.secondary = None;
                true
            }
            WindowEvent::Focused => {
                self.window_focused = true;
                self.follow_system_motion();
                true
            }
            WindowEvent::ThemeChanged { .. } => {
                self.follow_system_motion();
                false
            }
            _ => false,
        }
    }

    fn pointer_moved(&mut self, at: Point) -> bool {
        self.pointer = Some(at);
        let hot = self.hit_at(at).map(|hit| hit.key);
        let changed = hot != self.hot;
        self.hot = hot;
        changed || self.active.is_some()
    }

    fn key(&mut self, key: KeyCode, modifiers: Modifiers) -> bool {
        if self.pending_focus().is_none() && key != KeyCode::Tab && !self.popup.is_open() {
            return false;
        }
        self.typed.push(Typed::Key(key, modifiers));
        true
    }

    fn text_input(&mut self, typed: Typed) -> bool {
        if (self.popup.is_open() && (!self.popup.is_card() || !self.popup.owns_focus(self.focus)))
            || !takes_text(self.pending_sense())
        {
            return false;
        }
        self.typed.push(typed);
        true
    }

    fn pending_focus(&self) -> Option<Key<Id>> {
        if !self.window_focused {
            return None;
        }
        if self.popup.is_open() {
            return self.focus;
        }
        let mut focus = self.focus;
        let mut start = 0;
        for (index, pointer) in &self.pointer_events {
            focus = self.focus_after_keys(focus, &self.typed[start..*index]);
            start = *index;
            if let Pointer::Press(at, ..) = pointer {
                focus = self
                    .hit_at(*at)
                    .filter(|hit| hit.enabled && hit.sense != Sense::Hover)
                    .map(|hit| hit.key);
            }
        }
        self.focus_after_keys(focus, &self.typed[start..])
    }

    fn focus_after_keys(&self, focus: Option<Key<Id>>, keys: &[Typed]) -> Option<Key<Id>> {
        keys.iter().fold(focus, |focus, typed| match typed {
            Typed::Key(KeyCode::Tab, modifiers) if !modifiers.ctrl && !modifiers.alt => {
                self.tab_focus(focus, modifiers.shift)
            }
            Typed::Key(KeyCode::Escape, _) => None,
            _ => focus,
        })
    }

    fn pending_sense(&self) -> Option<Sense> {
        let focus = self.pending_focus()?;
        self.last_hits
            .iter()
            .find(|hit| hit.key == focus)
            .map(|hit| hit.sense)
    }

    /// Whether a text field has the keyboard, so the app should leave typing and editing shortcuts
    /// to it.
    pub fn text_focused(&self) -> bool {
        self.window_focused && self.focused_sense() == Some(Sense::Text)
    }

    /// Whether anything has the keyboard.
    pub fn has_focus(&self) -> bool {
        self.window_focused && self.focus.is_some()
    }

    /// Takes the keyboard away from whatever has it.
    pub fn clear_focus(&mut self) {
        self.focus = None;
        self.wants_frame = true;
    }

    pub(crate) fn give_focus(&mut self, key: Option<Key<Id>>) {
        self.focus = key;
        self.focus_seen = key.is_some();
    }

    /// Gives the keyboard to part `part` of widget `id`, as if Tab had got there: a text field
    /// selects all of its text.
    pub fn focus(&mut self, id: Id, part: u16) {
        self.focus = Some(Key::new(id, part));
        self.focus_visible = true;
        self.wants_frame = true;
    }

    /// Whether another frame is needed now: something is easing, or a click changed what a
    /// widget shows. A blinking caret needs one only at [`Self::next_frame_at`].
    pub fn wants_frame(&self) -> bool {
        self.wants_frame
    }

    /// When the next frame is due even with nothing else going on: the caret's next blink, while a
    /// text field has the keyboard. Schedule it with `request_canvas_redraw_at`; the iOS and Android
    /// hosts do it themselves.
    pub fn next_frame_at(&self) -> Option<Instant> {
        if !self.window_focused || !takes_text(self.focused_sense()) {
            return None;
        }
        let since = self.blink_from.elapsed();
        if since >= BLINK_FOR {
            return None;
        }
        let into = since.as_secs_f32().fract();
        let wait = if into < BLINK_ON {
            BLINK_ON - into
        } else {
            1.0 - into
        };
        Some(Instant::now() + Duration::from_secs_f32(wait))
    }

    /// The pointer shape for what the pointer is over.
    pub fn cursor(&self) -> CursorIcon {
        self.cursor
    }

    /// Where a focused text field's caret is, for [`crate::Window::set_ime_cursor_area`], so an
    /// input method's window opens next to it.
    pub fn ime_area(&self) -> Option<Rect> {
        self.ime_area
    }

    /// Tells `window` the pointer shape and the caret area for an input method, when either changed
    /// since it was last told.
    pub fn sync_window(&mut self, window: &Window) {
        if self.cursor != self.sent.0 {
            match window.set_cursor(self.cursor) {
                Ok(()) => self.sent.0 = self.cursor,
                Err(e) => log::debug!("cursor: {e}"),
            }
        }
        if self.ime_area != self.sent.1 {
            if let Some(area) = self.ime_area
                && let Err(e) = window.set_ime_cursor_area(area)
            {
                log::debug!("input method area: {e}");
            }
            self.sent.1 = self.ime_area;
        }
    }

    /// Wheel movement since the last frame that no widget used.
    pub fn wheel(&self) -> (f32, f32) {
        self.wheel
    }

    fn hit_at(&self, at: Point) -> Option<&Hit<Id>> {
        let hits = &self.last_hits;
        let found = |i: &u32| {
            let hit = &hits[*i as usize];
            (!hit.rect.is_empty() && hit.rect.contains(at)).then_some(*i)
        };
        match self.hit_grid.candidates(at) {
            None => hits
                .iter()
                .rev()
                .find(|hit| !hit.rect.is_empty() && hit.rect.contains(at)),
            Some((cell, big)) => {
                let last = |list: &[u32]| list.iter().rev().find_map(found);
                last(cell).max(last(big)).map(|i| &hits[i as usize])
            }
        }
    }

    pub(crate) fn begin(&mut self, theme: &Theme) {
        if !self.fonts.as_ref().is_some_and(|f| f.matches(theme)) {
            self.fonts = Some(Rc::new(Fonts::new(theme)));
        }
        self.changed = false;
        #[cfg(feature = "accessibility")]
        self.accessibility_begin();
        self.popup.begin_frame();
        self.resolve_keys();
        if let Some(at) = self.pointer {
            self.hot = self.hit_at(at).map(|hit| hit.key);
        }
        self.order.clear();
        self.hits.clear();
        self.scroll_hits.clear();
        if let Some(drag) = &mut self.scroll_drag {
            drag.seen = false;
        }
        self.tweens.frame();
        self.cursor = CursorIcon::Default;
        self.ime_area = None;
        self.tip.anchor = None;
        self.clip = None;
        self.focus_seen = false;
    }

    fn resolve_pointer(&mut self, event: Pointer) {
        match event {
            Pointer::Secondary(at) => {
                if self.popup.is_open() {
                    self.popup.close();
                }
                self.secondary = self
                    .hit_at(at)
                    .filter(|hit| hit.enabled)
                    .map(|hit| (hit.key, at));
            }
            Pointer::Press(at, clicks, modifiers) => {
                let hit = self.hit_at(at).copied().filter(|hit| hit.enabled);
                self.active = hit.map(|hit| hit.key);
                self.focus = hit
                    .filter(|hit| hit.sense != Sense::Hover)
                    .map(|hit| hit.key);
                self.focus_visible = false;
                if let Some(hit) = hit {
                    self.press = Some(Press {
                        key: hit.key,
                        at,
                        clicks,
                        modifiers,
                    });
                }
            }
            Pointer::Release(at) => {
                let under = self.hit_at(at).map(|hit| hit.key);
                if let Some(active) = self.active.take()
                    && under == Some(active)
                {
                    self.clicked.push(active);
                    self.click_at = Some(at);
                }
            }
        }
    }

    fn resolve_keys(&mut self) {
        let mut typed = mem::take(&mut self.typed);
        let mut pointer_events = mem::take(&mut self.pointer_events);
        let mut pointers = pointer_events.drain(..).peekable();
        self.typed_targets.clear();
        let mut index = 0;
        typed.retain(|typed| {
            while pointers.peek().is_some_and(|(at, _)| *at <= index) {
                if let Some((_, event)) = pointers.next() {
                    self.resolve_pointer(event);
                }
            }
            index += 1;
            if let Typed::Key(key, modifiers) = typed
                && self.resolve_key(*key, *modifiers)
            {
                return false;
            }
            self.typed_targets.push(self.focus);
            true
        });
        for (_, event) in pointers {
            self.resolve_pointer(event);
        }
        self.pointer_events = pointer_events;
        self.typed = typed;
    }

    fn resolve_key(&mut self, key: KeyCode, modifiers: Modifiers) -> bool {
        if self.popup.is_card() {
            if key == KeyCode::Escape {
                self.focus = self.popup.close();
                self.focus_visible = true;
                self.changed = true;
                return true;
            }
            if key == KeyCode::Tab && !modifiers.ctrl && !modifiers.alt {
                self.focus = self.popup.next_focus(self.focus, modifiers.shift);
                self.focus_visible = true;
                return true;
            }
            if !self.popup.owns_focus(self.focus) {
                return true;
            }
        } else if self.popup.is_open() {
            if key == KeyCode::Tab {
                self.last_order
                    .retain(|key| !self.popup.owns_menu_part(*key));
            }
            let (used, closed) = self.popup.key(key);
            if let Some(focus) = closed {
                self.focus = focus;
                self.focus_visible = true;
                self.changed = true;
            }
            if used || closed.is_none() {
                return true;
            }
        }
        self.resolve_widget_key(key, modifiers)
    }

    fn resolve_widget_key(&mut self, key: KeyCode, modifiers: Modifiers) -> bool {
        let sense = self.focused_sense();
        match key {
            KeyCode::Tab if !modifiers.ctrl && !modifiers.alt => {
                self.move_focus(modifiers.shift);
                true
            }
            KeyCode::Enter | KeyCode::Space if sense == Some(Sense::Click) => {
                self.clicked.extend(self.focus);
                true
            }
            KeyCode::Escape if sense != Some(Sense::Text) => {
                self.focus = None;
                true
            }
            _ => false,
        }
    }

    fn focused_sense(&self) -> Option<Sense> {
        let focus = self.focus?;
        self.last_hits
            .iter()
            .find(|hit| hit.key == focus)
            .map(|hit| hit.sense)
    }

    fn move_focus(&mut self, back: bool) {
        self.focus = self.tab_focus(self.focus, back);
        self.focus_visible = true;
    }

    fn tab_focus(&self, focus: Option<Key<Id>>, back: bool) -> Option<Key<Id>> {
        let order = &self.last_order;
        if order.is_empty() {
            return focus;
        }
        let at = focus.and_then(|f| order.iter().position(|k| *k == f));
        let next = match (at, back) {
            (None, false) => 0,
            (None, true) => order.len() - 1,
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
        };
        order.get(next).copied()
    }

    /// Drops edit state for fields not drawn for [`KEEP_EDITS_FOR`], and the stalest past
    /// [`KEEP_EDITS`].
    fn forget_old_edits(&mut self) {
        let now = Instant::now();
        for hit in &self.hits {
            let kept = self.edits.contains_key(&hit.key)
                || self.editing.contains_key(&hit.key)
                || self.edit_for == Some(hit.key);
            if kept {
                self.edit_seen.insert(hit.key, now);
            }
        }
        let seen = &mut self.edit_seen;
        for key in self.edits.keys().chain(self.editing.keys()) {
            seen.entry(*key).or_insert(now);
        }
        seen.retain(|_, at| now.duration_since(*at) < KEEP_EDITS_FOR);
        // Fields drawn this frame always keep theirs; of the rest, the stalest go first.
        let mut unseen: Vec<(Instant, Key<Id>)> = seen
            .iter()
            .filter(|(_, at)| **at < now)
            .map(|(key, at)| (*at, *key))
            .collect();
        if unseen.len() > KEEP_EDITS {
            let extra = unseen.len() - KEEP_EDITS;
            unseen.select_nth_unstable_by_key(extra - 1, |(at, _)| *at);
            for (_, key) in &unseen[..extra] {
                seen.remove(key);
            }
        }
        self.edits.retain(|key, _| seen.contains_key(key));
        self.editing.retain(|key, _| seen.contains_key(key));
    }

    pub(crate) fn end(&mut self) {
        if !self.focus_seen {
            self.focus = None;
        }
        if self.edit_for != self.focus {
            if let Some(key) = self.edit_for.take() {
                self.edits.insert(key, mem::take(&mut self.edit));
            }
            if let Some(key) = self.focus
                && let Some(edit) = self.edits.remove(&key)
            {
                self.edit = edit;
                self.edit_for = Some(key);
            }
        }
        if self
            .scroll_drag
            .as_ref()
            .is_some_and(|d| !d.seen || !self.pointer_down)
        {
            self.scroll_drag = None;
        }
        mem::swap(&mut self.scroll_hits, &mut self.last_scroll_hits);
        self.forget_old_edits();
        mem::swap(&mut self.hits, &mut self.last_hits);
        self.hit_grid
            .build(self.last_hits.iter().map(|hit| hit.rect));
        mem::swap(&mut self.order, &mut self.last_order);
        #[cfg(feature = "accessibility")]
        if let Some(bridge) = &mut self.accessibility {
            let modal = self.popup.is_open().then_some(self.popup.focus_order());
            bridge.finish(
                self.focus,
                modal,
                self.popup.is_open() && !self.popup.is_card(),
            );
        }
        self.wants_frame = self.tweens.busy() || self.changed || !self.clicked.is_empty();
        self.clicked.clear();
        self.click_at = None;
        self.typed.clear();
        self.typed_targets.clear();
        self.press = None;
        self.secondary = None;
        self.wheel = (0.0, 0.0);
        self.measures.retain(|_, (_, used)| mem::take(used));
    }

    pub(crate) fn scroll_begin(&self) -> usize {
        self.hits.len()
    }

    pub(crate) fn scroll_end(&mut self, key: Key<Id>, start: usize) {
        if self.touch_scrolling {
            self.scroll_hits.insert(key, start..self.hits.len());
        }
    }

    pub(crate) fn scroll_origin(&self, key: Key<Id>, rect: Rect) -> Option<Point> {
        let press = self.press?;
        if !self.pointer_down || !rect.contains(press.at) || self.popup.is_open() {
            return None;
        }
        let (index, hit) = self
            .last_hits
            .iter()
            .enumerate()
            .rev()
            .find(|(_, hit)| hit.enabled && hit.rect.contains(press.at))?;
        if matches!(hit.sense, Sense::Adjust | Sense::Text) {
            return None;
        }
        let owner = self
            .last_scroll_hits
            .iter()
            .filter(|(_, range)| range.contains(&index))
            .min_by_key(|(_, range)| range.len())
            .map(|(owner, _)| *owner);
        (owner == Some(key)).then_some(press.at)
    }

    pub(crate) fn interact(
        &mut self,
        key: Key<Id>,
        rect: Rect,
        sense: Sense,
        enabled: bool,
    ) -> Interaction {
        let rect = match self.clip {
            Some(clip) => rect
                .intersection(clip)
                .unwrap_or_else(|| Rect::new(rect.x, rect.y, 0.0, 0.0)),
            None => rect,
        };
        let enabled = enabled && rect.width > 0.0 && rect.height > 0.0;
        #[cfg(feature = "accessibility")]
        if let Some(bridge) = &mut self.accessibility {
            bridge.register(key, rect, sense, enabled);
        }
        self.hits.push(Hit {
            rect,
            key,
            sense,
            enabled,
        });
        if enabled && sense != Sense::Hover {
            self.order.push(key);
        }
        if self.focus == Some(key) {
            self.focus_seen = enabled;
        }
        let hovered = enabled && self.hot == Some(key);
        let held = enabled && self.active == Some(key);
        if hovered || held {
            self.cursor = match sense {
                Sense::Text | Sense::Typing => CursorIcon::Text,
                Sense::Hover => self.cursor,
                Sense::Click | Sense::Adjust => CursorIcon::Pointer,
            };
        }
        let focused = self.window_focused && enabled && self.focus == Some(key);
        Interaction {
            hovered,
            held,
            clicked: enabled && self.clicked.contains(&key),
            secondary_clicked: enabled && self.secondary.is_some_and(|(k, _)| k == key),
            focused,
            focus_visible: focused && self.focus_visible,
            enabled,
        }
    }

    pub(crate) fn pointer(&self) -> Option<Point> {
        self.pointer
    }

    pub(crate) fn measure(
        &mut self,
        text: &str,
        font: &Font,
        measure: impl FnOnce() -> TextMetrics,
    ) -> TextMetrics {
        let mut hasher = FastHasher::default();
        text.hash(&mut hasher);
        font.family.hash(&mut hasher);
        font.size.to_bits().hash(&mut hasher);
        font.weight.hash(&mut hasher);
        font.style.hash(&mut hasher);
        font.features.hash(&mut hasher);
        font.variations.len().hash(&mut hasher);
        for variation in &font.variations {
            variation.tag.hash(&mut hasher);
            variation.value.to_bits().hash(&mut hasher);
        }
        font.vertical.hash(&mut hasher);
        let entry = self
            .measures
            .entry(hasher.finish())
            .or_insert_with(|| (measure(), true));
        entry.1 = true;
        entry.0
    }
}

/// How the input meets a widget this frame, from [`Ui::interact`].
///
/// [`Ui::interact`]: super::Ui::interact
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Interaction {
    /// The pointer is over it.
    pub hovered: bool,
    /// A press on it has not been let go yet.
    pub held: bool,
    /// It was clicked, or pressed from the keyboard, this frame.
    pub clicked: bool,
    /// The secondary button went down on it this frame, which asks for a context menu.
    pub secondary_clicked: bool,
    /// It has the keyboard.
    pub focused: bool,
    /// It has the keyboard and got it from the keyboard, so it should show a focus ring.
    pub focus_visible: bool,
    /// It takes input at all.
    pub enabled: bool,
}

fn point(x: f64, y: f64) -> Point {
    Point::new(to_f32(x), to_f32(y))
}

#[allow(clippy::cast_possible_truncation)]
fn to_f32(v: f64) -> f32 {
    v as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PointerKind;
    use std::cell::Cell;
    use std::thread::sleep;

    fn move_to(x: f64, y: f64) -> WindowEvent {
        WindowEvent::MouseMove {
            x,
            y,
            buttons: Default::default(),
            modifiers: Modifiers::default(),
            time: Default::default(),
            pointer: PointerKind::Mouse,
        }
    }

    #[test]
    fn event_ownership_is_independent_of_redrawing_and_tracks_queued_drags() {
        let mut state = UiState::<u8>::new();
        let theme = Theme::dark();
        state.begin(&theme);
        state.interact(
            Key::new(1, 0),
            Rect::new(0.0, 0.0, 40.0, 30.0),
            Sense::Click,
            true,
        );
        state.end();
        assert_eq!(
            state.handle_input(&move_to(10.0, 10.0)),
            InputResponse {
                consumed: true,
                redraw: true
            }
        );
        assert_eq!(
            state.handle_input(&move_to(11.0, 10.0)),
            InputResponse {
                consumed: true,
                redraw: false
            }
        );
        let press = WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
            modifiers: Modifiers::default(),
            x: 10.0,
            y: 10.0,
            click_count: 1,
            time: Default::default(),
            pointer: PointerKind::Mouse,
        };
        assert!(state.handle_input(&press).consumed);
        assert!(
            state.handle_input(&move_to(90.0, 90.0)).consumed,
            "pending drag owns motion outside"
        );
        let release = WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed: false,
            modifiers: Modifiers::default(),
            x: 90.0,
            y: 90.0,
            click_count: 1,
            time: Default::default(),
            pointer: PointerKind::Mouse,
        };
        assert!(state.handle_input(&release).consumed);
        assert!(!state.handle_input(&move_to(91.0, 90.0)).consumed);
        assert!(
            !state
                .handle_input(&WindowEvent::Resized {
                    width: 200,
                    height: 200
                })
                .consumed
        );
    }

    #[test]
    fn popups_own_input_and_prevent_editing_the_field_behind_them() {
        use crate::ui::MenuItem;
        let mut state = UiState::<u8>::new();
        let theme = Theme::dark();
        let field = Key::new(1, 0);
        state.begin(&theme);
        state.interact(field, Rect::new(0.0, 0.0, 40.0, 30.0), Sense::Text, true);
        state.give_focus(Some(field));
        state.end();
        state.popup.open(
            Key::new(2, 0),
            Rect::default(),
            0.0,
            [MenuItem::new("Brush")],
            Some(field),
        );
        assert!(state.popup_open());
        let input = state.handle_input(&WindowEvent::TextInput {
            text: "must not type".into(),
        });
        assert!(input.consumed);
        assert!(state.typed.is_empty());
        assert!(
            state
                .handle_input(&WindowEvent::KeyInput {
                    key: KeyCode::Left,
                    pressed: true,
                    modifiers: Modifiers::default()
                })
                .consumed
        );
        state.begin(&theme);
        assert!(
            state.typed.is_empty(),
            "menu-unhandled keys do not edit background text"
        );
        assert!(
            state
                .handle_input(&WindowEvent::KeyInput {
                    key: KeyCode::Escape,
                    pressed: true,
                    modifiers: Modifiers::default()
                })
                .consumed
        );
        state.begin(&theme);
        assert!(!state.popup_open());
        assert_eq!(state.focus, Some(field));
    }

    #[test]
    fn the_hit_grid_finds_what_a_scan_finds() {
        // Overlapping hits of mixed sizes: cells, panels spanning many cells, empty ones.
        let mut seed = 7u32;
        let mut next = |n: u32| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (seed >> 8) % n
        };
        #[allow(clippy::cast_precision_loss)]
        let rects: Vec<Rect> = (0..400)
            .map(|i| {
                let big = i % 37 == 0;
                let (w, h) = if big {
                    (300, 200)
                } else {
                    (next(60), next(40))
                };
                Rect::new(next(900) as f32, next(700) as f32, w as f32, h as f32)
            })
            .collect();
        let mut rects = rects;
        // a hit everywhere, and one that is not finite.
        rects.insert(5, Rect::new(f32::NEG_INFINITY, 0.0, f32::INFINITY, 1e9));
        rects.push(Rect::new(0.0, 0.0, f32::NAN, 10.0));
        // a small hit that sets the grid's far corner.
        rects.push(Rect::new(1990.0, 1490.0, 10.0, 10.0));
        let mut grid = HitGrid::default();
        grid.build(rects.iter().copied());
        assert!(grid.cols > 0, "built");
        let scan = |at: Point| rects.iter().rposition(|r| !r.is_empty() && r.contains(at));
        // every hit's corners, where float edges go wrong.
        let corners: Vec<Point> = rects
            .iter()
            .filter(|r| r.width.is_finite() && r.x.is_finite())
            .flat_map(|r| [Point::new(r.x, r.y), Point::new(r.right(), r.bottom())])
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let random =
            (0..5000).map(|_| Point::new(next(1300) as f32 - 100.0, next(1000) as f32 - 100.0));
        for at in corners.into_iter().chain(random.collect::<Vec<_>>()) {
            let found = |list: &[u32]| {
                list.iter()
                    .rev()
                    .find(|i| {
                        let r = rects[**i as usize];
                        !r.is_empty() && r.contains(at)
                    })
                    .map(|i| *i as usize)
            };
            let from_grid = grid
                .candidates(at)
                .and_then(|(cell, big)| found(cell).max(found(big)));
            assert_eq!(from_grid, scan(at), "at {at:?}");
        }
    }

    #[test]
    fn fields_on_screen_keep_their_edits_and_the_rest_are_capped() {
        let mut state = UiState::<u16>::new();
        let theme = Theme::dark();
        let draw = |state: &mut UiState<u16>, count: u16| {
            state.begin(&theme);
            for k in 0..count {
                let at = Rect::new(0.0, f32::from(k) * 10.0, 40.0, 8.0);
                state.interact(Key::new(k, 0), at, Sense::Text, true);
            }
            state.end();
        };
        draw(&mut state, 70);
        for k in 0..70 {
            state.edits.insert(Key::new(k, 0), TextEdit::new());
        }
        draw(&mut state, 70);
        assert_eq!(state.edits.len(), 70, "all drawn, all kept");
        sleep(Duration::from_millis(2));
        draw(&mut state, 0);
        assert_eq!(state.edits.len(), KEEP_EDITS, "out of sight, capped");
    }

    #[test]
    fn window_focus_loss_dismisses_popups() {
        use crate::ui::MenuItem;
        let mut state = UiState::<u8>::new();
        let field = Key::new(1, 0);
        state.popup.open(
            Key::new(2, 0),
            Rect::default(),
            0.0,
            [MenuItem::new("Brush")],
            Some(field),
        );
        assert!(!state.handle_input(&WindowEvent::Unfocused).consumed);
        assert!(!state.popup_open(), "window focus loss dismisses popups");
    }

    #[test]
    fn controls_outside_the_clip_are_not_keyboard_focus_targets() {
        let mut state = UiState::<u8>::new();
        let theme = Theme::dark();
        let key = Key::new(1, 0);
        state.begin(&theme);
        state.give_focus(Some(key));
        state.clip = Some(Rect::new(0.0, 0.0, 20.0, 20.0));
        let input = state.interact(key, Rect::new(100.0, 100.0, 40.0, 30.0), Sense::Text, true);
        assert!(!input.enabled);
        state.end();
        assert!(!state.has_focus());
        assert!(state.last_order.is_empty());
        assert!(!state.handle_input(&move_to(100.0, 100.0)).consumed);
    }

    #[test]
    fn text_measurements_distinguish_all_shaping_settings_and_reuse_exact_matches() {
        let mut state = UiState::<u8>::new();
        let base = Font::new("Sans", 16.0);
        let fonts = [
            base.clone(),
            base.clone().with_feature("liga", 0),
            base.clone().with_variation("wdth", 75.0),
            base.clone().with_variation("wdth", 125.0),
            base.with_vertical(true),
        ];
        let calls = Cell::new(0u8);
        for (index, font) in (1u8..).zip(&fonts) {
            for _ in 0..2 {
                let measured = state.measure("office", font, || {
                    calls.set(calls.get() + 1);
                    TextMetrics {
                        width: f32::from(index),
                        height: 16.0,
                        ascent: 12.0,
                        descent: 4.0,
                        advance: f32::from(index),
                    }
                });
                assert_eq!(measured.width, f32::from(index));
            }
        }
        assert_eq!(
            calls.get(),
            5,
            "one measurement per distinct shaping configuration"
        );
    }

    #[test]
    #[ignore = "timing; run with --release --ignored --nocapture"]
    fn timing_pen_samples_with_many_widgets() {
        use crate::render::{CpuRasterizer, Renderer};
        use crate::ui::theme::Theme;
        use crate::window::PenSample;
        use std::time::Instant;
        let theme = Theme::dark();
        let mut raster = CpuRasterizer::new(1600, 1000);
        let mut state = UiState::<u32>::new();
        let mut ctx = raster.begin_frame().expect("frame");
        {
            let mut ui = state.frame(&mut *ctx, &theme);
            for i in 0..5000u16 {
                let (x, y) = (f32::from(i % 100) * 16.0, f32::from(i / 100) * 20.0);
                ui.interact(u32::from(i), 0, Rect::new(x, y, 14.0, 18.0), Sense::Click, true);
            }
        }
        drop(ctx);
        let samples: Vec<WindowEvent> = (0..6400u16)
            .map(|i| {
                WindowEvent::Pen(PenSample {
                    x: f64::from(i % 1600),
                    y: f64::from(i % 997),
                    pressure: 0.5,
                    ..PenSample::default()
                })
            })
            .collect();
        let started = Instant::now();
        let mut consumed = 0;
        for event in &samples {
            consumed += usize::from(state.handle_input(event).consumed);
        }
        let per = started.elapsed().as_secs_f64() * 1e9 / samples.len() as f64;
        println!("handle_input per pen sample: {per:.0} ns ({consumed} consumed)");
    }
}
