//! Glob-importable set of the types most Aurea apps need.
//!
//! ```rust,no_run
//! use aurea::prelude::*;
//!
//! fn main() -> AureaResult<()> {
//!     let mut window = Window::new("Hello", 400, 300)?;
//!     window.set_content(Label::new("Hello, Aurea!")?)?;
//!     window.run()?;
//!     Ok(())
//! }
//! ```

pub use crate::window::{
    CursorGrabMode, EventCallback, KeyCode, Modifiers, MouseButton, Window, WindowEvent,
    WindowId, WindowType,
};

pub use crate::elements::{
    Button, Checkbox, ComboBox, Container, Divider, Element, ImageView, Label, Orientation,
    ProgressBar, SidebarList, Slider, Spacer, SplitOrientation, SplitView, Stack, TabBar,
    TextEditor, TextField, TextView,
};

pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};

pub use crate::render::{Canvas, Color, DrawingContext, Point, Rect, RendererBackend};

pub use crate::{AureaError, AureaResult};

pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};
