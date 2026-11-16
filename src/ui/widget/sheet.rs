//! A panel rising from the bottom edge, dismissed by a swipe down, a tap above it or Escape.

use crate::AureaResult;
use crate::render::{Color, Rect};
use crate::ui::component::card;
use crate::ui::frame::{Eased, Response, Ui};
use crate::ui::paint::Component;
use crate::ui::state::{Sense, WidgetId};
use crate::ui::widget::look::Look;

const BACKDROP: u16 = 0;
const PANEL: u16 = 1;
const GRIP: u16 = 2;

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
    ease: f32,
    grip: f32,
    dismiss_at: f32,
    flick: f32,
}

impl BottomSheet {
    /// A sheet `height` tall, or 90% of the canvas if that is less.
    pub fn new(height: f32) -> Self {
        Self {
            height,
            ease: 0.3,
            grip: 28.0,
            dismiss_at: 0.3,
            flick: 12.0,
        }
    }

    /// How much of the way to its place it moves each frame, from just above 0 to 1 (at once);
    /// 0.3 by default.
    #[must_use]
    pub fn ease(mut self, share: f32) -> Self {
        self.ease = share.clamp(0.01, 1.0);
        self
    }

    /// How tall the grip along its top is, the part a finger drags; 28 by default.
    #[must_use]
    pub fn grip_height(mut self, height: f32) -> Self {
        self.grip = height.max(0.0);
        self
    }

    /// The share of its height it is pulled down to be dismissed when let go; 0.3 by default.
    #[must_use]
    pub fn dismiss_at(mut self, share: f32) -> Self {
        self.dismiss_at = share.clamp(0.0, 1.0);
        self
    }

    /// How many pixels down in one frame count as a flick, which dismisses it however far it
    /// went; 12 by default.
    #[must_use]
    pub fn flick(mut self, pixels: f32) -> Self {
        self.flick = pixels.max(0.0);
        self
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
        let Some(look) = self.look(ui, state, id) else {
            return Ok(None);
        };
        look.paint_default(ui)?;
        // the content runs inside its clip, so it is the one step the library runs for the app
        let inner = look.parts.content;
        ui.clipped(inner, |ui| content(ui, inner)).map(Some)
    }

    /// Takes the input for the sheet and works out its chrome, without drawing: the backdrop
    /// shade, the panel, the grip and the content area. `None` when it isn't showing. The app
    /// draws the chrome, or [`Look::paint_default`] does, then its content in `parts.content`.
    /// Dismissing sets `state.open` to false, as [`BottomSheet::show`] does.
    ///
    /// ```rust,no_run
    /// # use aurea::ui::{BottomSheet, SheetState, Ui};
    /// # fn f(ui: &mut Ui<'_, u8>, state: &mut SheetState) -> aurea::AureaResult<()> {
    /// if let Some(sheet) = BottomSheet::new(320.0).look(ui, state, 1) {
    ///     sheet.paint_default(ui)?;
    ///     ui.fill(sheet.parts.content, ui.theme.palette.surface)?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn look<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        state: &mut SheetState,
        id: Id,
    ) -> Option<Look<SheetParts>> {
        if !state.visible() {
            return None;
        }
        let target = if state.open { 1.0 } else { 0.0 };
        state.shown += (target - state.shown) * self.ease;
        if (target - state.shown).abs() < 0.002 {
            state.shown = target;
        } else {
            ui.keep_animating();
        }
        if !state.visible() {
            return None;
        }

        let canvas = Rect::new(0.0, 0.0, ui.ctx.width() as f32, ui.ctx.height() as f32);
        let tall = self.height.min(canvas.height * 0.9);
        let shade = ui.theme.palette.shadow;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let alpha = (f32::from(shade.a) * state.shown).round() as u8;
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
        let grip = Rect::new(rect.x, rect.y, rect.width, self.grip);
        let input = ui.interact(id, GRIP, grip, Sense::Adjust, true);
        self.pull(ui, state, input.held, tall);
        if ui.state.escaped {
            state.open = false;
        }

        let inner = Rect::new(
            rect.x,
            rect.y + self.grip,
            rect.width,
            (tall - self.grip).max(0.0),
        );
        Some(Look {
            rect,
            state: Eased {
                enabled: 1.0,
                ..Eased::default()
            },
            held_at: ui.held_at(id, GRIP),
            response: Response::new(rect, input),
            parts: SheetParts {
                backdrop: canvas,
                shade: shade.with_alpha(alpha),
                grip,
                content: inner,
            },
        })
    }

    /// Follows a finger on the grip, and on letting go dismisses or settles back.
    fn pull<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        state: &mut SheetState,
        held: bool,
        tall: f32,
    ) {
        if !held {
            if state.grabbed.take().is_some()
                && (state.pulled > tall * self.dismiss_at || state.flick > self.flick)
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
}

/// What a [`BottomSheet`] draws, from [`BottomSheet::look`]: the shade over the canvas, the panel
/// with its grip, and the content area under the grip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetParts {
    /// The whole canvas, which the shade covers.
    pub backdrop: Rect,
    /// The shade's colour, faded as the sheet rises.
    pub shade: Color,
    /// The grip along the panel's top, the part a finger drags.
    pub grip: Rect,
    /// The area under the grip, where the content goes; the app clips it to this.
    pub content: Rect,
}

impl Look<SheetParts> {
    /// The shade, the panel and the grip's bar, in that order, as [`BottomSheet::show`] draws
    /// them around its content.
    pub fn paint_default<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        self.paint_backdrop(ui)?;
        self.paint_panel(ui)?;
        self.paint_grip(ui)
    }

    /// The shade over the canvas.
    pub fn paint_backdrop<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        ui.fill(self.parts.backdrop, self.parts.shade)
    }

    /// The panel's card.
    pub fn paint_panel<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        card(ui.theme).paint(ui, self.rect)
    }

    /// The bar that shows the grip, centred near the panel's top.
    pub fn paint_grip<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>) -> AureaResult<()> {
        let rect = self.rect;
        let bar = Rect::new(rect.x + rect.width / 2.0 - 18.0, rect.y + 10.0, 36.0, 4.0);
        ui.fill(bar, ui.theme.palette.border)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rig::Rig;

    const OPEN: u8 = 7;

    fn draw(rig: &mut Rig, state: &mut SheetState) -> bool {
        draw_with(rig, state, BottomSheet::new(200.0))
    }

    fn draw_with(rig: &mut Rig, state: &mut SheetState, sheet: BottomSheet) -> bool {
        rig.frame(|ui| {
            sheet
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

    #[test]
    fn a_lower_dismiss_share_lets_a_short_slow_pull_go() {
        let mut rig = Rig::new(400, 600);
        let mut state = SheetState::opened();
        let sheet = BottomSheet::new(200.0).dismiss_at(0.02).ease(1.0);
        draw_with(&mut rig, &mut state, sheet);
        rig.at(200.0, 410.0);
        draw_with(&mut rig, &mut state, sheet);
        rig.button(200.0, 410.0, true);
        draw_with(&mut rig, &mut state, sheet);
        for y in [412.0, 414.0, 416.0] {
            rig.drag_to(200.0, y);
            draw_with(&mut rig, &mut state, sheet);
        }
        rig.button(200.0, 416.0, false);
        draw_with(&mut rig, &mut state, sheet);
        assert!(!state.open, "6 of 200 is past 2%");
    }

    #[test]
    fn ease_one_puts_it_in_place_at_once_and_the_grip_sets_where_content_starts() {
        let mut rig = Rig::new(400, 600);
        let mut state = SheetState::opened();
        let content = rig.frame(|ui| {
            BottomSheet::new(200.0)
                .ease(1.0)
                .grip_height(50.0)
                .show(ui, &mut state, OPEN, |_, inner| Ok(inner))
                .expect("draw")
        });
        assert_eq!(content, Some(Rect::new(0.0, 450.0, 400.0, 150.0)));
    }

    #[test]
    fn the_app_draws_its_own_panel_and_a_tap_above_still_dismisses() {
        let mut rig = Rig::new(400, 600);
        let mut state = SheetState::opened();
        let draw = |rig: &mut Rig, state: &mut SheetState| {
            rig.frame(|ui| {
                if let Some(sheet) = BottomSheet::new(200.0).look(ui, state, OPEN) {
                    ui.fill(sheet.parts.content, Color::rgb(0, 200, 0))
                        .expect("content");
                }
            })
        };
        for _ in 0..40 {
            draw(&mut rig, &mut state);
        }
        assert_eq!(rig.pixel(200, 450), [0, 200, 0, 255], "the app's content");
        rig.click(200.0, 100.0);
        for _ in 0..40 {
            draw(&mut rig, &mut state);
        }
        assert!(!state.open, "above it");
    }
}
