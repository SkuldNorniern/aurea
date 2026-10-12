//! Glob-importable set of the types most Aurea apps need.

pub use crate::window::{
    CursorGrabMode, CursorIcon, EventCallback, KeyCode, Modifiers, MouseButton, MouseButtons,
    PenPhase, PenSample, PenTool, Window, WindowEvent, WindowId, WindowType,
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

/// Plots and scope views.
pub use crate::render::graph;

pub use crate::{AureaError, AureaResult};

pub use aurea_runtime::{DamageRegion, FrameInfo, FrameScheduler};
