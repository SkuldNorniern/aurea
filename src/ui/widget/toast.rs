//! Short messages that come up over the canvas and go by themselves.

use std::time::{Duration, Instant};

#[cfg(feature = "accessibility")]
use accesskit::Live;

use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::card;
use crate::ui::frame::Ui;
use crate::ui::paint::{Component, Label, Overflow, Surface};
use crate::ui::state::{Channel, Sense, WidgetId};
use crate::ui::theme::Palette;
use crate::ui::widget::button::Button;

const FIRST_PART: u16 = 0xE800;
const PARTS: u64 = 0x400;
const SHOWN: Channel = Channel::Custom(0xE800);
const WIDEST: f32 = 420.0;
const MARGIN: f32 = 16.0;
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
    stay: Option<Duration>,
}

impl Toast {
    /// A message that stays four seconds, longer under the pointer.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: ToastTone::Info,
            action: None,
            stay: Some(STAY),
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
        self.stay = Some(stay);
        self
    }

    /// Stays until it is clicked away.
    #[must_use]
    pub fn sticky(mut self) -> Self {
        self.stay = None;
        self
    }
}

#[derive(Debug)]
struct Showing {
    handle: u64,
    toast: Toast,
    until: Option<Instant>,
    leaving: bool,
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
#[derive(Debug, Default)]
pub struct Toasts {
    next: u64,
    live: Vec<Showing>,
}

impl Toasts {
    /// No messages.
    pub fn new() -> Self {
        Self::default()
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

    /// Draws them along the canvas's bottom and gives the handle of one whose action was pressed.
    pub fn show<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, id: Id) -> AureaResult<Option<u64>> {
        let now = Instant::now();
        #[allow(clippy::cast_precision_loss)]
        let canvas = Rect::new(0.0, 0.0, ui.ctx.width() as f32, ui.ctx.height() as f32);
        let mut bottom = canvas.bottom() - MARGIN;
        let mut pressed = None;
        for live in self.live.iter_mut().rev() {
            let part = part_of(live.handle);
            let target = if live.leaving { 0.0 } else { 1.0 };
            let shown = ui.animate_from(id, part, SHOWN, 0.0, target, FADE);
            let (rect, lines) = place(ui, &live.toast, canvas, bottom);
            bottom = rect.y - MARGIN / 2.0;
            let input = ui.interact(id, part, rect, Sense::Hover, true);
            if input.hovered {
                live.until = None;
            }
            if input.clicked {
                live.leaving = true;
            }
            #[cfg(feature = "accessibility")]
            {
                ui.accessible_description(id, part, Role::Alert, &live.toast.text, None);
                ui.with_accessible(id, part, |node| node.set_live(Live::Polite));
            }
            if draw(ui, live, id, rect, lines, shown)? {
                pressed = Some(live.handle);
                live.leaving = true;
            }
            if let Some(stay) = live.toast.stay {
                let until = *live.until.get_or_insert(now + stay);
                if now >= until && !input.hovered {
                    live.leaving = true;
                } else if !live.leaving {
                    ui.wake_at(until);
                }
            }
        }
        self.live.retain(|live| {
            !live.leaving || {
                let part = part_of(live.handle);
                ui.animated(id, part, SHOWN).is_some_and(|s| s > 0.01)
            }
        });
        if pressed.is_some() {
            ui.mark_changed();
        }
        Ok(pressed)
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

fn place<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    toast: &Toast,
    canvas: Rect,
    bottom: f32,
) -> (Rect, f32) {
    let fonts = ui.fonts();
    let pad = ui.theme.padding;
    let line = fonts.text.size * 1.4;
    let widest = WIDEST.min(canvas.width - MARGIN * 2.0).max(40.0);
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
    let rect = Rect::new(
        canvas.center().x - width / 2.0,
        bottom - height,
        width,
        height,
    );
    (rect, lines)
}

/// True when the action was pressed.
fn draw<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    live: &Showing,
    id: Id,
    rect: Rect,
    lines: f32,
    shown: f32,
) -> AureaResult<bool> {
    let p = ui.theme.palette;
    let pad = ui.theme.padding;
    let line = ui.fonts().text.size * 1.4;
    let rect = Rect::new(
        rect.x,
        rect.y + (1.0 - shown) * 8.0,
        rect.width,
        rect.height,
    );
    card(ui.theme).fade(shown).paint(ui, rect)?;
    let strip = Rect::new(rect.x, rect.y, STRIP, rect.height).inset_xy(0.0, 4.0);
    Surface::new(live.toast.tone.color(&p).fade(shown))
        .radius(STRIP / 2.0)
        .paint(ui, strip)?;

    let mut text = rect.inset_xy(pad, 0.0);
    let mut pressed = false;
    if let Some(label) = &live.toast.action {
        let button = Button::new(label);
        let wide = button.width(ui);
        let (right, left) = text.split_right(wide, pad);
        text = left;
        // Buttons do not fade, so this one comes once the card is nearly in.
        if shown > 0.8 {
            let row = ui.theme.control_height;
            let at = right.centered(wide, row);
            let part = part_of(live.handle) + 1;
            pressed = button.part(part).show(ui, id, at)?.clicked;
        }
    }
    let tall = line * lines;
    let body = Rect::new(text.x, rect.center().y - tall / 2.0, text.width, tall);
    let overflow = if lines > 1.0 {
        Overflow::Wrap(line)
    } else {
        Overflow::Ellipsis
    };
    Label::new(&live.toast.text, p.text.fade(shown))
        .overflow(overflow)
        .paint(ui, body)?;
    Ok(pressed)
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
        let canvas = Rect::new(0.0, 0.0, 500.0, 300.0);
        let pad = rig.theme.padding;
        let toast = toasts.live[0].toast.clone();
        let (card, _) = rig.frame(|ui| place(ui, &toast, canvas, 300.0 - MARGIN));
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
}
