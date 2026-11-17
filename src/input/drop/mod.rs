//! Files dragged over or dropped on a window, as `WindowEvent::FilesHovered`, `FilesDropped` and
//! `FilesHoverEnded`.

#[cfg(not(feature = "drop-files"))]
mod off;
#[cfg(feature = "drop-files")]
mod on;

#[cfg(not(feature = "drop-files"))]
pub(crate) use off::deliver;
#[cfg(feature = "drop-files")]
pub(crate) use on::deliver;

#[cfg(target_os = "ios")]
#[cfg(not(feature = "drop-files"))]
pub(crate) use off::dropped;
#[cfg(target_os = "ios")]
#[cfg(feature = "drop-files")]
pub(crate) use on::dropped;

/// Whether file drops are built in.
pub(crate) const fn built() -> bool {
    cfg!(feature = "drop-files")
}
