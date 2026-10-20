//! The platform's dialogs: file open, save as and pick a folder, message boxes, and the colour
//! chooser.

use std::ffi::{CStr, CString, c_int, c_uint};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use crate::ffi::{
    NG_ERROR_UNSUPPORTED, ng_platform_color_dialog, ng_platform_file_dialog,
    ng_platform_free_dialog_paths, ng_platform_message_box,
};
use crate::render::Color;
use crate::{AureaError, AureaResult, Platform, Window};

const OPEN: c_int = 0;
const SAVE: c_int = 1;
const FOLDER: c_int = 2;

/// A file dialog to set up, then show with [`open`](Self::open), [`save`](Self::save) or
/// [`pick_folder`](Self::pick_folder).
#[derive(Clone, Debug, Default)]
pub struct FileDialog {
    title: String,
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

    /// Where to save; `None` when cancelled.
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

/// How a message box looks: its icon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageKind {
    #[default]
    Info,
    Warning,
    Error,
}

impl MessageKind {
    fn code(self) -> c_int {
        match self {
            Self::Info => 0,
            Self::Warning => 1,
            Self::Error => 2,
        }
    }
}

/// The buttons a message box has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageButtons {
    #[default]
    Ok,
    OkCancel,
    YesNo,
    SaveDiscardCancel,
}

impl MessageButtons {
    fn code(self) -> c_int {
        match self {
            Self::Ok => 0,
            Self::OkCancel => 1,
            Self::YesNo => 2,
            Self::SaveDiscardCancel => 3,
        }
    }
}

/// The button the person answered with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageChoice {
    Ok,
    Cancel,
    Yes,
    No,
    Save,
    Discard,
}

impl MessageChoice {
    fn from_code(code: c_int) -> Option<Self> {
        Some(match code {
            0 => Self::Ok,
            1 => Self::Cancel,
            2 => Self::Yes,
            3 => Self::No,
            4 => Self::Save,
            5 => Self::Discard,
            _ => return None,
        })
    }
}

/// A message box to set up, then show with [`show`](Self::show).
#[derive(Clone, Debug)]
pub struct MessageDialog {
    title: String,
    message: String,
    kind: MessageKind,
    buttons: MessageButtons,
}

impl MessageDialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: String::new(),
            kind: MessageKind::default(),
            buttons: MessageButtons::default(),
        }
    }

    /// The body text.
    pub fn message(mut self, text: impl Into<String>) -> Self {
        self.message = text.into();
        self
    }

    /// The icon: information when not set.
    pub fn kind(mut self, kind: MessageKind) -> Self {
        self.kind = kind;
        self
    }

    /// The buttons: OK when not set.
    pub fn buttons(mut self, buttons: MessageButtons) -> Self {
        self.buttons = buttons;
        self
    }

    /// Waits for the person to answer.
    pub fn show(self, window: &Window) -> AureaResult<MessageChoice> {
        let text = |s: &str| CString::new(s).map_err(|_| AureaError::InvalidTitle);
        let title = text(&self.title)?;
        let message = text(&self.message)?;
        let answer = unsafe {
            ng_platform_message_box(
                window.handle(),
                self.kind.code(),
                title.as_ptr(),
                message.as_ptr(),
                self.buttons.code(),
            )
        };
        if answer == NG_ERROR_UNSUPPORTED {
            return Err(AureaError::Unsupported {
                operation: "message_box",
                platform: Platform::current(),
            });
        }
        MessageChoice::from_code(answer).ok_or(AureaError::PlatformError(answer))
    }
}

/// A colour chooser to set up, then show with [`show`](Self::show).
#[derive(Clone, Debug)]
pub struct ColorDialog {
    title: String,
    start: Color,
    alpha: bool,
}

impl Default for ColorDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorDialog {
    /// Starts at black, without alpha.
    pub fn new() -> Self {
        Self {
            title: String::new(),
            start: Color::rgb(0, 0, 0),
            alpha: false,
        }
    }

    /// The window title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// The colour shown first.
    pub fn color(mut self, start: Color) -> Self {
        self.start = start;
        self
    }

    /// Offers opacity where the platform can (not Windows, whose chooser has none).
    pub fn alpha(mut self, alpha: bool) -> Self {
        self.alpha = alpha;
        self
    }

    /// Waits for the person to pick; `None` when they cancel, including by closing the chooser.
    pub fn show(self, window: &Window) -> AureaResult<Option<Color>> {
        let title = CString::new(self.title).map_err(|_| AureaError::InvalidTitle)?;
        let mut picked: c_uint = 0;
        let answer = unsafe {
            ng_platform_color_dialog(
                window.handle(),
                title.as_ptr(),
                argb_of(self.start),
                c_int::from(self.alpha),
                &mut picked,
            )
        };
        match answer {
            1 => Ok(Some(color_of(picked))),
            0 => Ok(None),
            NG_ERROR_UNSUPPORTED => Err(AureaError::Unsupported {
                operation: "color_dialog",
                platform: Platform::current(),
            }),
            code => Err(AureaError::PlatformError(code)),
        }
    }
}

fn argb_of(color: Color) -> c_uint {
    u32::from_be_bytes([color.a, color.r, color.g, color.b])
}

fn color_of(argb: c_uint) -> Color {
    let [a, r, g, b] = argb.to_be_bytes();
    Color::rgba(r, g, b, a)
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

    #[test]
    fn message_kinds_and_buttons_are_numbered_as_native() {
        assert_eq!(
            [MessageKind::Info, MessageKind::Warning, MessageKind::Error].map(MessageKind::code),
            [0, 1, 2]
        );
        assert_eq!(
            [
                MessageButtons::Ok,
                MessageButtons::OkCancel,
                MessageButtons::YesNo,
                MessageButtons::SaveDiscardCancel,
            ]
            .map(MessageButtons::code),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn message_choices_come_back_as_native_numbers() {
        assert_eq!(
            [0, 1, 2, 3, 4, 5, 6, -1].map(MessageChoice::from_code),
            [
                Some(MessageChoice::Ok),
                Some(MessageChoice::Cancel),
                Some(MessageChoice::Yes),
                Some(MessageChoice::No),
                Some(MessageChoice::Save),
                Some(MessageChoice::Discard),
                None,
                None,
            ]
        );
    }

    #[test]
    fn colours_are_packed_as_argb_words() {
        assert_eq!(argb_of(Color::rgba(0x11, 0x22, 0x33, 0x44)), 0x4411_2233);
        assert_eq!(argb_of(Color::rgb(0, 0, 0)), 0xFF00_0000);
    }

    #[test]
    fn argb_words_come_back_as_colours() {
        assert_eq!(color_of(0x4411_2233), Color::rgba(0x11, 0x22, 0x33, 0x44));
        assert_eq!(color_of(0x00FF_8000), Color::rgba(0xFF, 0x80, 0x00, 0x00));
        let c = Color::rgba(9, 8, 7, 6);
        assert_eq!(color_of(argb_of(c)), c);
    }

    #[test]
    fn colour_dialog_starts_at_black_without_alpha() {
        let d = ColorDialog::new();
        assert_eq!(d.start, Color::rgb(0, 0, 0));
        assert!(!d.alpha);
        let d = d.color(Color::rgba(1, 2, 3, 4)).alpha(true).title("Pick");
        assert_eq!(
            (d.start, d.alpha, d.title.as_str()),
            (Color::rgba(1, 2, 3, 4), true, "Pick")
        );
    }
}
