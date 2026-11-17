//! Short messages that come up over the canvas and go by themselves.

use std::time::{Duration, Instant};

use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::accessibility::{Live, Role};
use crate::ui::component::card;
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::{Component, Label, Overflow, Surface};
use crate::ui::state::{Channel, Interaction, Sense, WidgetId};
use crate::ui::theme::Palette;
use crate::ui::widget::button::{Button, ButtonParts};
use crate::ui::widget::look::Look;

const FIRST_PART: u16 = 0xE800;
const PARTS: u64 = 0x400;
const SHOWN: Channel = Channel::Custom(0xE800);
const STRIP: f32 = 3.0;
const FADE: f32 = 6.0;
const STAY: Duration = Duration::from_secs(4);

/// What a toast is about, as the strip on its edge shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastTone {
    /// News.
    #[default]
    Info,
    /// Something finished.
    Success,
    /// Something to look at.
    Warning,
    /// Something failed.
    Danger,
}

impl ToastTone {
    fn color(self, p: &Palette) -> Color {
        match self {
            Self::Info => p.accent,
            Self::Success => p.success,
            Self::Warning => p.warning,
            Self::Danger => p.danger,
        }
    }
}

/// One message for [`Toasts`].
#[derive(Clone, Debug)]
pub struct Toast {
    text: String,
    tone: ToastTone,
    action: Option<String>,
    stay: Stay,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stay {
    /// As long as its [`Toasts`] say.
    Default,
    For(Duration),
    Sticky,
}

impl Toast {
    /// A message that stays as long as its [`Toasts`] say, four seconds by default, longer under
    /// the pointer.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: ToastTone::Info,
            action: None,
            stay: Stay::Default,
        }
    }

    /// Sets the tone.
    #[must_use]
    pub fn tone(mut self, tone: ToastTone) -> Self {
        self.tone = tone;
        self
    }

    /// A button in the toast, such as `Undo`; [`Toasts::show`] says when it is pressed.
    #[must_use]
    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }

    /// How long it stays.
    #[must_use]
    pub fn stay(mut self, stay: Duration) -> Self {
        self.stay = Stay::For(stay);
        self
    }

    /// Stays until it is clicked away.
    #[must_use]
    pub fn sticky(mut self) -> Self {
        self.stay = Stay::Sticky;
        self
    }
}

#[derive(Debug)]
struct Showing {
    handle: u64,
    toast: Toast,
    until: Option<Instant>,
    leaving: bool,
    /// Where it was last drawn, before its rise.
    rect: Rect,
    /// How far up it has come this frame, from 0 to 1.
    shown: f32,
    /// How many lines its text wraps to this frame.
    lines: f32,
}

/// The messages up now, kept by the app. A click takes one away.
///
/// ```rust,no_run
/// # use aurea::ui::{Toast, Toasts, Ui};
/// # fn f(ui: &mut Ui<'_, u8>, toasts: &mut Toasts) -> aurea::AureaResult<()> {
/// let undo = toasts.add(Toast::new("Layer deleted").action("Undo"));
/// // each frame, after everything but a dialog:
/// if toasts.show(ui, 9)? == Some(undo) {
///     // undo
/// }
/// # Ok(())
/// # }
/// ```
///
/// A desktop canvas that only draws on change schedules [`crate::ui::UiState::next_frame_at`] so
/// they leave on time; the iOS and Android hosts do it themselves.
#[derive(Debug)]
pub struct Toasts {
    next: u64,
    live: Vec<Showing>,
    widest: f32,
    margin: f32,
    stay: Duration,
    top: bool,
}

impl Default for Toasts {
    fn default() -> Self {
        Self {
            next: 0,
            live: Vec::new(),
            widest: 420.0,
            margin: 16.0,
            stay: STAY,
            top: false,
        }
    }
}

impl Toasts {
    /// No messages.
    pub fn new() -> Self {
        Self::default()
    }

    /// How wide a toast goes before its text wraps.
    #[must_use]
    pub fn widest(mut self, widest: f32) -> Self {
        self.widest = widest.max(40.0);
        self
    }

    /// The space between the canvas's edge and the toasts, and half that between toasts.
    #[must_use]
    pub fn margin(mut self, margin: f32) -> Self {
        self.margin = margin.max(0.0);
        self
    }

    /// How long a toast stays when it does not say; four seconds by default.
    #[must_use]
    pub fn stay(mut self, stay: Duration) -> Self {
        self.stay = stay;
        self
    }

    /// Stacks them down from the canvas's top, the newest at the top, instead of up from its
    /// bottom.
    #[must_use]
    pub fn top(mut self) -> Self {
        self.top = true;
        self
    }

    /// Puts `toast` up, the newest at the bottom, and gives its handle.
    pub fn add(&mut self, toast: impl Into<Toast>) -> u64 {
        let handle = self.next;
        self.next += 1;
        self.live.push(Showing {
            handle,
            toast: toast.into(),
            until: None,
            leaving: false,
            rect: Rect::default(),
            shown: 0.0,
            lines: 1.0,
        });
        handle
    }

    /// Takes the toast `handle` down.
    pub fn dismiss(&mut self, handle: u64) {
        for live in &mut self.live {
            if live.handle == handle {
                live.leaving = true;
            }
        }
    }

    /// Whether any are up.
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// Takes the input for each toast and works out what each would draw, without drawing: the app
    /// draws them, or [`Look::paint_default`] does, newest at the bottom. Then [`Look::pressed`]
    /// gives the handle of one whose action was pressed, and [`Toasts::end`] takes it down and
    /// lets go the ones that have faded out. [`Toasts::show`] does all of it.
    ///
    /// ```rust,no_run
    /// # use aurea::ui::{Toasts, Ui};
    /// # fn f(ui: &mut Ui<'_, u8>, toasts: &mut Toasts) {
    /// let look = toasts.look(ui, 9);
    /// for toast in &look.parts.toasts {
    ///     // draw toast.rect, toast.text and toast.action in the app's own style
    /// }
    /// let pressed = look.pressed();
    /// toasts.end(ui, 9, pressed);
    /// # }
    /// ```
    pub fn look<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, id: Id) -> Look<ToastsParts<'_>> {
        let now = Instant::now();
        #[allow(clippy::cast_precision_loss)]
        let canvas = Rect::new(0.0, 0.0, ui.ctx.width() as f32, ui.ctx.height() as f32);
        let margin = self.margin;
        let mut edge = if self.top {
            canvas.y + margin
        } else {
            canvas.bottom() - margin
        };
        for live in self.live.iter_mut().rev() {
            let part = part_of(live.handle);
            let target = if live.leaving { 0.0 } else { 1.0 };
            live.shown = ui.animate_from(id, part, SHOWN, 0.0, target, FADE);
            let (size, lines) = measure(ui, &live.toast, canvas, self.widest, margin);
            live.lines = lines;
            let rect = stack(canvas, size, &mut edge, margin, self.top);
            live.rect = rect;
            let input = ui.interact(id, part, rect, Sense::Hover, true);
            if input.hovered {
                live.until = None;
            }
            if input.clicked {
                live.leaving = true;
            }
            {
                ui.accessible_description(id, part, Role::Alert, &live.toast.text, None);
                ui.with_accessible(id, part, |node| node.set_live(Live::Polite));
            }
            expire(ui, live, self.stay, now, input.hovered);
        }
        let this: &Toasts = self;
        let mut toasts = Vec::with_capacity(this.live.len());
        for live in this.live.iter().rev() {
            toasts.push(live.look(ui, id));
        }
        Look {
            rect: canvas,
            state: Eased {
                enabled: 1.0,
                ..Eased::default()
            },
            held_at: None,
            response: Response::new(canvas, Interaction::default()),
            parts: ToastsParts { toasts },
        }
    }

    /// Takes down the toast `pressed` names, if any, and lets go the ones that have faded out.
    /// Call it after the frame's toasts are drawn, with the `pressed` of their look, as
    /// [`Toasts::show`] does.
    pub fn end<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, id: Id, pressed: Option<u64>) {
        if let Some(handle) = pressed {
            self.dismiss(handle);
            ui.mark_changed();
        }
        self.live.retain(|live| {
            !live.leaving || {
                let part = part_of(live.handle);
                ui.animated(id, part, SHOWN).is_some_and(|s| s > 0.01)
            }
        });
    }

    /// Draws them along the canvas's bottom and gives the handle of one whose action was pressed.
    pub fn show<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, id: Id) -> AureaResult<Option<u64>> {
        let look = self.look(ui, id);
        look.paint_default(ui)?;
        let pressed = look.pressed();
        self.end(ui, id, pressed);
        Ok(pressed)
    }
}

/// One live toast as [`Toasts::look`] works it out: where it is drawn, its tone and text, and
/// its action button's look while it is up enough to take a press.
pub struct ToastLook<'t> {
    /// The toast's handle, as [`Toasts::add`] gave it.
    pub handle: u64,
    /// Where it is drawn this frame, nudged down while it rises.
    pub rect: Rect,
    /// How far up it has come, from 0 to 1; the card and text fade with it.
    pub shown: f32,
    /// What it is about, for the strip on its edge.
    pub tone: ToastTone,
    /// The message.
    pub text: &'t str,
    /// Where the message is drawn, inside the card and left of the action.
    pub text_at: Rect,
    /// How many lines the message wraps to; more than one wraps, one ellipsizes.
    pub lines: f32,
    /// The action button's look, once the toast is up enough for a press; `None` otherwise, and
    /// when the toast has no action.
    pub action: Option<Look<ButtonParts<'t>>>,
}

/// What [`Toasts::look`] works out for the frame, one [`ToastLook`] for each live toast, newest
/// first.
pub struct ToastsParts<'t> {
    /// The live toasts, the newest first; each is drawn in turn.
    pub toasts: Vec<ToastLook<'t>>,
}

impl ToastLook<'_> {
    /// The card, faded in as the toast rises.
    pub fn paint_card<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        card(ui.theme).fade(self.shown).paint(ui, self.rect)
    }

    /// The strip on the card's left edge, in the tone's colour.
    pub fn paint_strip<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let strip = Rect::new(self.rect.x, self.rect.y, STRIP, self.rect.height).inset_xy(0.0, 4.0);
        Surface::new(self.tone.color(&ui.theme.palette).fade(self.shown))
            .radius(STRIP / 2.0)
            .paint(ui, strip)
    }

    /// The action button, when it has a look.
    pub fn paint_action<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        match &self.action {
            Some(action) => action.paint_default(ui),
            None => Ok(()),
        }
    }

    /// The message, in the text colour faded with the card.
    pub fn paint_text<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let line = ui.fonts().text.size * 1.4;
        let overflow = if self.lines > 1.0 {
            Overflow::Wrap(line)
        } else {
            Overflow::Ellipsis
        };
        Label::new(self.text, ui.theme.palette.text.fade(self.shown))
            .overflow(overflow)
            .paint(ui, self.text_at)
    }

    /// The card, the strip, the action and the message, in that order.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_card(ui)?;
        self.paint_strip(ui)?;
        self.paint_action(ui)?;
        self.paint_text(ui)
    }
}

impl Look<ToastsParts<'_>> {
    /// Draws every toast, as [`Toasts::show`] does, newest first.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        for toast in &self.parts.toasts {
            toast.paint_default(ui)?;
        }
        Ok(())
    }

    /// The handle of the toast whose action was pressed this frame, if one was.
    pub fn pressed(&self) -> Option<u64> {
        let mut pressed = None;
        for toast in &self.parts.toasts {
            if toast.action.is_some_and(|action| action.response.clicked) {
                pressed = Some(toast.handle);
            }
        }
        pressed
    }
}

impl Showing {
    /// The toast's look this frame, from the rise and place [`Toasts::look`] gave it.
    fn look<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, id: Id) -> ToastLook<'_> {
        let part = part_of(self.handle);
        let pad = ui.theme.padding;
        let rect = Rect::new(
            self.rect.x,
            self.rect.y + (1.0 - self.shown) * 8.0,
            self.rect.width,
            self.rect.height,
        );
        let mut text = rect.inset_xy(pad, 0.0);
        let mut action = None;
        if let Some(label) = &self.toast.action {
            let button = Button::new(label);
            let wide = button.width(ui);
            let (right, left) = text.split_right(wide, pad);
            text = left;
            // Buttons do not fade, so this one comes once the card is nearly in.
            if self.shown > 0.8 {
                let row = ui.theme.control_height;
                let at = right.centered(wide, row);
                action = Some(button.part(part + 1).look(ui, id, at));
            }
        }
        let tall = ui.fonts().text.size * 1.4 * self.lines;
        let body = Rect::new(text.x, rect.center().y - tall / 2.0, text.width, tall);
        ToastLook {
            handle: self.handle,
            rect,
            shown: self.shown,
            tone: self.toast.tone,
            text: self.toast.text.as_str(),
            text_at: body,
            lines: self.lines,
            action,
        }
    }
}

impl From<&str> for Toast {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Toast {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

fn part_of(handle: u64) -> u16 {
    FIRST_PART + u16::try_from(handle % PARTS).unwrap_or(0) * 2
}

/// Where a toast of `size` goes at `edge`, which moves past it for the next.
fn stack(canvas: Rect, size: (f32, f32), edge: &mut f32, margin: f32, top: bool) -> Rect {
    let x = canvas.center().x - size.0 / 2.0;
    if top {
        let rect = Rect::new(x, *edge, size.0, size.1);
        *edge = rect.bottom() + margin / 2.0;
        rect
    } else {
        let rect = Rect::new(x, *edge - size.1, size.0, size.1);
        *edge = rect.y - margin / 2.0;
        rect
    }
}

/// Sends `live` off once its time is up, or asks to wake then.
fn expire<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    live: &mut Showing,
    default: Duration,
    now: Instant,
    hovered: bool,
) {
    let stay = match live.toast.stay {
        Stay::Default => default,
        Stay::For(stay) => stay,
        Stay::Sticky => return,
    };
    let until = *live.until.get_or_insert(now + stay);
    if now >= until && !hovered {
        live.leaving = true;
    } else if !live.leaving {
        ui.wake_at(until);
    }
}

/// The toast's width and height, and its lines of text.
fn measure<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    toast: &Toast,
    canvas: Rect,
    widest: f32,
    margin: f32,
) -> ((f32, f32), f32) {
    let fonts = ui.fonts();
    let pad = ui.theme.padding;
    let line = fonts.text.size * 1.4;
    let widest = widest.min(canvas.width - margin * 2.0).max(40.0);
    let button = match &toast.action {
        Some(label) => Button::new(label).width(ui) + pad,
        None => 0.0,
    };
    let room = (widest - pad * 2.0 - button).max(1.0);
    let full = ui.text_width(&toast.text, &fonts.text);
    let lines = if full <= room {
        1.0
    } else {
        (full / (room * 0.9)).ceil()
    };
    let width = full.min(room) + pad * 2.0 + button;
    let height = (line * lines + pad).max(ui.theme.control_height + pad / 2.0);
    ((width, height), lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    fn show(rig: &mut Rig, toasts: &mut Toasts) -> Option<u64> {
        rig.frame(|ui| toasts.show(ui, 1).expect("draw"))
    }

    fn settled() -> Rig {
        Rig::new(500, 300).still()
    }

    #[test]
    fn it_leaves_when_its_time_is_up_and_not_before() {
        let mut rig = settled();
        let mut toasts = Toasts::new();
        toasts.add(Toast::new("Saved").stay(Duration::from_secs(60)));
        toasts.add(Toast::new("Gone").stay(Duration::ZERO));
        show(&mut rig, &mut toasts);
        show(&mut rig, &mut toasts);
        show(&mut rig, &mut toasts);
        assert_eq!(toasts.live.len(), 1);
        assert_eq!(toasts.live[0].toast.text, "Saved");
    }

    #[test]
    fn the_toasts_default_stay_is_for_those_that_set_none() {
        let mut rig = settled();
        let mut toasts = Toasts::new().stay(Duration::ZERO);
        toasts.add("Gone");
        toasts.add(Toast::new("Kept").stay(Duration::from_secs(60)));
        toasts.add(Toast::new("Pinned").sticky());
        show(&mut rig, &mut toasts);
        show(&mut rig, &mut toasts);
        show(&mut rig, &mut toasts);
        let left: Vec<_> = toasts.live.iter().map(|l| l.toast.text.as_str()).collect();
        assert_eq!(left, ["Kept", "Pinned"]);
    }

    #[test]
    fn top_stacks_them_down_from_the_top_within_the_margin_and_width() {
        let mut rig = settled();
        let mut toasts = Toasts::new().top().margin(10.0).widest(120.0);
        toasts.add(Toast::new("Older").sticky());
        toasts.add(Toast::new("Newer and longer than the width allows").sticky());
        show(&mut rig, &mut toasts);
        let (older, newer) = (toasts.live[0].rect, toasts.live[1].rect);
        assert_eq!(newer.y, 10.0, "the newest at the top");
        assert_eq!(older.y, newer.bottom() + 5.0);
        assert!(newer.width <= 120.0 + 0.01, "{}", newer.width);
    }

    #[test]
    fn its_time_asks_for_a_frame() {
        let mut rig = settled();
        let mut toasts = Toasts::new();
        assert!(rig.state.next_frame_at().is_none());
        toasts.add(Toast::new("Saved").stay(Duration::from_secs(60)));
        show(&mut rig, &mut toasts);
        let due = rig.state.next_frame_at().expect("a wake");
        assert!(due > Instant::now() + Duration::from_secs(50));
    }

    #[test]
    fn a_sticky_one_stays_until_clicked() {
        let mut rig = settled();
        let mut toasts = Toasts::new();
        toasts.add(Toast::new("Read me").sticky());
        show(&mut rig, &mut toasts);
        assert!(rig.state.next_frame_at().is_none());
        show(&mut rig, &mut toasts);
        assert!(!toasts.is_empty());
        rig.click(250, 260);
        show(&mut rig, &mut toasts);
        show(&mut rig, &mut toasts);
        assert!(toasts.is_empty());
    }

    #[test]
    fn its_action_reports_the_handle_and_takes_it_down() {
        let mut rig = settled();
        let mut toasts = Toasts::new();
        let undo = toasts.add(Toast::new("Deleted").action("Undo").sticky());
        show(&mut rig, &mut toasts);
        show(&mut rig, &mut toasts);
        let pad = rig.theme.padding;
        let card = toasts.live[0].rect;
        show(&mut rig, &mut toasts);
        rig.click(card.right() - pad - 4.0, card.center().y);
        let seen = show(&mut rig, &mut toasts);
        assert_eq!(seen, Some(undo));
    }

    #[test]
    fn the_newest_is_at_the_bottom_and_nothing_under_gets_the_click() {
        let mut rig = settled();
        let mut toasts = Toasts::new();
        toasts.add(Toast::new("Older").sticky());
        toasts.add(Toast::new("Newer").sticky());
        let under = |rig: &mut Rig, toasts: &mut Toasts| {
            rig.frame(|ui| {
                let clicked = Button::new("Under")
                    .show(ui, 2, Rect::new(0.0, 0.0, 500.0, 300.0))
                    .expect("draw")
                    .clicked;
                toasts.show(ui, 1).expect("draw");
                clicked
            })
        };
        under(&mut rig, &mut toasts);
        under(&mut rig, &mut toasts);
        rig.click(250, 260);
        assert!(!under(&mut rig, &mut toasts));
        under(&mut rig, &mut toasts);
        assert_eq!(toasts.live.len(), 1);
        assert_eq!(toasts.live[0].toast.text, "Older");
    }

    #[test]
    fn dismiss_takes_one_down() {
        let mut rig = settled();
        let mut toasts = Toasts::new();
        let handle = toasts.add(Toast::new("Bye").sticky());
        show(&mut rig, &mut toasts);
        toasts.dismiss(handle);
        show(&mut rig, &mut toasts);
        show(&mut rig, &mut toasts);
        assert!(toasts.is_empty());
    }

    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn the_app_draws_its_own_toast_and_the_action_still_comes_back() {
        let mut rig = settled();
        let mut toasts = Toasts::new();
        let undo = toasts.add(Toast::new("Deleted").action("Undo").sticky());
        let draw = |rig: &mut Rig, toasts: &mut Toasts| {
            rig.frame(|ui| {
                let look = toasts.look(ui, 1);
                let toast = &look.parts.toasts[0];
                let (card, action) = (toast.rect, toast.action.map(|a| a.rect));
                ui.fill(card, Color::rgb(0, 200, 0)).expect("card");
                let pressed = look.pressed();
                toasts.end(ui, 1, pressed);
                (card, action, pressed)
            })
        };
        draw(&mut rig, &mut toasts);
        draw(&mut rig, &mut toasts);
        let (card, action, _) = draw(&mut rig, &mut toasts);
        assert_eq!(
            rig.pixel((card.x + 1.0) as u32, card.center().y as u32),
            [0, 200, 0, 255],
            "the app's card"
        );
        let action = action.expect("the action is up");
        rig.click(action.center().x, action.center().y);
        let (.., pressed) = draw(&mut rig, &mut toasts);
        assert_eq!(pressed, Some(undo));
    }
}
