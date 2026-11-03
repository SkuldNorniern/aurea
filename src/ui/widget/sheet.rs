//! A panel rising from the bottom edge, dismissed by a swipe down, a tap above it or Escape.

use crate::AureaResult;
use crate::render::Rect;
use crate::ui::component::card;
use crate::ui::frame::Ui;
use crate::ui::paint::Component;
use crate::ui::state::{Sense, WidgetId};

const BACKDROP: u16 = 0;
const PANEL: u16 = 1;
const GRIP: u16 = 2;
/// How much of the way to its place the sheet moves each frame.
const EASE: f32 = 0.3;
/// How tall the part that drags is.
const GRIP_HEIGHT: f32 = 28.0;
/// The share of its height a sheet is dragged down to be dismissed.
const DISMISS_AT: f32 = 0.3;
/// A flick this many pixels in one frame dismisses it however far it went.
const FLICK: f32 = 12.0;

/// Whether a [`BottomSheet`] is open and where it stands, kept by the app between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SheetState {
    /// Whether it is open or going away. Set it to open or dismiss from code.
    pub open: bool,
    /// How far up it has come, from 0 to 1.
    shown: f32,
    /// How far a finger has it pulled down, and how far the finger moved last frame.
    pulled: f32,
    flick: f32,
    /// Where the finger took hold.
    grabbed: Option<f32>,
}

impl SheetState {
    /// A sheet that starts open, rising from the edge.
    pub fn opened() -> Self {
        Self {
            open: true,
            ..Self::default()
        }
    }

    /// Whether anything of it still shows.
    pub fn visible(&self) -> bool {
        self.open || self.shown > 0.0
    }
}

/// A panel rising from the bottom edge. It keeps inside the draw context, so inside the safe area
/// on touch hosts, and takes the pointer from what is under it while it shows.
///
/// ```no_run
/// # use aurea::ui::{BottomSheet, SheetState, Ui};
/// # fn f(ui: &mut Ui<'_, u8>, state: &mut SheetState) -> aurea::AureaResult<()> {
/// BottomSheet::new(320.0).show(ui, state, 1, |ui, content| {
///     ui.fill(content, ui.theme.palette.surface)
/// })?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct BottomSheet {
    height: f32,
}

impl BottomSheet {
    /// A sheet `height` tall, or 90% of the canvas if that is less.
    pub fn new(height: f32) -> Self {
        Self { height }
    }

    /// Draws the sheet while it shows, with `content` given the area under its grip. `None` when
    /// it isn't showing. Dismissing sets `state.open` to false.
    pub fn show<Id: WidgetId, R>(
        self,
        ui: &mut Ui<'_, Id>,
        state: &mut SheetState,
        id: Id,
        content: impl FnOnce(&mut Ui<'_, Id>, Rect) -> AureaResult<R>,
    ) -> AureaResult<Option<R>> {
        if !state.visible() {
            return Ok(None);
        }
        let target = if state.open { 1.0 } else { 0.0 };
        state.shown += (target - state.shown) * EASE;
        if (target - state.shown).abs() < 0.002 {
            state.shown = target;
        } else {
            ui.keep_animating();
        }
        if !state.visible() {
            return Ok(None);
        }

        let canvas = Rect::new(0.0, 0.0, ui.ctx.width() as f32, ui.ctx.height() as f32);
        let tall = self.height.min(canvas.height * 0.9);
        let shade = ui.theme.palette.shadow;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let alpha = (f32::from(shade.a) * state.shown).round() as u8;
        ui.fill(canvas, shade.with_alpha(alpha))?;
        if ui
            .interact(id, BACKDROP, canvas, Sense::Click, true)
            .clicked
        {
            state.open = false;
        }
        ui.take_wheel();

        let top = canvas.bottom() - tall * state.shown + state.pulled;
        let rect = Rect::new(canvas.x, top, canvas.width, tall);
        ui.interact(id, PANEL, rect, Sense::Hover, true);
        let grip = Rect::new(rect.x, rect.y, rect.width, GRIP_HEIGHT);
        let held = ui.interact(id, GRIP, grip, Sense::Adjust, true).held;
        pull(ui, state, held, tall);
        if ui.state.escaped {
            state.open = false;
        }

        card(ui.theme).paint(ui, rect)?;
        let bar = Rect::new(rect.x + rect.width / 2.0 - 18.0, rect.y + 10.0, 36.0, 4.0);
        ui.fill(bar, ui.theme.palette.border)?;
        let inner = Rect::new(
            rect.x,
            rect.y + GRIP_HEIGHT,
            rect.width,
            (tall - GRIP_HEIGHT).max(0.0),
        );
        ui.clipped(inner, |ui| content(ui, inner)).map(Some)
    }
}

/// Follows a finger on the grip, and on letting go dismisses or settles back.
fn pull<Id: WidgetId>(ui: &mut Ui<'_, Id>, state: &mut SheetState, held: bool, tall: f32) {
    if !held {
        if state.grabbed.take().is_some()
            && (state.pulled > tall * DISMISS_AT || state.flick > FLICK)
        {
            state.open = false;
        }
        state.pulled = 0.0;
        state.flick = 0.0;
        return;
    }
    let Some(y) = ui.pointer().map(|at| at.y) else {
        return;
    };
    let grabbed = *state.grabbed.get_or_insert(y);
    let pulled = (y - grabbed).max(0.0);
    state.flick = pulled - state.pulled;
    state.pulled = pulled;
    ui.keep_animating();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    const OPEN: u8 = 7;

    fn draw(rig: &mut Rig, state: &mut SheetState) -> bool {
        rig.frame(|ui| {
            BottomSheet::new(200.0)
                .show(ui, state, OPEN, |_, _| Ok(()))
                .expect("draw")
                .is_some()
        })
    }

    fn settle(rig: &mut Rig, state: &mut SheetState) {
        for _ in 0..40 {
            draw(rig, state);
        }
    }

    #[test]
    fn it_shows_only_while_open_or_going_away() {
        let mut rig = Rig::new(400, 600);
        let mut state = SheetState::default();
        assert!(!draw(&mut rig, &mut state), "closed draws nothing");
        state.open = true;
        assert!(draw(&mut rig, &mut state));
        settle(&mut rig, &mut state);
        state.open = false;
        assert!(draw(&mut rig, &mut state), "it slides away first");
        settle(&mut rig, &mut state);
        assert!(!draw(&mut rig, &mut state));
    }

    #[test]
    fn a_tap_above_it_dismisses_and_a_tap_on_it_does_not() {
        let mut rig = Rig::new(400, 600);
        let mut state = SheetState::opened();
        settle(&mut rig, &mut state);
        rig.click(200.0, 550.0);
        settle(&mut rig, &mut state);
        assert!(state.open, "on the panel");
        rig.click(200.0, 100.0);
        settle(&mut rig, &mut state);
        assert!(!state.open, "above it");
    }

    #[test]
    fn a_long_pull_dismisses_and_a_short_one_settles_back() {
        let mut rig = Rig::new(400, 600);
        let mut state = SheetState::opened();
        settle(&mut rig, &mut state);
        // the panel's top is at y 400, and its grip just under that.
        rig.at(200.0, 410.0);
        draw(&mut rig, &mut state);
        rig.button(200.0, 410.0, true);
        draw(&mut rig, &mut state);
        for y in [412.0, 414.0, 416.0] {
            rig.drag_to(200.0, y);
            draw(&mut rig, &mut state);
        }
        rig.button(200.0, 416.0, false);
        settle(&mut rig, &mut state);
        assert!(state.open, "a short pull stays");

        rig.at(200.0, 410.0);
        draw(&mut rig, &mut state);
        rig.button(200.0, 410.0, true);
        draw(&mut rig, &mut state);
        for y in [430.0, 460.0, 500.0] {
            rig.drag_to(200.0, y);
            draw(&mut rig, &mut state);
        }
        rig.button(200.0, 500.0, false);
        settle(&mut rig, &mut state);
        assert!(!state.open, "a long one goes");
    }
}
