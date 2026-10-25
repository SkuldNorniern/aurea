//! One frame of canvas widgets: [`Ui`] draws them and answers what the input did to each.

use std::mem;
use std::rc::Rc;

use super::state::{Channel, Fonts, Interaction, Key, Press, Sense, Typed, UiState, WidgetId};
use super::theme::Theme;
use crate::AureaResult;
use crate::render::{Color, DrawingContext, Font, Paint, PaintStyle, Point, Rect, TextMetrics};
use crate::window::{CursorIcon, KeyCode, Modifiers};

/// Where text sits across the space it is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// At the start.
    #[default]
    Start,
    /// In the middle.
    Center,
    /// At the end.
    End,
}

/// What the input did to a widget this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response {
    /// Where the widget was drawn.
    pub rect: Rect,
    /// The pointer is over it.
    pub hovered: bool,
    /// It is being pressed or dragged.
    pub held: bool,
    /// It was clicked, or activated from the keyboard.
    pub clicked: bool,
    /// The secondary button went down on it: open its context menu.
    pub secondary_clicked: bool,
    /// Its value changed this frame.
    pub changed: bool,
    /// It has the keyboard.
    pub focused: bool,
    /// Enter was pressed in a text field.
    pub submitted: bool,
    /// Escape was pressed in a text field, which let go of the keyboard.
    pub cancelled: bool,
    /// A text editor acquired focus this frame.
    pub editing_started: bool,
    /// A text editor released focus this frame, including Escape or disabling.
    pub focus_lost: bool,
    /// Explicit submission, or dirty text committed by opt-in commit-on-blur.
    pub committed: bool,
    /// A list's pick was opened: double-clicked, or Enter pressed on it.
    pub activated: bool,
    /// A list row was pressed: its index, even when it was already the pick.
    pub row_clicked: Option<usize>,
}

impl Response {
    /// The response for `input` on `rect`, nothing changed yet.
    pub fn new(rect: Rect, input: Interaction) -> Self {
        Self {
            rect,
            hovered: input.hovered,
            held: input.held,
            clicked: input.clicked,
            secondary_clicked: input.secondary_clicked,
            activated: false,
            changed: false,
            focused: input.focused,
            submitted: false,
            cancelled: false,
            editing_started: false,
            focus_lost: false,
            committed: false,
            row_clicked: None,
        }
    }
}

/// How far along hover, press and focus are for a widget, each `0..=1`, eased by the theme's
/// motion.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Eased {
    /// Hover amount.
    pub hover: f32,
    /// Press amount.
    pub press: f32,
    /// The focus ring: only for focus that came from the keyboard.
    pub focus: f32,
    /// `1` enabled, faded towards the theme's disabled look when not.
    pub enabled: f32,
}

/// One thing typed for the widget that has the keyboard; see [`Ui::typed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypedInput<'a> {
    /// A key pressed, with the modifiers held.
    Key(KeyCode, Modifiers),
    /// Text to insert.
    Text(&'a str),
    /// Text an input method is still composing, and the cursor in it.
    Preedit(&'a str, Option<usize>),
}

/// A press that landed on a widget this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressAt {
    /// Where the press landed.
    pub at: Point,
    /// 1 for a click, 2 for a double click, and so on.
    pub clicks: u8,
    /// Whether shift was held; also in `modifiers`.
    pub shift: bool,
    /// The modifier keys held: alt for another caret, ctrl or command to follow a link.
    pub modifiers: Modifiers,
}

/// One frame's drawing with widgets.
pub struct Ui<'a, Id: WidgetId> {
    /// The canvas, for drawing anything the widgets do not.
    pub ctx: &'a mut dyn DrawingContext,
    /// Colours, sizes and fonts.
    pub theme: &'a Theme,
    pub(crate) state: &'a mut UiState<Id>,
}

impl<Id: WidgetId> UiState<Id> {
    /// Starts a frame on `ctx`, styled by `theme`.
    ///
    /// ```rust,no_run
    /// # use aurea::render::{Canvas, RendererBackend};
    /// # use aurea::ui::{Button, Theme, UiState};
    /// # use std::{cell::RefCell, rc::Rc};
    /// # fn main() -> aurea::AureaResult<()> {
    /// # let canvas = Canvas::new(400, 300, RendererBackend::Cpu)?;
    /// #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    /// enum Id { Save }
    ///
    /// let ui = Rc::new(RefCell::new(UiState::<Id>::new()));
    /// let theme = Theme::dark();
    /// canvas.set_draw_callback(move |ctx| {
    ///     let mut state = ui.borrow_mut();
    ///     let mut ui = state.frame(ctx, &theme);
    ///     let at = aurea::Rect::new(20.0, 20.0, 96.0, 30.0);
    ///     if Button::new("Save").primary().show(&mut ui, Id::Save, at)?.clicked {
    ///         // save
    ///     }
    ///     Ok(())
    /// })?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn frame<'a>(
        &'a mut self,
        ctx: &'a mut dyn DrawingContext,
        theme: &'a Theme,
    ) -> Ui<'a, Id> {
        self.begin(theme);
        Ui {
            ctx,
            theme,
            state: self,
        }
    }
}

impl<Id: WidgetId> Drop for Ui<'_, Id> {
    fn drop(&mut self) {
        if let Err(e) = super::widget::popup::draw(self) {
            log::warn!("popup: {e}");
        }
        if let Err(e) = super::widget::tooltip::draw(self) {
            log::warn!("tooltip: {e}");
        }
        self.state.end();
    }
}

impl<Id: WidgetId> Ui<'_, Id> {
    /// Puts part `part` of widget `id` on this frame's hit list at `rect`, and says how the input
    /// meets it.
    pub fn interact(
        &mut self,
        id: Id,
        part: u16,
        rect: Rect,
        sense: Sense,
        enabled: bool,
    ) -> Interaction {
        self.state
            .interact(Key::new(id, part), rect, sense, enabled)
    }

    /// Hover, press and focus for `input` on that part, eased.
    pub fn eased(&mut self, id: Id, part: u16, input: Interaction) -> Eased {
        let key = Key::new(id, part);
        let m = self.theme.motion;
        let hover = self.ease(key, Channel::Hover, flag(input.hovered), m.hover);
        let target = flag(input.held);
        let press = self
            .state
            .tweens
            .towards((key, Channel::Press), target, m.press, m.release);
        let focus = self.ease(key, Channel::Focus, flag(input.focus_visible), m.focus);
        let enabled = if input.enabled {
            1.0
        } else {
            self.theme.disabled_alpha
        };
        Eased {
            hover,
            press,
            focus,
            enabled,
        }
    }

    /// `channel` of that part moved towards `target` at `rate` per second.
    pub fn animate(&mut self, id: Id, part: u16, channel: Channel, target: f32, rate: f32) -> f32 {
        self.ease(Key::new(id, part), channel, target, rate)
    }

    /// Like [`Ui::animate`], but a tween read for the first time, or again after it was forgotten
    /// for not being read in a frame, starts at `start` and eases towards `target` from there.
    pub fn animate_from(
        &mut self,
        id: Id,
        part: u16,
        channel: Channel,
        start: f32,
        target: f32,
        rate: f32,
    ) -> f32 {
        self.state
            .tweens
            .from_to((Key::new(id, part), channel), start, target, rate)
    }

    /// Puts `channel` of that part at `value` now, for something the user drags, which should
    /// follow the pointer exactly.
    pub fn set_animated(&mut self, id: Id, part: u16, channel: Channel, value: f32) {
        self.state.tweens.set((Key::new(id, part), channel), value);
    }

    /// Where `channel` of that part is now, if it is animating; reading it does not keep it.
    pub fn animated(&self, id: Id, part: u16, channel: Channel) -> Option<f32> {
        self.state.tweens.get((Key::new(id, part), channel))
    }

    pub(crate) fn ease(&mut self, key: Key<Id>, channel: Channel, target: f32, rate: f32) -> f32 {
        self.state.tweens.to((key, channel), target, rate)
    }

    /// The press that landed on that part this frame, if one did.
    pub fn press(&self, id: Id, part: u16) -> Option<PressAt> {
        let key = Key::new(id, part);
        self.state
            .press
            .filter(|press: &Press<Id>| press.key == key)
            .map(|press| PressAt {
                at: press.at,
                clicks: press.clicks,
                shift: press.modifiers.shift,
                modifiers: press.modifiers,
            })
    }

    /// What was typed since the last frame for the widget that has the keyboard, in order: keys,
    /// text, and what an input method is still composing. A widget gets text and composing text
    /// when it senses [`Sense::Text`] or [`Sense::Typing`].
    pub fn typed(&self) -> impl Iterator<Item = TypedInput<'_>> + '_ {
        self.state
            .typed
            .iter()
            .zip(&self.state.typed_targets)
            .filter(|(_, target)| **target == self.state.focus)
            .map(|(typed, _)| match typed {
                Typed::Key(key, mods) => TypedInput::Key(*key, *mods),
                Typed::Text(text) => TypedInput::Text(text),
                Typed::Preedit(text, cursor) => TypedInput::Preedit(text, *cursor),
            })
    }

    /// Where the caret of a widget being typed into is, so an input method's window opens next to
    /// it; aurea's own fields set it themselves. Pass it on with [`UiState::sync_window`].
    pub fn set_ime_area(&mut self, area: Rect) {
        self.state.ime_area = Some(area);
    }

    /// The keys pressed since the last frame, in order, for the widget that has the keyboard.
    pub fn keys(&self) -> impl Iterator<Item = (KeyCode, Modifiers)> + '_ {
        self.state
            .typed
            .iter()
            .zip(&self.state.typed_targets)
            .filter_map(|(typed, target)| match typed {
                _ if *target != self.state.focus => None,
                Typed::Key(key, mods) => Some((*key, *mods)),
                _ => None,
            })
    }

    /// Notes that a widget's value changed, so the frame after shows it everywhere it is drawn.
    pub fn mark_changed(&mut self) {
        self.state.changed = true;
    }

    /// Takes the keyboard away from whatever has it.
    pub fn clear_focus(&mut self) {
        self.state.clear_focus();
    }

    /// Modifier keys held now.
    pub fn modifiers(&self) -> Modifiers {
        self.state.modifiers
    }

    /// The pointer position over the canvas, if any.
    pub fn pointer(&self) -> Option<Point> {
        self.state.pointer()
    }

    /// Shows `icon` over the canvas this frame, for a part whose pointer says more than a hand: a
    /// splitter's arrows, a crosshair for a tool.
    pub fn set_cursor(&mut self, icon: CursorIcon) {
        self.state.cursor = icon;
    }

    /// Wheel movement since the last frame.
    pub fn wheel(&self) -> (f32, f32) {
        self.state.wheel()
    }

    /// Wheel movement since the last frame, taken so nothing drawn after scrolls with it too: the
    /// innermost scrolling area under the pointer takes it first.
    pub fn take_wheel(&mut self) -> (f32, f32) {
        mem::take(&mut self.state.wheel)
    }

    /// Consumes only the used wheel movement, leaving each axis's remainder for an enclosing scroll
    /// area.
    pub fn consume_wheel(&mut self, used: (f32, f32)) {
        fn remaining(available: f32, used: f32) -> f32 {
            if used.is_finite() && available.signum() == used.signum() {
                available - used.abs().min(available.abs()).copysign(available)
            } else {
                available
            }
        }
        self.state.wheel = (
            remaining(self.state.wheel.0, used.0),
            remaining(self.state.wheel.1, used.1),
        );
    }

    /// Runs `f` with drawing and the pointer both clipped to `rect`, inside any clip already in
    /// force: what scrolls out of a list is neither seen nor clicked.
    pub fn clipped<R>(
        &mut self,
        rect: Rect,
        f: impl FnOnce(&mut Self) -> AureaResult<R>,
    ) -> AureaResult<R> {
        let outer = self.state.clip;
        let inner = match outer {
            Some(clip) => rect
                .intersection(clip)
                .unwrap_or_else(|| Rect::new(rect.x, rect.y, 0.0, 0.0)),
            None => rect,
        };
        self.ctx.save()?;
        self.ctx.clip_rect(inner)?;
        self.state.clip = Some(inner);
        let result = f(self);
        self.state.clip = outer;
        self.ctx.restore()?;
        result
    }

    /// Where the secondary button went down on that part this frame.
    pub fn secondary_at(&self, id: Id, part: u16) -> Option<Point> {
        let key = Key::new(id, part);
        self.state
            .secondary
            .filter(|(k, _)| *k == key)
            .map(|(_, at)| at)
    }

    /// Seconds since the state was made, for looping effects.
    pub fn time(&self) -> f32 {
        self.state.tweens.time()
    }

    /// Asks for another frame after this one, for something the app animates itself.
    pub fn keep_animating(&mut self) {
        self.state.tweens.keep_busy();
    }

    /// Shows `text` under `response`'s rect while the pointer rests on it, after a short delay.
    pub fn tooltip(&mut self, response: &Response, text: &str) {
        if response.hovered && !response.held {
            self.show_tip(response.rect, text);
        }
    }

    /// Shows `text` next to `at`, such as under a squiggle the app found the pointer resting on,
    /// after the same delay; call it each frame it should stay.
    pub fn tooltip_at(&mut self, at: Point, text: &str) {
        self.show_tip(Rect::new(at.x, at.y, 0.0, 0.0), text);
    }

    fn show_tip(&mut self, anchor: Rect, text: &str) {
        self.state.tip.anchor = Some(anchor);
        self.state.tip.text.clear();
        self.state.tip.text.push_str(text);
    }

    /// The theme's fonts, made once per theme.
    pub fn fonts(&self) -> Rc<Fonts> {
        self.state
            .fonts
            .clone()
            .unwrap_or_else(|| Rc::new(Fonts::new(self.theme)))
    }

    /// Measures `text`, from a cache while it keeps being drawn.
    pub fn measure(&mut self, text: &str, font: &Font) -> TextMetrics {
        let ctx = &mut *self.ctx;
        self.state.measure(text, font, || {
            ctx.measure_text(text, font)
                .unwrap_or_else(|_| estimate(text, font))
        })
    }

    /// The width `text` takes in `font`.
    pub fn text_width(&mut self, text: &str, font: &Font) -> f32 {
        self.measure(text, font).width
    }

    /// `text` in one line across `rect`, centred top to bottom.
    pub fn text(
        &mut self,
        text: &str,
        rect: Rect,
        font: &Font,
        color: Color,
        align: Align,
    ) -> AureaResult<()> {
        if text.is_empty() || color.a == 0 {
            return Ok(());
        }
        let m = self.measure(text, font);
        let x = match align {
            Align::Start => rect.x,
            Align::Center => rect.x + (rect.width - m.width) / 2.0,
            Align::End => rect.right() - m.width,
        };
        let baseline = baseline_in(rect, &m, font);
        self.ctx
            .draw_text_with_font(text, Point::new(x, baseline), font, &fill(color))
    }

    /// Like [`Self::text`], cut short with an ellipsis when it is wider than `rect`.
    pub fn text_clipped(
        &mut self,
        text: &str,
        rect: Rect,
        font: &Font,
        color: Color,
        align: Align,
    ) -> AureaResult<bool> {
        if self.text_width(text, font) <= rect.width {
            self.text(text, rect, font, color, align)?;
            return Ok(false);
        }
        let fits = self.longest_prefix(text, font, rect.width);
        let mut shown = mem::take(&mut self.state.scratch);
        shown.clear();
        shown.push_str(text[..fits].trim_end());
        shown.push('\u{2026}');
        let result = self.text(&shown, rect, font, color, align);
        self.state.scratch = shown;
        result.map(|()| true)
    }

    fn longest_prefix(&mut self, text: &str, font: &Font, width: f32) -> usize {
        let end_of = |chars: usize| {
            text.char_indices()
                .nth(chars)
                .map_or(text.len(), |(i, _)| i)
        };
        let mut probe = mem::take(&mut self.state.probe);
        let (mut lo, mut hi) = (0, text.chars().count());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            probe.clear();
            probe.push_str(&text[..end_of(mid)]);
            probe.push('\u{2026}');
            if self.text_width(&probe, font) <= width {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        self.state.probe = probe;
        end_of(lo)
    }

    /// `text` wrapped at word boundaries to `rect`'s width, from its top, lines `line_height`
    /// apart.
    pub fn text_wrapped(
        &mut self,
        text: &str,
        rect: Rect,
        font: &Font,
        color: Color,
        line_height: f32,
    ) -> AureaResult<f32> {
        let mut line = mem::take(&mut self.state.scratch);
        let mut candidate = mem::take(&mut self.state.probe);
        line.clear();
        let result = self.wrap_into(
            text,
            rect,
            (font, color, line_height),
            &mut line,
            &mut candidate,
        );
        self.state.scratch = line;
        self.state.probe = candidate;
        result
    }

    fn wrap_into(
        &mut self,
        text: &str,
        rect: Rect,
        (font, color, line_height): (&Font, Color, f32),
        line: &mut String,
        candidate: &mut String,
    ) -> AureaResult<f32> {
        let mut y = rect.y;
        for word in text.split_whitespace() {
            candidate.clear();
            candidate.push_str(line);
            if !candidate.is_empty() {
                candidate.push(' ');
            }
            candidate.push_str(word);
            if !line.is_empty() && self.text_width(candidate, font) > rect.width {
                let at = Rect::new(rect.x, y, rect.width, line_height);
                self.text(line, at, font, color, Align::Start)?;
                y += line_height;
                line.clear();
                line.push_str(word);
            } else {
                mem::swap(line, candidate);
            }
        }
        if !line.is_empty() {
            let at = Rect::new(rect.x, y, rect.width, line_height);
            self.text(line, at, font, color, Align::Start)?;
            y += line_height;
        }
        Ok(y - rect.y)
    }

    /// `rect` filled with `color`.
    pub fn fill(&mut self, rect: Rect, color: Color) -> AureaResult<()> {
        if color.a == 0 {
            return Ok(());
        }
        self.ctx.draw_rect(rect, &fill(color))
    }

    /// A shadow of `rect` with corners of `radius`, blurred by `blur` pixels as in CSS.
    pub fn shadow(&mut self, rect: Rect, radius: f32, blur: f32, color: Color) -> AureaResult<()> {
        if color.a == 0 {
            return Ok(());
        }
        self.ctx
            .draw_shadow(rect, radius, blur.max(0.0) / 2.0, color)
    }

    /// `rect` filled with corners of `radius`.
    pub fn round(&mut self, rect: Rect, radius: f32, color: Color) -> AureaResult<()> {
        if color.a == 0 {
            return Ok(());
        }
        self.ctx.draw_round_rect(rect, radius, &fill(color))
    }

    /// A line `width` wide just inside `rect`'s edge.
    pub fn border(&mut self, rect: Rect, radius: f32, width: f32, color: Color) -> AureaResult<()> {
        if color.a == 0 || width <= 0.0 {
            return Ok(());
        }
        let half = width / 2.0;
        self.ctx.draw_round_rect(
            rect.inset(half),
            (radius - half).max(0.0),
            &stroke(color, width),
        )
    }

    /// A filled circle at `center`.
    pub fn circle(&mut self, center: Point, radius: f32, color: Color) -> AureaResult<()> {
        if color.a == 0 {
            return Ok(());
        }
        self.ctx.draw_circle(center, radius, &fill(color))
    }

    /// A straight line from `a` to `b`, `width` wide.
    pub fn line(&mut self, a: Point, b: Point, width: f32, color: Color) -> AureaResult<()> {
        if color.a == 0 || width <= 0.0 {
            return Ok(());
        }
        self.ctx
            .draw_line(a.x, a.y, b.x, b.y, &stroke(color, width))
    }

    /// A polyline through `points`, `width` wide, drawn from a path the state keeps so it allocates
    /// nothing once warm.
    pub fn polyline(&mut self, points: &[Point], width: f32, color: Color) -> AureaResult<()> {
        self.stroke_through(points.iter().copied(), width, color)
    }

    /// Like [`Self::polyline`], over points that are not in a slice: a stepped trace's corners,
    /// made as they are drawn.
    pub fn stroke_through(
        &mut self,
        points: impl IntoIterator<Item = Point>,
        width: f32,
        color: Color,
    ) -> AureaResult<()> {
        if color.a == 0 {
            return Ok(());
        }
        self.trace_path(points, false);
        self.ctx.draw_path(&self.state.path, &stroke(color, width))
    }

    /// The shape through `points`, closed and filled.
    pub fn fill_through(
        &mut self,
        points: impl IntoIterator<Item = Point>,
        color: Color,
    ) -> AureaResult<()> {
        if color.a == 0 {
            return Ok(());
        }
        self.trace_path(points, true);
        self.ctx.draw_path(&self.state.path, &fill(color))
    }

    fn trace_path(&mut self, points: impl IntoIterator<Item = Point>, closed: bool) {
        use crate::render::PathCommand;
        let path = &mut self.state.path;
        path.commands.clear();
        for (i, p) in points.into_iter().enumerate() {
            path.commands.push(if i == 0 {
                PathCommand::MoveTo(p)
            } else {
                PathCommand::LineTo(p)
            });
        }
        if closed && !path.commands.is_empty() {
            path.commands.push(PathCommand::Close);
        }
    }
}

fn flag(on: bool) -> f32 {
    f32::from(u8::from(on))
}

pub(crate) fn fill(color: Color) -> Paint {
    Paint::new().color(color).style(PaintStyle::Fill)
}

fn stroke(color: Color, width: f32) -> Paint {
    Paint::new()
        .color(color)
        .style(PaintStyle::Stroke)
        .stroke_width(width)
}

pub(crate) fn baseline_in(rect: Rect, m: &TextMetrics, font: &Font) -> f32 {
    let (ascent, descent) = if m.ascent > 0.0 {
        (m.ascent, m.descent)
    } else {
        (font.size * 0.8, font.size * 0.2)
    };
    rect.y + (rect.height - (ascent + descent)) / 2.0 + ascent
}

fn estimate(text: &str, font: &Font) -> TextMetrics {
    #[allow(clippy::cast_precision_loss)]
    let chars = text.chars().count() as f32;
    let width = chars * font.size * 0.55;
    TextMetrics {
        width,
        height: font.size,
        ascent: font.size * 0.8,
        descent: font.size * 0.2,
        advance: width,
    }
}
