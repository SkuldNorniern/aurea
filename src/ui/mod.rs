//! Widgets drawn on a canvas, styled by a [`Theme`] and the same on every
//! platform.
//!
//! They are immediate: each frame the draw callback says what is on screen
//! and where, and each widget answers what the input did to it. There is no
//! tree to keep in sync with the app's state, which stays the app's own.
//!
//! # Layers
//!
//! Each layer is made of the one under it, and every layer is public, so an
//! app builds at whichever height fits.
//!
//! 1. [`Ui`] itself: [`Ui::interact`] puts something on the hit list and
//!    reports the input on it, [`Ui::eased`] and [`Ui::animate`] keep values
//!    moving, and shape and text calls draw.
//! 2. [`paint`]: pieces that only draw. [`Surface`], [`Label`], [`Mark`].
//! 3. [`component`]: pieces that follow input. [`StateColors`],
//!    [`FocusRing`], [`Track`], [`Knob`], [`EditableText`].
//! 4. [`widget`]: [`Button`], [`Toggle`], [`Checkbox`], [`Slider`],
//!    [`Segmented`], [`ProgressBar`] and [`TextField`], each one
//!    [`Ui::interact`] call and a few components, with a style made from
//!    the theme that can be changed field by field.
//!
//! An app's own widget is the same shape as the built-in ones:
//!
//! ```rust,no_run
//! use aurea::ui::{Component, FocusRing, Label, Response, Sense, StateColors, Surface, Ui, Widget, WidgetId};
//! use aurea::{AureaResult, Rect};
//!
//! /// A card that lights up and can be picked.
//! struct Tile<'t> { title: &'t str, picked: bool }
//!
//! impl<Id: WidgetId> Widget<Id> for Tile<'_> {
//!     fn show(self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
//!         let input = ui.interact(id, 0, rect, Sense::Click, true);
//!         let state = ui.eased(id, 0, input);
//!         let p = ui.theme.palette;
//!         let face = StateColors::new(p.surface, p.hover, p.pressed).at(state);
//!         let edge = if self.picked { p.accent } else { p.border };
//!         FocusRing::new(ui.theme, 10.0, state.focus).paint(ui, rect)?;
//!         Surface::new(face).radius(10.0).border(1.5, edge).paint(ui, rect)?;
//!         Label::new(self.title, p.text).strong().paint(ui, rect.inset(12.0))?;
//!         Ok(Response::new(rect, input))
//!     }
//! }
//! ```
//!
//! # Wiring it to a canvas
//!
//! ```rust,no_run
//! use std::{cell::RefCell, rc::Rc};
//! use aurea::render::{Canvas, RendererBackend, request_canvas_redraw};
//! use aurea::ui::{Button, Slider, TextField, Theme, Toggle, UiState};
//! use aurea::{Rect, Window};
//!
//! #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
//! enum Id { Name, Volume, Muted, Save }
//!
//! #[derive(Default)]
//! struct App { name: String, volume: f32, muted: bool }
//!
//! # fn main() -> aurea::AureaResult<()> {
//! let window = Window::new("Settings", 360, 240)?;
//! let canvas = Canvas::new(360, 240, RendererBackend::Cpu)?;
//! let ui = Rc::new(RefCell::new(UiState::<Id>::new()));
//! ui.borrow_mut().set_reduced_motion(aurea::reduced_motion());
//! let app = Rc::new(RefCell::new(App::default()));
//! let theme = Theme::dark();
//! let id = canvas.id();
//!
//! let drawn = Rc::clone(&ui);
//! canvas.set_draw_callback(move |ctx| {
//!     let mut state = drawn.borrow_mut();
//!     let mut app = app.borrow_mut();
//!     {
//!         let mut ui = state.frame(ctx, &theme);
//!         let row = Rect::new(20.0, 20.0, 320.0, 30.0);
//!         TextField::new(&mut app.name).placeholder("Name").show(&mut ui, Id::Name, row)?;
//!         let row = Rect::new(20.0, 66.0, 320.0, 24.0);
//!         Slider::new(&mut app.volume, 0.0..=1.0).show(&mut ui, Id::Volume, row)?;
//!         let row = Rect::new(20.0, 106.0, 320.0, 28.0);
//!         Toggle::new(&mut app.muted).label("Muted").show(&mut ui, Id::Muted, row)?;
//!         let at = Rect::new(244.0, 190.0, 96.0, 30.0);
//!         if Button::new("Save").primary().show(&mut ui, Id::Save, at)?.clicked {
//!             // save app
//!         }
//!     }
//!     // Easing, a blinking caret or a click want the next frame too.
//!     if state.wants_frame() {
//!         request_canvas_redraw(id);
//!     }
//!     Ok(())
//! })?;
//!
//! window.on_event(move |event| {
//!     if ui.borrow_mut().handle(&event) {
//!         request_canvas_redraw(id);
//!     }
//! });
//! # Ok(())
//! # }
//! ```
//!
//! Keyboard: Tab and Shift+Tab move between widgets, Enter and Space press
//! buttons and switches, arrows move sliders, and text fields take the
//! usual editing keys, clipboard shortcuts and undo, with the platform's
//! own modifiers. [`UiState::sync_window`] keeps the pointer shape and an
//! input method's window following the widgets.

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
pub use widget::button::{Button, ButtonStyle, Tone};
pub use widget::toggle::{Checkbox, CheckboxStyle, Toggle, ToggleStyle};
pub use widget::slider::{Slider, SliderStyle};
