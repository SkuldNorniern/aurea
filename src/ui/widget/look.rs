//! What a widget hands an app's own painter, so the app draws any look over the widget's behaviour.

use crate::render::{Point, Rect};
use crate::ui::frame::Eased;

/// A widget's state for this frame, given to the painter of its `show_with`.
///
/// The widget has already taken the input; the painter only draws. `parts` is the widget's own:
/// its label, value or what is selected, and the style it would draw with.
///
/// ```rust,no_run
/// # use aurea::ui::{Button, Ui};
/// # use aurea::render::Color;
/// # fn f(ui: &mut Ui<'_, u8>) -> aurea::AureaResult<()> {
/// let at = aurea::Rect::new(0.0, 0.0, 120.0, 36.0);
/// Button::new("Glass").show_with(ui, 1, at, |ui, look| {
///     let lift = 0.08 + 0.1 * look.state.hover;
///     ui.round(look.rect, look.rect.height / 2.0, Color::rgba(255, 255, 255, (lift * 255.0) as u8))?;
///     look.paint_label(ui)
/// })?;
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
    /// The widget's own parts.
    pub parts: P,
}
