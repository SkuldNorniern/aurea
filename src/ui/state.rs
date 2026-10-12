//! What canvas widgets keep between frames, and the input that reaches them.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::mem;
use std::rc::Rc;
use std::time::{Duration, Instant};

use aurea_animation::Tweens;

use super::paint::TextSize;
use super::text_edit::TextEdit;
use super::theme::Theme;
use super::widget::popup::Popup;
use crate::render::{Font, Path, Point, Rect, TextMetrics};
use crate::window::{CursorIcon, KeyCode, Modifiers, MouseButton, Window, WindowEvent};

/// What identifies a widget from one frame to the next: usually an app enum, like `Hit::Save` or
/// `Hit::Volume`.
pub trait WidgetId: Copy + Eq + Hash + Debug {}

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
    Hover,
    Press,
    Focus,
    /// What the widget shows: a switch's position, a knob, a highlight.
    Value,
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct Press<Id> {
    pub key: Key<Id>,
    pub at: Point,
    pub clicks: u8,
    pub shift: bool,
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

/// The state canvas widgets keep between frames.
#[derive(Debug)]
pub struct UiState<Id: WidgetId> {
    pub(crate) tweens: Tweens<(Key<Id>, Channel)>,
    pointer: Option<Point>,
    pointer_events: Vec<Pointer>,
    pub(crate) typed: Vec<Typed>,
    hits: Vec<Hit<Id>>,
    last_hits: Vec<Hit<Id>>,
    order: Vec<Key<Id>>,
    last_order: Vec<Key<Id>>,
    hot: Option<Key<Id>>,
    pub(crate) active: Option<Key<Id>>,
    pub(crate) focus: Option<Key<Id>>,
    pub(crate) focus_visible: bool,
    focus_seen: bool,
    pub(crate) clicked: Vec<Key<Id>>,
    pub(crate) press: Option<Press<Id>>,
    pub(crate) secondary: Option<(Key<Id>, Point)>,
    pub(crate) popup: Popup<Id>,
    pub(crate) edit: TextEdit,
    pub(crate) edit_for: Option<Key<Id>>,
    pub(crate) blink_from: Instant,
    pub(crate) wheel: (f32, f32),
    pub(crate) path: Path,
    pub(crate) scratch: String,
    pub(crate) probe: String,
    measures: HashMap<u64, (TextMetrics, bool)>,
    pub(crate) fonts: Option<Rc<Fonts>>,
    pub(crate) tip: Tip,
    pub(crate) cursor: CursorIcon,
    pub(crate) ime_area: Option<Rect>,
    pub(crate) changed: bool,
    wants_frame: bool,
    sent: (CursorIcon, Option<Rect>),
}

impl<Id: WidgetId> Default for UiState<Id> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Id: WidgetId> UiState<Id> {
    pub fn new() -> Self {
        Self {
            tweens: Tweens::new(),
            pointer: None,
            pointer_events: Vec::new(),
            typed: Vec::new(),
            hits: Vec::new(),
            last_hits: Vec::new(),
            order: Vec::new(),
            last_order: Vec::new(),
            hot: None,
            active: None,
            focus: None,
            focus_visible: false,
            focus_seen: false,
            clicked: Vec::new(),
            press: None,
            secondary: None,
            popup: Popup::default(),
            edit: TextEdit::new(),
            edit_for: None,
            blink_from: Instant::now(),
            wheel: (0.0, 0.0),
            path: Path::new(),
            scratch: String::new(),
            probe: String::new(),
            measures: HashMap::new(),
            fonts: None,
            tip: Tip::default(),
            cursor: CursorIcon::Default,
            ime_area: None,
            changed: false,
            wants_frame: false,
            sent: (CursorIcon::Default, None),
        }
    }

    /// Values jump to their targets instead of easing, for people who asked the system for less
    /// motion: pass [`crate::reduced_motion`].
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.tweens.set_instant(reduced);
    }

    /// Takes one window event, in the canvas's coordinates.
    pub fn handle(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::MouseMove { x, y, .. } => self.pointer_moved(point(*x, *y)),
            WindowEvent::MouseExited => {
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
            } => {
                let at = point(*x, *y);
                self.pointer = Some(at);
                self.pointer_events.push(if *pressed {
                    Pointer::Press(at, *click_count, *modifiers)
                } else {
                    Pointer::Release(at)
                });
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
                self.pointer_events.push(Pointer::Secondary(at));
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
                self.active = None;
                true
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
        if self.focus.is_none() && key != KeyCode::Tab && !self.popup.is_open() {
            return false;
        }
        self.typed.push(Typed::Key(key, modifiers));
        true
    }

    fn text_input(&mut self, typed: Typed) -> bool {
        if !self.text_focused() {
            return false;
        }
        self.typed.push(typed);
        true
    }

    /// Whether a text field has the keyboard, so the app should leave typing and editing shortcuts
    /// to it.
    pub fn text_focused(&self) -> bool {
        self.focus.is_some() && self.edit_for == self.focus
    }

    /// Whether anything has the keyboard.
    pub fn has_focus(&self) -> bool {
        self.focus.is_some()
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

    /// Whether another frame is needed: something is easing, a caret is blinking, or a click
    /// changed what a widget shows.
    pub fn wants_frame(&self) -> bool {
        self.wants_frame
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
        self.last_hits
            .iter()
            .rev()
            .find(|hit| hit.rect.contains(at))
    }

    pub(crate) fn begin(&mut self, theme: &Theme) {
        if !self.fonts.as_ref().is_some_and(|f| f.matches(theme)) {
            self.fonts = Some(Rc::new(Fonts::new(theme)));
        }
        self.changed = false;
        for event in mem::take(&mut self.pointer_events) {
            self.resolve_pointer(event);
        }
        self.resolve_keys();
        if let Some(at) = self.pointer {
            self.hot = self.hit_at(at).map(|hit| hit.key);
        }
        self.order.clear();
        self.hits.clear();
        self.tweens.frame();
        self.cursor = CursorIcon::Default;
        self.ime_area = None;
        self.tip.anchor = None;
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
                        shift: modifiers.shift,
                    });
                }
            }
            Pointer::Release(at) => {
                let under = self.hit_at(at).map(|hit| hit.key);
                if let Some(active) = self.active.take()
                    && under == Some(active)
                {
                    self.clicked.push(active);
                }
            }
        }
    }

    fn resolve_keys(&mut self) {
        let mut typed = mem::take(&mut self.typed);
        typed.retain(|typed| match typed {
            Typed::Key(key, modifiers) => !self.resolve_key(*key, *modifiers),
            _ => true,
        });
        self.typed = typed;
    }

    fn resolve_key(&mut self, key: KeyCode, modifiers: Modifiers) -> bool {
        if self.popup.is_open() {
            let (used, closed) = self.popup.key(key);
            if let Some(focus) = closed {
                self.focus = focus.or(self.focus);
                self.focus_visible = true;
                self.changed = true;
            }
            if used {
                return true;
            }
        }
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
        let order = &self.last_order;
        if order.is_empty() {
            return;
        }
        let at = self.focus.and_then(|f| order.iter().position(|k| *k == f));
        let next = match (at, back) {
            (None, false) => 0,
            (None, true) => order.len() - 1,
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
        };
        self.focus = order.get(next).copied();
        self.focus_visible = true;
    }

    pub(crate) fn end(&mut self) {
        if !self.focus_seen {
            self.focus = None;
        }
        if self.edit_for != self.focus {
            self.edit_for = None;
        }
        mem::swap(&mut self.hits, &mut self.last_hits);
        mem::swap(&mut self.order, &mut self.last_order);
        let blinking = self.text_focused() && self.blink_from.elapsed() < BLINK_FOR;
        self.wants_frame =
            self.tweens.busy() || self.changed || !self.clicked.is_empty() || blinking;
        self.clicked.clear();
        self.typed.clear();
        self.press = None;
        self.secondary = None;
        self.wheel = (0.0, 0.0);
        self.measures.retain(|_, (_, used)| mem::take(used));
    }

    pub(crate) fn interact(
        &mut self,
        key: Key<Id>,
        rect: Rect,
        sense: Sense,
        enabled: bool,
    ) -> Interaction {
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
                Sense::Text => CursorIcon::Text,
                Sense::Hover => self.cursor,
                Sense::Click | Sense::Adjust => CursorIcon::Pointer,
            };
        }
        let focused = enabled && self.focus == Some(key);
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
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        font.family.hash(&mut hasher);
        font.size.to_bits().hash(&mut hasher);
        font.weight.hash(&mut hasher);
        font.style.hash(&mut hasher);
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
