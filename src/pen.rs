//! Where pen samples come from. Windows has two sources and picks one; every other system has one.

use std::sync::atomic::{AtomicU8, Ordering};

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

#[cfg(windows)]
unsafe extern "C" {
    fn ng_windows_pen_set_mode(mode: i32);
}

/// Chooses the source pens use, for the whole app; Windows only, others have one. A stroke being
/// drawn by the source that stops is cancelled. Call on the UI thread.
pub fn set_pen_mode(mode: PenMode) {
    let code = match mode {
        PenMode::Auto => 0,
        PenMode::System => 1,
        PenMode::Wintab => 2,
    };
    MODE.store(code, Ordering::Relaxed);
    SOURCE.store(0, Ordering::Relaxed);
    #[cfg(windows)]
    // SAFETY: takes any value, and the caller is on the UI thread.
    unsafe {
        ng_windows_pen_set_mode(i32::from(code));
    }
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

pub(crate) fn note_source(wintab: bool) {
    SOURCE.store(if wintab { 2 } else { 1 }, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_source_starts_over_with_the_mode_and_follows_samples() {
        note_source(true);
        assert_eq!(pen_source(), PenSource::Wintab);
        note_source(false);
        assert_eq!(pen_source(), PenSource::System);
        set_pen_mode(PenMode::Wintab);
        assert_eq!(pen_mode(), PenMode::Wintab);
        assert_eq!(pen_source(), PenSource::None);
        set_pen_mode(PenMode::default());
        assert_eq!(pen_mode(), PenMode::Auto);
    }
}
