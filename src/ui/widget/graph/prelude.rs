//! Everything needed to put a graph on a canvas, in one import.
//!
//! ```rust,no_run
//! use aurea::ui::graph::prelude::*;
//! ```
//!
//! The views and their styles come along, and so do [`Rect`] and [`Color`],
//! because a graph is always shown in a rect and styled with colours.

pub use super::{
    Axis, Bounds, Channel, Cursor, Graph, Mapping, Placed, Plot, Points, Range, SampleBuffer,
    Scale, Scope, Series, Tick, TickPlan, Timebase, Trigger, TriggerEdge, TriggerMode,
};
pub use crate::render::{Color, Rect};
pub use crate::ui::{GraphStyle, GraphView, ScopeStyle, ScopeView};
