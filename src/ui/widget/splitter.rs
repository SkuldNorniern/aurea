//! A splitter: two panes and the line between them, dragged or stepped with the arrow keys.

use crate::AureaResult;
use crate::render::{Color, Rect};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::Role;
use crate::ui::component::FocusRing;
use crate::ui::frame::{Response, Ui};
use crate::ui::paint::Component;
use crate::ui::state::{Channel, Interaction, Sense, WidgetId};
use crate::ui::theme::Theme;
use crate::window::{CursorIcon, KeyCode, PointerKind};

const GRAB: Channel = Channel::Custom(0xED00);
const STEP: f32 = 8.0;

/// How a splitter looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitterStyle {
    /// The line at rest.
    pub line: Color,
    /// The line hovered or dragged.
    pub active: Color,
    /// How thick the line is, between the panes.
    pub thickness: f32,
    /// How wide a mouse can catch it.
    pub grab: f32,
    /// How wide a finger can catch it.
    pub touch_grab: f32,
}

impl SplitterStyle {
    /// Creates the default style from `theme`.
    pub fn new(theme: &Theme) -> Self {
        Self {
            line: theme.palette.border,
            active: theme.palette.accent,
            thickness: theme.border_width.max(1.0),
            grab: 8.0,
            touch_grab: 24.0,
        }
    }
}

/// Two panes side by side, or stacked. `at` is the first pane's size in pixels; keep it to keep the
/// layout.
#[derive(Debug)]
pub struct Splitter<'v> {
    at: &'v mut f32,
    stacked: bool,
    min: (f32, f32),
    label: &'v str,
    style: Option<SplitterStyle>,
}

impl<'v> Splitter<'v> {
    /// Creates a splitter with panes side by side, the first `at` pixels wide.
    pub fn new(at: &'v mut f32) -> Self {
        Self {
            at,
            stacked: false,
            min: (0.0, 0.0),
            label: "",
            style: None,
        }
    }

    /// One pane over the other.
    #[must_use]
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }

    /// The smallest each pane goes. When the rect cannot fit both, they share it by these.
    #[must_use]
    pub fn min(mut self, first: f32, second: f32) -> Self {
        self.min = (first.max(0.0), second.max(0.0));
        self
    }

    /// What a screen reader calls it.
    #[must_use]
    pub fn label(mut self, label: &'v str) -> Self {
        self.label = label;
        self
    }

    /// Sets the style.
    #[must_use]
    pub fn style(mut self, style: SplitterStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Draws the line in `rect` and gives the two panes' rects and the input.
    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<(Rect, Rect, Response)> {
        let style = self.style.unwrap_or_else(|| SplitterStyle::new(ui.theme));
        let length = if self.stacked {
            rect.height
        } else {
            rect.width
        };
        let room = (length - style.thickness).max(0.0);
        let before = *self.at;
        let mut at = settle(before, room, self.min);
        let grab = if ui.pointer_kind() == PointerKind::Touch {
            style.touch_grab
        } else {
            style.grab
        };
        let reach = line_rect(
            rect,
            self.stacked,
            at + style.thickness / 2.0 - grab / 2.0,
            grab,
        );
        let input = ui.interact(id, 0, reach, Sense::Adjust, true);

        at = moved(ui, id, input, rect, self.stacked, at, room);
        at = settle(at, room, self.min);
        *self.at = at;

        let eased = ui.eased(id, 0, input);
        if input.hovered || input.held {
            ui.set_cursor(if self.stacked {
                CursorIcon::ResizeVertical
            } else {
                CursorIcon::ResizeHorizontal
            });
        }
        #[cfg(feature = "accessibility")]
        ui.accessible_description(id, 0, Role::Splitter, self.label, None);
        let line = line_rect(rect, self.stacked, at, style.thickness);
        let lit = eased.hover.max(eased.press);
        ui.fill(line, style.line.mix(style.active, lit))?;
        FocusRing::new(ui.theme, 0.0, eased.focus).paint(ui, line)?;

        let changed = at != before;
        if changed {
            ui.mark_changed();
        }
        let mut response = Response::new(reach, input);
        response.changed = changed;
        let (first, second) = panes(rect, self.stacked, at, style.thickness);
        Ok((first, second, response))
    }
}

/// Where this frame's drag or keys put the line at `at`.
fn moved<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    id: Id,
    input: Interaction,
    rect: Rect,
    stacked: bool,
    mut at: f32,
    room: f32,
) -> f32 {
    let origin = start(rect, stacked);
    // Where in the line the press landed, so it does not jump to the pointer.
    if let Some(press) = ui.press(id, 0) {
        let off = along(press.at.x, press.at.y, stacked) - origin - at;
        ui.set_animated(id, 0, GRAB, off);
    }
    if input.held
        && let Some(p) = ui.pointer()
    {
        let off = ui.animated(id, 0, GRAB).unwrap_or(0.0);
        // Only setting it keeps it past this frame.
        ui.set_animated(id, 0, GRAB, off);
        at = along(p.x, p.y, stacked) - origin - off;
    }
    if !input.focused {
        return at;
    }
    let (back, ahead) = if stacked {
        (KeyCode::Up, KeyCode::Down)
    } else {
        (KeyCode::Left, KeyCode::Right)
    };
    for (key, _) in ui.keys() {
        match key {
            k if k == back => at -= STEP,
            k if k == ahead => at += STEP,
            KeyCode::Home => at = 0.0,
            KeyCode::End => at = room,
            _ => {}
        }
    }
    at
}

fn along(x: f32, y: f32, stacked: bool) -> f32 {
    if stacked { y } else { x }
}

fn start(rect: Rect, stacked: bool) -> f32 {
    along(rect.x, rect.y, stacked)
}

fn line_rect(rect: Rect, stacked: bool, at: f32, thickness: f32) -> Rect {
    if stacked {
        Rect::new(rect.x, rect.y + at, rect.width, thickness)
    } else {
        Rect::new(rect.x + at, rect.y, thickness, rect.height)
    }
}

fn panes(rect: Rect, stacked: bool, at: f32, thickness: f32) -> (Rect, Rect) {
    if stacked {
        rect.split_top(at, thickness)
    } else {
        rect.split_left(at, thickness)
    }
}

/// `at` kept inside `room`, each pane at its least; when both do not fit, shared by those.
fn settle(at: f32, room: f32, (first, second): (f32, f32)) -> f32 {
    let least = first + second;
    if least > room {
        return if least > 0.0 {
            room * first / least
        } else {
            0.0
        };
    }
    at.clamp(first, room - second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    const AREA: Rect = Rect::new(0.0, 0.0, 400.0, 200.0);

    fn show(rig: &mut Rig, at: &mut f32) -> (Rect, Rect, Response) {
        rig.frame(|ui| {
            Splitter::new(at)
                .min(50.0, 100.0)
                .show(ui, 1, AREA)
                .expect("draw")
        })
    }

    #[test]
    fn panes_fill_the_rect_around_the_line() {
        let mut rig = Rig::new(400, 200).still();
        let mut at = 120.0;
        let (first, second, _) = show(&mut rig, &mut at);
        let thick = SplitterStyle::new(&rig.theme).thickness;
        assert_eq!(first, Rect::new(0.0, 0.0, 120.0, 200.0));
        assert_eq!(second.x, 120.0 + thick);
        assert_eq!(second.right(), 400.0);
    }

    #[test]
    fn dragging_follows_the_pointer_and_stops_at_the_least() {
        let mut rig = Rig::new(400, 200).still();
        let mut at = 120.0;
        show(&mut rig, &mut at);
        rig.at(122, 50);
        show(&mut rig, &mut at);
        rig.button(122, 50, true);
        show(&mut rig, &mut at);
        rig.drag_to(202, 50);
        let (.., response) = show(&mut rig, &mut at);
        assert_eq!(at, 200.0, "the press's place in the line stays under it");
        assert!(response.changed);
        for x in [212, 230] {
            rig.drag_to(x, 50);
            show(&mut rig, &mut at);
            assert_eq!(at, x as f32 - 2.0, "and keeps there for the whole drag");
        }
        rig.drag_to(399, 50);
        show(&mut rig, &mut at);
        let thick = SplitterStyle::new(&rig.theme).thickness;
        assert_eq!(at, 400.0 - thick - 100.0);
        rig.drag_to(0, 50);
        show(&mut rig, &mut at);
        assert_eq!(at, 50.0);
    }

    #[test]
    fn arrow_keys_step_it_once_it_has_the_keyboard() {
        let mut rig = Rig::new(400, 200).still();
        let mut at = 120.0;
        show(&mut rig, &mut at);
        rig.key(KeyCode::Tab);
        show(&mut rig, &mut at);
        rig.key(KeyCode::Right);
        show(&mut rig, &mut at);
        assert_eq!(at, 128.0);
        rig.key(KeyCode::Home);
        show(&mut rig, &mut at);
        assert_eq!(at, 50.0);
    }

    #[test]
    fn stacked_splits_the_height() {
        let mut rig = Rig::new(400, 200).still();
        let mut at = 60.0;
        let (first, second, _) = rig.frame(|ui| {
            Splitter::new(&mut at)
                .stacked()
                .show(ui, 1, AREA)
                .expect("draw")
        });
        assert_eq!(first, Rect::new(0.0, 0.0, 400.0, 60.0));
        assert!(second.y > 60.0 && second.bottom() == 200.0);
    }

    #[test]
    fn too_small_a_rect_shares_it_by_the_least() {
        assert_eq!(settle(10.0, 30.0, (20.0, 40.0)), 10.0);
        assert_eq!(settle(90.0, 0.0, (0.0, 0.0)), 0.0);
        assert_eq!(settle(90.0, 100.0, (0.0, 20.0)), 80.0);
    }
}
