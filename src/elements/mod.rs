//! Native UI elements (widgets) and container traits.

// Layout and the base traits stay in every build: a window's content and a canvas are elements.
// The native controls live in `widgets` and need that feature.
mod container;
mod divider;
mod native;
mod shared;
mod traits;

#[cfg(feature = "widgets")]
mod widgets;

#[cfg(feature = "embed")]
mod swiftui_host;

pub use container::{Orientation, Stack};
pub use divider::{Divider, DividerOrientation};
pub use native::NativeElement;
pub use shared::Shared;
pub use traits::{Container, Element};

#[cfg(feature = "widgets")]
pub use widgets::*;

#[cfg(feature = "embed")]
pub use swiftui_host::{SwiftUIHost, SwiftUIHostNotAvailable};
