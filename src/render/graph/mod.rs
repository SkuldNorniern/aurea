//! Plots and oscilloscope views drawn onto a canvas.
//!
//! Everything here draws through [`DrawingContext`](aurea_render::DrawingContext),
//! so a graph works on whichever backend the canvas is using and takes part in
//! the same damage tracking as the rest of the frame.
//!
//! # A plot
//!
//! ```rust,no_run
//! use aurea::render::graph::{Axis, Graph, Series};
//! use aurea::render::Rect;
//!
//! let mut graph = Graph::new().with_y(Axis::fixed(-1.0, 1.0));
//! let signal = graph.add_series(Series::rolling("signal", 512));
//!
//! // Each frame: feed samples, then draw.
//! graph.push(signal, 0.5);
//! # fn draw(graph: &mut Graph, ctx: &mut dyn aurea::render::DrawingContext) {
//! graph.draw(ctx, Rect::new(0.0, 0.0, 800.0, 400.0)).ok();
//! # }
//! ```
//!
//! # A scope
//!
//! [`Scope`] is a plot set up the way an instrument is: channels with their own
//! volts per division and offset, a timebase in seconds per division, and a
//! trigger that lines the trace up so a repeating signal stands still.
//!
//! ```rust,no_run
//! use aurea::render::graph::{Channel, Scope, Trigger};
//!
//! let mut scope = Scope::new(1024);
//! scope.add_channel(Channel::new("CH1"));
//! scope.trigger = Trigger::rising(0.0);
//! ```
//!
//! # Using the pieces on their own
//!
//! The parts are separate on purpose. [`Mapping`] and [`TickPlan`] are plain
//! maths with no drawing in them, [`GraphStyle`] holds no state, and [`Graph`]
//! only puts them together. A view this module does not offer can use the scale
//! and tick maths directly and draw the rest itself.

mod buffer;
mod numeric;
mod plot;
mod scale;
mod scope;
mod series;
mod style;
mod ticks;

pub use buffer::SampleBuffer;
pub use plot::{Axis, Bounds, Cursor, Graph};
pub use scale::{Mapping, Placed, Range, Scale};
pub use scope::{Channel, Scope, Timebase, Trigger, TriggerEdge, TriggerMode};
pub use series::{Plot, Points, Series};
pub use style::{AxisStyle, GraphStyle, GridStyle, Margin, Stroke};
pub use ticks::{Tick, TickPlan};
