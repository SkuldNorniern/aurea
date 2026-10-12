//! Shapes and the transforms that move them.

mod path;
mod transform;

// Geometry belongs to the foundation, not to the renderer: the runtime speaks
// in rectangles too, and it should not need a renderer to do so.
pub use aurea_foundation::{Point, Rect};

pub use path::{FillRule, Path, PathCommand};
pub use transform::Transform;
