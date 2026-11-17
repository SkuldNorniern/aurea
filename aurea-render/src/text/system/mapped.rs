//! A font file mapped from disk: touched pages only, and the system can drop them when short.

use libc::{MAP_FAILED, MAP_PRIVATE, PROT_READ, mmap, munmap};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::ptr::null_mut;
use std::slice::from_raw_parts;

/// Read-only bytes of a file, which must not change while mapped: reading a shrunk file kills the
/// process.
pub(super) struct Mapped {
    start: *const u8,
    len: usize,
}

// SAFETY: the mapping is read-only and owned here until drop.
unsafe impl Send for Mapped {}
// SAFETY: as above; nothing writes through it.
unsafe impl Sync for Mapped {}

impl Mapped {
    /// `None` for an empty file or when it can't be mapped.
    pub(super) fn open(path: &Path) -> Option<Self> {
        let file = File::open(path).ok()?;
        let len = usize::try_from(file.metadata().ok()?.len()).ok()?;
        if len == 0 {
            return None;
        }
        // SAFETY: a new private read-only mapping of an open file; it outlives the descriptor.
        let start = unsafe { mmap(null_mut(), len, PROT_READ, MAP_PRIVATE, file.as_raw_fd(), 0) };
        (start != MAP_FAILED).then_some(Self {
            start: start.cast_const().cast(),
            len,
        })
    }
}

impl AsRef<[u8]> for Mapped {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: `start` holds `len` readable bytes until drop.
        unsafe { from_raw_parts(self.start, self.len) }
    }
}

impl Drop for Mapped {
    fn drop(&mut self) {
        // SAFETY: unmaps what `open` mapped, once; no borrow of it outlives `self`.
        unsafe { munmap(self.start.cast_mut().cast(), self.len) };
    }
}

#[cfg(test)]
mod tests {
    use super::Mapped;
    use std::env::temp_dir;
    use std::fs::{remove_file, write};
    use std::process::id;

    #[test]
    fn maps_a_file_and_refuses_an_empty_or_missing_one() {
        let path = temp_dir().join(format!("aurea-mapped-{}", id()));
        write(&path, b"font bytes").expect("write");
        let mapped = Mapped::open(&path).expect("mapped");
        assert_eq!(mapped.as_ref(), b"font bytes");
        drop(mapped);
        write(&path, b"").expect("write");
        assert!(Mapped::open(&path).is_none());
        remove_file(&path).expect("remove");
        assert!(Mapped::open(&path).is_none());
    }
}
