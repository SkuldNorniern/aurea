//! Raw mouse deltas, as `WindowEvent::RawMouseMotion`, and the cursor lock they make possible.
//! Not on iOS or Android, which have no raw mouse deltas.

#[cfg(not(feature = "raw-mouse"))]
mod off;
#[cfg(feature = "raw-mouse")]
mod on;

#[cfg(not(feature = "raw-mouse"))]
pub(crate) use off::deliver;
#[cfg(feature = "raw-mouse")]
pub(crate) use on::deliver;

/// Whether raw mouse deltas and cursor lock are built in.
pub(crate) const fn built() -> bool {
    cfg!(feature = "raw-mouse")
}
