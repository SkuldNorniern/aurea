//! Vector images: SVG path data, a small SVG reader, and built-in icons.

mod icons;
pub(crate) mod path_data;
mod svg;

pub use icons::Icon;
pub use path_data::{SvgError, append_path_data, parse_path_data};
pub use svg::VectorImage;
