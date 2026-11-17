//! Where pen samples come from. Windows has two sources and picks one; every other system has one.
//! The pen events are built and delivered in `on` with the `pen` feature, and dropped without it.

use std::sync::atomic::{AtomicU8, Ordering};

#[cfg(not(feature = "pen"))]
mod off;
#[cfg(feature = "pen")]
mod on;

// The mobile builders are each used by one or two of the hosts, not on every target.
#[allow(unused_imports)]
#[cfg(not(feature = "pen"))]
pub(crate) use off::{
    batch_size, deliver, hover, predicted, sample, set_batch_size, set_batching, set_mode, tap,
};
#[allow(unused_imports)]
#[cfg(feature = "pen")]
pub(crate) use on::{
    batch_size, deliver, hover, predicted, sample, set_batch_size, set_batching, set_mode, tap,
};

/// Which source pens may use, as a paint app's tablet setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PenMode {
    /// Windows Ink while it sends the pen, Wintab while it does not.
    #[default]
    Auto,
    /// Windows Ink only.
    System,
    /// Wintab only, when the tablet's driver has it.
    Wintab,
}

/// Which source the latest pen sample came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PenSource {
    /// No sample yet.
    #[default]
    None,
    /// The system's own pen input: Windows Ink, on Windows.
    System,
    /// A Wintab driver.
    Wintab,
}

static MODE: AtomicU8 = AtomicU8::new(0);
static SOURCE: AtomicU8 = AtomicU8::new(0);

/// Chooses the source pens use, for the whole app; Windows only, others have one. A stroke being
/// drawn by the source that stops is cancelled. Call on the UI thread.
pub fn set_pen_mode(mode: PenMode) {
    set_mode(mode);
}

/// The source chosen with [`set_pen_mode`].
pub fn pen_mode() -> PenMode {
    match MODE.load(Ordering::Relaxed) {
        1 => PenMode::System,
        2 => PenMode::Wintab,
        _ => PenMode::Auto,
    }
}

/// The source the latest pen sample came from, for an app to show which one is live. It stays
/// [`PenSource::None`] while nothing sends the pen, as when Windows Ink is off and the driver has
/// no Wintab, or the tablet is only a mouse.
pub fn pen_source() -> PenSource {
    match SOURCE.load(Ordering::Relaxed) {
        1 => PenSource::System,
        2 => PenSource::Wintab,
        _ => PenSource::None,
    }
}

/// Whether pen input is built in.
pub(crate) const fn built() -> bool {
    cfg!(feature = "pen")
}
