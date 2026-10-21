//! Core shared types, errors, platform detection, and events for Aurea.

pub mod capability;
/// Error type and result alias.
pub mod error;
pub mod events;
pub mod geometry;
pub mod hash;
pub mod os_version;
pub mod platform;
pub mod sync;

pub use capability::{Capability, CapabilityChecker, Support};
pub use error::{AureaError, AureaResult};
pub use events::{
    EventCallback, KeyCode, Modifiers, MouseButton, MouseButtons, PenPhase, PenSample, PenTool,
    WindowEvent,
};
pub use geometry::{Point, Rect};
pub use hash::{FastBuild, FastHasher};
pub use os_version::OsVersion;
pub use platform::{DesktopPlatform, MobilePlatform, Platform};
pub use sync::lock;
