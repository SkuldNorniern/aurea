//! Widgets drawn on a canvas, styled by a [`Theme`] and the same on every platform.

pub mod component;
mod frame;
pub mod paint;
mod state;
pub mod text_edit;
mod theme;
pub mod widget;

pub use component::{EditableText, FocusRing, Knob, StateColors, TextOutcome, Track, card};
pub use frame::{Align, Eased, PressAt, Response, Ui};
pub use paint::{Border, Component, Label, Mark, MarkShape, Overflow, Shadow, Surface, TextSize};
pub use state::{Channel, Fonts, Interaction, Sense, UiState, WidgetId};
pub use theme::{Motion, Palette, Theme};
pub use widget::Widget;
