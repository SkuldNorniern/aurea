//! Core shared types, errors, platform detection, and events for Aurea.

pub mod capability;
pub mod error;
pub mod events;
pub mod platform;
pub mod sync;

pub use capability::{Capability, CapabilityChecker};
pub use error::{AureaError, AureaResult};
pub use events::{EventCallback, KeyCode, Modifiers, MouseButton, WindowEvent};
pub use platform::{DesktopPlatform, MobilePlatform, Platform};
pub use sync::lock;
