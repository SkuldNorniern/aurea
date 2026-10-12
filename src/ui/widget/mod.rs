//! The top layer: widgets, each a few components and one call to
//! [`Ui::interact`].
//!
//! Every widget takes a style, made from the theme by default and changed
//! field by field when one widget should look different. A widget of the
//! app's own implements [`Widget`] and is shown the same way, with
//! [`Ui::add`].

pub mod button;
pub mod dock;
pub mod progress;
pub mod segmented;
pub mod slider;
pub mod text_field;
pub mod toggle;
pub(crate) mod tooltip;

use super::frame::{Response, Ui};
use super::state::WidgetId;
use crate::AureaResult;
use crate::render::Rect;

/// Something drawn at a rect that answers input.
pub trait Widget<Id: WidgetId> {
    fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response>;
}

impl<Id: WidgetId> Ui<'_, Id> {
    /// Shows `widget` as `id` at `rect`.
    pub fn add(&mut self, id: Id, rect: Rect, widget: impl Widget<Id>) -> AureaResult<Response> {
        widget.show(self, id, rect)
    }
}
