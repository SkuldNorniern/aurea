//! Rust <-> native ABI declarations for Aurea, and the build of its native layer: Win32 on Windows,
//! AppKit on macOS, GTK3 on Linux. Apps use `aurea`, not this crate.

#[cfg(all(any(target_os = "ios", target_os = "android"), not(feature = "mobile")))]
compile_error!(
    "Aurea's iOS and Android hosts are experimental: enable the `mobile` feature to build them"
);

mod codes;
mod declarations;

pub use codes::*;
pub use declarations::*;
