//! Runtime state modules: event queue, frame queue, damage queue.
//!
//! Explicit internal modules for runtime state as per Phase 1 architecture.

pub(crate) mod damage;
pub(crate) mod event_queue;
pub(crate) mod frame_queue;
