//! Runtime state: event queue, frame queue, damage region.

#![warn(missing_docs)]

pub mod damage;
pub mod event_queue;
pub mod frame_queue;

pub use damage::DamageRegion;
pub use event_queue::EventQueue;
pub use frame_queue::{FrameCallbackId, FrameInfo, FrameScheduler, TickerId};
