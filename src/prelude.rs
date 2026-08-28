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
//!
//! Drawing types come along, so a canvas app needs no second import:
//!
//! ```rust,no_run
//! use aurea::prelude::*;
//!
//! # fn main() -> AureaResult<()> {
//! let canvas = Canvas::new(400, 300, RendererBackend::Cpu)?;
//! let (mut chart, signal) = graph::quick::live("cpu", 600);
//! chart.push(signal, 0.42);
//! canvas.set_draw_callback(move |ctx| {
//!     let area = Rect::new(0.0, 0.0, ctx.width() as f32, ctx.height() as f32);
//!     let _ = area;
//!     Ok(())
//! })?;
//! # Ok(())
//! # }
//! ```

pub use crate::window::{
    CursorGrabMode, EventCallback, KeyCode, Modifiers, MouseButton, Window, WindowEvent, WindowId,
    WindowType,
};

pub use crate::elements::{
    Button, Checkbox, ComboBox, Container, Divider, Element, ImageView, Label, Orientation,
    ProgressBar, SidebarList, Slider, Spacer, SplitOrientation, SplitView, Stack, TabBar,
    TextEditor, TextField, TextView,
};

pub use crate::menu::{MenuBar, MenuShortcut, ShortcutKey, SubMenu};

pub use crate::render::{
    Canvas, Color, DrawingContext, Paint, PaintStyle, Point, Rect, RendererBackend,
};

/// Plots and scope views. Not glob-imported: the names are general enough
/// (`Axis`, `Range`, `Series`) that pulling them into every app would be rude.
/// Use `graph::quick` for the common shapes, or `graph::prelude::*` to bring
/// the lot in where you actually draw.
pub use crate::render::graph;

pub use crate::{AureaError, AureaResult};

pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};
