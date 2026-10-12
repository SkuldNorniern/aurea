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
    #[default]
    Start,
    Center,
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
    /// Its value changed this frame.
    pub changed: bool,
    /// It has the keyboard.
    pub focused: bool,
    /// Enter was pressed in a text field.
    pub submitted: bool,
    /// Escape was pressed in a text field, which let go of the keyboard.
    pub cancelled: bool,
}

impl Response {
    /// The response for `input` on `rect`, nothing changed yet.
    pub fn new(rect: Rect, input: Interaction) -> Self {
        Self {
            rect,
            hovered: input.hovered,
            held: input.held,
            clicked: input.clicked,
            changed: false,
            focused: input.focused,
            submitted: false,
            cancelled: false,
        }
    }
}

/// How far along hover, press and focus are for a widget, each `0..=1`, eased by the theme's
/// motion.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Eased {
    pub hover: f32,
    pub press: f32,
    /// The focus ring: only for focus that came from the keyboard.
    pub focus: f32,
    /// `1` enabled, faded towards the theme's disabled look when not.
    pub enabled: f32,
}

/// A press that landed on a widget this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressAt {
    pub at: Point,
    /// 1 for a click, 2 for a double click, and so on.
    pub clicks: u8,
    pub shift: bool,
}

/// One frame's drawing with widgets.
pub struct Ui<'a, Id: WidgetId> {
    /// The canvas, for drawing anything the widgets do not.
    pub ctx: &'a mut dyn DrawingContext,
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

    /// Puts `channel` of that part at `value` now, for something the user drags, which should
    /// follow the pointer exactly.
    pub fn set_animated(&mut self, id: Id, part: u16, channel: Channel, value: f32) {
        self.state.tweens.set((Key::new(id, part), channel), value);
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
                shift: press.shift,
            })
    }

    /// The keys pressed since the last frame, in order, for the widget that has the keyboard.
    pub fn keys(&self) -> impl Iterator<Item = (KeyCode, Modifiers)> + '_ {
        self.state.typed.iter().filter_map(|typed| match typed {
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

    /// Where the pointer is over the canvas.
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

    /// Seconds since the state was made, for looping effects.
    pub fn time(&self) -> f32 {
        self.state.tweens.time()
    }

    /// Asks for another frame after this one, for something the app animates itself.
    pub fn keep_animating(&mut self) {
        self.state.tweens.keep_busy();
    }

    /// Shows `text` under `response`'s rect while the pointer rests on it.
    pub fn tooltip(&mut self, response: &Response, text: &str) {
        if response.hovered && !response.held {
            self.state.tip.anchor = Some(response.rect);
            self.state.tip.text.clear();
            self.state.tip.text.push_str(text);
        }
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

    pub fn fill(&mut self, rect: Rect, color: Color) -> AureaResult<()> {
        if color.a == 0 {
            return Ok(());
        }
        self.ctx.draw_rect(rect, &fill(color))
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
        let paint = Paint::new()
            .color(color)
            .style(PaintStyle::Stroke)
            .stroke_width(width);
        self.ctx
            .draw_round_rect(rect.inset(half), (radius - half).max(0.0), &paint)
    }

    pub fn circle(&mut self, center: Point, radius: f32, color: Color) -> AureaResult<()> {
        if color.a == 0 {
            return Ok(());
        }
        self.ctx.draw_circle(center, radius, &fill(color))
    }

    /// A polyline through `points`, `width` wide, drawn from a path the state keeps so it allocates
    /// nothing once warm.
    pub fn polyline(&mut self, points: &[Point], width: f32, color: Color) -> AureaResult<()> {
        use crate::render::PathCommand;
        let path = &mut self.state.path;
        path.commands.clear();
        for (i, p) in points.iter().enumerate() {
            path.commands.push(if i == 0 {
                PathCommand::MoveTo(*p)
            } else {
                PathCommand::LineTo(*p)
            });
        }
        let paint = Paint::new()
            .color(color)
            .style(PaintStyle::Stroke)
            .stroke_width(width);
        self.ctx.draw_path(&self.state.path, &paint)
    }
}

fn flag(on: bool) -> f32 {
    f32::from(u8::from(on))
}

pub(crate) fn fill(color: Color) -> Paint {
    Paint::new().color(color).style(PaintStyle::Fill)
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
