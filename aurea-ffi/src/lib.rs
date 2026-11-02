//! Rust <-> native ABI declarations for Aurea.

#![warn(missing_docs)]

#[cfg(all(any(target_os = "ios", target_os = "android"), not(feature = "mobile")))]
compile_error!(
    "Aurea's iOS and Android hosts are experimental: enable the `mobile` feature to build them"
);

mod codes;
mod declarations;

pub use codes::*;
pub use declarations::*;
