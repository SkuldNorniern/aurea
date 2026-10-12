//! The platform's file dialogs: open, save as, and pick a folder.
//!
//! Each call waits for the person to choose and returns what they chose, or
//! nothing when they cancel. The window behind keeps drawing meanwhile: the
//! dialog runs the platform's own loop, which still hands out frames.
//!
//! ```rust,no_run
//! # fn f(window: &aurea::Window) -> aurea::AureaResult<()> {
//! use aurea::dialog::FileDialog;
//!
//! let photos = FileDialog::new()
//!     .title("Open")
//!     .filter("Images", &["png", "jpg", "jpeg"])
//!     .filter("All files", &["*"])
//!     .multiple(true)
//!     .open(window)?;
//! let target = FileDialog::new().filter("PNG", &["png"]).file_name("photo.png").save(window)?;
//! # Ok(())
//! # }
//! ```

use std::ffi::{CStr, CString, c_int};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use crate::ffi::{ng_platform_file_dialog, ng_platform_free_dialog_paths};
use crate::{AureaError, AureaResult, Window};

/// What the dialog picks, as the native side numbers it.
const OPEN: c_int = 0;
const SAVE: c_int = 1;
const FOLDER: c_int = 2;

/// A file dialog to set up, then show with [`open`](Self::open),
/// [`save`](Self::save) or [`pick_folder`](Self::pick_folder).
#[derive(Clone, Debug, Default)]
pub struct FileDialog {
    title: String,
    /// One line per filter: `Name|ext;ext`.
    filters: String,
    file_name: String,
    directory: Option<PathBuf>,
    multiple: bool,
}

impl FileDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    /// Files ending in one of `extensions`, without dots, shown as `name`.
    /// `"*"` takes any file. The first filter is the one shown first, and
    /// on save its first extension is added to a name typed without one.
    pub fn filter(mut self, name: &str, extensions: &[&str]) -> Self {
        let clean = |s: &str| s.replace(['|', '\n', ';'], " ");
        if !self.filters.is_empty() {
            self.filters.push('\n');
        }
        self.filters.push_str(&clean(name));
        self.filters.push('|');
        let exts: Vec<String> = extensions
            .iter()
            .map(|e| clean(e.trim_start_matches('.')))
            .collect();
        self.filters.push_str(&exts.join(";"));
        self
    }

    /// The name a save dialog starts with.
    pub fn file_name(mut self, name: &str) -> Self {
        self.file_name = name.to_string();
        self
    }

    /// The folder it opens in.
    pub fn directory(mut self, directory: &Path) -> Self {
        self.directory = Some(directory.to_path_buf());
        self
    }

    /// Lets an open dialog take several files.
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    /// The files chosen to open; empty when cancelled.
    pub fn open(&self, parent: &Window) -> AureaResult<Vec<PathBuf>> {
        self.run(Some(parent), OPEN)
    }

    /// Where to save; `None` when cancelled. The platform has already asked
    /// about replacing a file that exists.
    pub fn save(&self, parent: &Window) -> AureaResult<Option<PathBuf>> {
        Ok(self.run(Some(parent), SAVE)?.into_iter().next())
    }

    /// The folder chosen; `None` when cancelled.
    pub fn pick_folder(&self, parent: &Window) -> AureaResult<Option<PathBuf>> {
        Ok(self.run(Some(parent), FOLDER)?.into_iter().next())
    }

    fn run(&self, parent: Option<&Window>, kind: c_int) -> AureaResult<Vec<PathBuf>> {
        let text = |s: &str| CString::new(s).map_err(|_| AureaError::InvalidTitle);
        let title = text(&self.title)?;
        let filters = text(&self.filters)?;
        let name = text(&self.file_name)?;
        let directory = text(
            &self
                .directory
                .as_deref()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )?;
        let window = parent.map_or(null_mut(), Window::handle);
        let raw = unsafe {
            ng_platform_file_dialog(
                window,
                kind,
                title.as_ptr(),
                filters.as_ptr(),
                name.as_ptr(),
                directory.as_ptr(),
                c_int::from(self.multiple),
            )
        };
        if raw.is_null() {
            return Ok(Vec::new());
        }
        let paths = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect();
        unsafe { ng_platform_free_dialog_paths(raw) };
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_are_written_one_per_line() {
        let d = FileDialog::new()
            .filter("Images", &["png", ".jpg"])
            .filter("All files", &["*"]);
        assert_eq!(d.filters, "Images|png;jpg\nAll files|*");
    }

    #[test]
    fn separators_in_names_cannot_break_the_list() {
        let d = FileDialog::new().filter("A|B\nC", &["x;y"]);
        assert_eq!(d.filters, "A B C|x y");
    }
}
