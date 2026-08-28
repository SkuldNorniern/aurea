//! Plots and oscilloscope views drawn onto a canvas.

mod buffer;
mod numeric;
mod plot;
pub mod prelude;
pub mod quick;
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
