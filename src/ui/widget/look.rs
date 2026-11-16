//! A widget's input taken and its look worked out, for the app to draw itself.

use crate::render::{Point, Rect};
use crate::ui::frame::{Eased, Response};

/// A widget's state for this frame, from its `look` call: the input is already taken, nothing is
/// drawn yet. Draw it any way, or with `paint_default` for the widget's own look.
///
/// `parts` is the widget's own: its label, value or what is selected, and the style it would draw
/// with.
///
/// ```rust,no_run
/// # use aurea::ui::{Button, Ui};
/// # use aurea::render::Color;
/// # fn f(ui: &mut Ui<'_, u8>) -> aurea::AureaResult<()> {
/// let at = aurea::Rect::new(0.0, 0.0, 120.0, 36.0);
/// let play = Button::new("Play").look(ui, 1, at);
/// let tint = Color::rgb(255, 255, 255).fade(0.15 + 0.1 * play.state.hover);
/// ui.round(play.rect, play.rect.height / 2.0, tint)?;
/// play.paint_label(ui)?;
/// if play.response.clicked {
///     // play
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Look<P> {
    /// Where to draw.
    pub rect: Rect,
    /// Hover, press, focus and enabled, eased as the widget's own look eases them.
    pub state: Eased,
    /// Where the press holding it landed, while a pointer holds it, for a ripple to spread from.
    pub held_at: Option<Point>,
    /// What the input did this frame, as `show` reports it.
    pub response: Response,
    /// The widget's own parts.
    pub parts: P,
}
