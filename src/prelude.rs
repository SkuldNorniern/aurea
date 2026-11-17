//! Glob-importable set of the types most Aurea apps need.
//!
//! Named imports read better as an app grows; these are for getting started. [`canvas`] and
//! [`native`] each bring one kind of widget, since both have a `Button`, a `Slider` and more.

pub use crate::window::{
    CursorGrabMode, CursorIcon, EventCallback, KeyCode, Modifiers, MouseButton, MouseButtons,
    PenPhase, PenSample, PenSampler, PenSampling, PenTool, Window, WindowEvent, WindowId,
    WindowType,
};

pub use crate::elements::{
    Button, Checkbox, ComboBox, Container, Divider, Element, ImageView, Label, Orientation,
    ProgressBar, SidebarList, Slider, Spacer, SplitOrientation, SplitView, Stack, TabBar,
    TextEditor, TextField, TextView,
};

pub use crate::input::keymap::{Chord, Keymap};
pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};

pub use crate::render::{
    Canvas, Color, DrawingContext, Paint, PaintStyle, Point, Rect, RendererBackend,
};

/// Plots and scope views.
pub use crate::ui::graph;

pub use crate::{AureaError, AureaResult};

pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};

/// What every app needs, whichever widgets it draws with.
mod core {
    pub use crate::input::keymap::{Chord, Keymap};
    pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};
    pub use crate::render::{
        Canvas, Color, DrawingContext, Paint, PaintStyle, Point, Rect, RendererBackend,
    };
    pub use crate::window::{
        CursorGrabMode, CursorIcon, EventCallback, KeyCode, Modifiers, MouseButton, MouseButtons,
        PenPhase, PenSample, PenSampler, PenSampling, PenTool, Window, WindowEvent, WindowId,
        WindowType,
    };
    pub use crate::{AureaError, AureaResult};
    pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};
}

/// Windows and drawing, with the canvas UI: [`crate::ui`]'s widgets, [`Ui`](crate::ui::Ui),
/// [`UiState`](crate::ui::UiState) and [`Theme`](crate::ui::Theme).
///
/// ```rust,no_run
/// use aurea::prelude::canvas::*;
/// # fn f(ui: &mut Ui<'_, u8>) -> AureaResult<()> {
/// if Button::new("Save").primary().show(ui, 1, Rect::new(0.0, 0.0, 90.0, 30.0))?.clicked {
///     // save
/// }
/// # Ok(())
/// # }
/// ```
pub mod canvas {
    pub use super::core::*;
    pub use crate::ui::*;
}

/// Windows and drawing, with the platform's own widgets from [`crate::elements`].
///
/// ```rust,no_run
/// use aurea::prelude::native::*;
/// # fn f() -> AureaResult<()> {
/// let mut window = Window::new("Notes", 640, 480)?;
/// let mut stack = Stack::new(Orientation::Vertical)?;
/// stack.add(TextField::new()?)?;
/// window.set_content(stack)?;
/// # Ok(())
/// # }
/// ```
pub mod native {
    pub use super::core::*;
    pub use crate::elements::*;
}
