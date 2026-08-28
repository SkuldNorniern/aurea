//! Everything needed to put a graph on a canvas, in one import.
//!
//! ```rust,no_run
//! use aurea::render::graph::prelude::*;
//! ```
//!
//! The module's own items are all here. [`Rect`](aurea_render::Rect) and
//! [`Color`](aurea_render::Color) come along because a graph is always drawn
//! into a rect and styled with colours, and having to import them separately is
//! just friction.

pub use super::{
    Axis, Bounds, Channel, Cursor, Graph, GraphStyle, GridStyle, Mapping, Margin, Placed, Plot,
    Points, Range, SampleBuffer, Scale, Scope, Series, Stroke, Tick, TickPlan, Timebase, Trigger,
    TriggerEdge, TriggerMode,
};

pub use super::AxisStyle;

pub use aurea_render::{Color, DrawingContext, Font, Rect};
