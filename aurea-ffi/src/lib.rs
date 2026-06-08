//! Rust <-> native ABI declarations for Aurea.
//!
//! This crate provides the extern "C" declarations for the native platform API.
//! The native library is built by this crate's build.rs.

mod declarations;

pub use declarations::*;
