//! Text controls.

mod base;
mod change;
mod editor;
mod features;
mod field;
mod offsets;
mod view;

pub use change::TextChange;
pub use editor::TextEditor;
pub use features::TextFeatures;
pub use field::TextField;
pub use view::TextView;

use crate::AureaResult;
use crate::elements::Element;
use std::ops::Range;

mod sealed {
    /// Only Aurea's text widgets have a text core.
    pub trait Sealed {
        fn text_base(&self) -> &super::base::TextBase;
    }
}

/// What every text control can do.
pub trait TextControl: Element + sealed::Sealed {
    /// Returns the whole text.
    fn text(&self) -> AureaResult<String> {
        self.text_base().text()
    }

    /// Replaces all the text.
    fn set_text(&self, text: &str) -> AureaResult<()> {
        self.text_base().set_text(text)
    }

    /// Counts every change to the text, the user's and the app's.
    fn revision(&self) -> u64 {
        self.text_base().revision()
    }

    /// The selected bytes; empty at the cursor when nothing is selected.
    fn selection(&self) -> AureaResult<Range<usize>> {
        self.text_base().selection()
    }

    /// On macOS a field takes a selection only while it is being edited.
    fn set_selection(&self, range: Range<usize>) -> AureaResult<()> {
        self.text_base().set_selection(range)
    }

    /// Replaces `range` with `with`, as one undoable edit, and puts the cursor after it.
    fn replace(&self, range: Range<usize>, with: &str) -> AureaResult<()> {
        self.text_base().replace(range, with)
    }

    /// Replaces the selected text with `with`, as one undoable edit.
    fn replace_selection(&self, with: &str) -> AureaResult<()> {
        let base = self.text_base();
        base.replace(base.selection()?, with)
    }

    /// Undoes the last edit.
    fn undo(&self) -> AureaResult<()> {
        self.text_base().undo(false)
    }

    /// Redoes the last undone edit.
    fn redo(&self) -> AureaResult<()> {
        self.text_base().undo(true)
    }

    /// Returns whether there is an edit to undo.
    fn can_undo(&self) -> bool {
        self.text_base().can_undo(false)
    }

    /// Returns whether there is an undone edit to redo.
    fn can_redo(&self) -> bool {
        self.text_base().can_undo(true)
    }

    /// Returns the features this control has.
    fn features(&self) -> TextFeatures {
        self.text_base().features()
    }

    /// Sets the features this control has.
    fn set_features(&self, features: TextFeatures) -> AureaResult<()> {
        self.text_base().set_features(features)
    }

    /// Calls `listener` after each edit the user makes, typing, pasting or undoing by keyboard.
    fn on_change(&self, listener: impl Fn(&TextChange) + 'static) -> AureaResult<()>
    where
        Self: Sized,
    {
        self.text_base()
            .on_change(move |change, _| listener(change))
    }
}

/// The parts every text widget has the same way.
macro_rules! text_widget {
    ($widget:ty) => {
        impl crate::elements::text::sealed::Sealed for $widget {
            fn text_base(&self) -> &crate::elements::text::base::TextBase {
                &self.base
            }
        }

        impl crate::elements::text::TextControl for $widget {}

        impl crate::elements::Element for $widget {
            fn handle(&self) -> *mut std::os::raw::c_void {
                self.base.native().handle()
            }

            fn released_to_parent(&self) {
                self.base.native().released_to_parent();
            }

            unsafe fn invalidate_platform(&self, _rect: Option<crate::render::Rect>) {
                self.base.invalidate();
            }
        }

        impl $widget {
            /// Replaces all the text; as
            /// [`TextControl::set_text`](crate::elements::TextControl::set_text).
            pub fn set_content(&mut self, content: &str) -> crate::AureaResult<()> {
                self.base.set_text(content)
            }

            /// Returns the whole text.
            pub fn get_content(&self) -> crate::AureaResult<String> {
                self.base.text()
            }
        }
    };
}

use text_widget;

#[cfg(all(test, any(target_os = "windows", target_os = "linux")))]
mod native_tests {
    //! The text API on the native controls, the same checks on each platform; only typing as the
    //! user differs.

    use super::*;
    use crate::elements::{Container, Orientation, Shared, Stack};
    use crate::{AureaError, Window};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[cfg(target_os = "windows")]
    fn type_text(control: &impl Element, text: &str) {
        use std::ffi::c_void;
        const WM_CHAR: u32 = 0x0102;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
        }
        for unit in text.encode_utf16() {
            unsafe { SendMessageW(control.handle(), WM_CHAR, usize::from(unit), 0) };
        }
    }

    #[cfg(target_os = "linux")]
    fn type_text(control: &impl Element, text: &str) {
        use std::ffi::{CString, c_void};
        use std::os::raw::{c_char, c_int};
        unsafe extern "C" {
            fn gtk_bin_get_child(bin: *mut c_void) -> *mut c_void;
            fn gtk_text_view_get_buffer(view: *mut c_void) -> *mut c_void;
            fn gtk_text_view_get_editable(view: *mut c_void) -> c_int;
            fn gtk_text_buffer_insert_interactive_at_cursor(
                buffer: *mut c_void,
                text: *const c_char,
                len: c_int,
                editable: c_int,
            ) -> c_int;
        }
        let text = CString::new(text).unwrap_or_default();
        unsafe {
            let view = gtk_bin_get_child(control.handle());
            let editable = gtk_text_view_get_editable(view);
            gtk_text_buffer_insert_interactive_at_cursor(
                gtk_text_view_get_buffer(view),
                text.as_ptr(),
                -1,
                editable,
            );
        }
    }

    fn editor_in(window: &mut Window) -> AureaResult<Shared<TextEditor>> {
        let editor = Shared::new(TextEditor::new()?);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(editor.clone(), 1.0)?;
        window.set_content(root)?;
        Ok(editor)
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn selection_and_replace_work_in_bytes() -> AureaResult<()> {
        let mut window = Window::new("select", 400, 300)?;
        let editor = editor_in(&mut window)?;
        let editor = editor.borrow();
        editor.set_text("a한🙂b\nc")?;
        editor.set_selection(4..8)?;
        assert_eq!(editor.selection()?, 4..8);
        editor.replace_selection("X")?;
        assert_eq!(editor.text()?, "a한Xb\nc");
        assert_eq!(editor.selection()?, 5..5, "cursor after the replacement");
        editor.replace(7..8, "line")?;
        assert_eq!(editor.text()?, "a한Xb\nline");
        assert!(matches!(
            editor.set_selection(2..3),
            Err(AureaError::InvalidTextRange)
        ));
        assert!(matches!(
            editor.replace(5..99, ""),
            Err(AureaError::InvalidTextRange)
        ));
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn edits_undo_and_redo() -> AureaResult<()> {
        let mut window = Window::new("undo", 400, 300)?;
        let editor = editor_in(&mut window)?;
        let editor = editor.borrow();
        editor.set_text("one")?;
        assert!(!editor.can_undo(), "new text starts a new history");
        editor.replace(3..3, " two")?;
        assert!(editor.can_undo());
        editor.undo()?;
        assert_eq!(editor.text()?, "one");
        assert!(editor.can_redo());
        editor.redo()?;
        assert_eq!(editor.text()?, "one two");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn only_the_users_edits_are_reported() -> AureaResult<()> {
        let mut window = Window::new("changes", 400, 300)?;
        let editor = editor_in(&mut window)?;
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&seen);
        editor
            .borrow()
            .on_change(move |c| log.borrow_mut().push(c.clone()))?;
        editor.borrow().set_text("ab")?;
        editor.borrow().replace(2..2, "c")?;
        assert!(seen.borrow().is_empty());
        let before = editor.borrow().revision();
        editor.borrow().set_selection(3..3)?;
        type_text(&*editor.borrow(), "한");
        let seen = seen.borrow();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(
            (seen[0].range.clone(), seen[0].inserted.as_str()),
            (3..3, "한")
        );
        assert!(seen[0].revision > before);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn features_switch_at_runtime() -> AureaResult<()> {
        let mut window = Window::new("features", 400, 300)?;
        let editor = editor_in(&mut window)?;
        let editor = editor.borrow();
        assert_eq!(editor.features(), TextFeatures::EDITOR);
        editor.set_text("fixed")?;
        editor.set_features(TextFeatures::EDITOR.with_read_only(true))?;
        editor.set_selection(5..5)?;
        type_text(&*editor, "!");
        assert_eq!(editor.text()?, "fixed", "typed into a read-only editor");
        editor.set_features(TextFeatures::VIEW)?;
        assert_eq!(editor.features(), TextFeatures::VIEW);
        type_text(&*editor, "!");
        assert_eq!(editor.text()?, "fixed!");
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn typed_words_undo_together() -> AureaResult<()> {
        let mut window = Window::new("words", 400, 300)?;
        let editor = editor_in(&mut window)?;
        let editor = editor.borrow();
        for key in ["h", "e", "y", " ", "x"] {
            type_text(&*editor, key);
        }
        let mut steps = Vec::new();
        while editor.can_undo() {
            editor.undo()?;
            steps.push(editor.text()?);
        }
        assert_eq!(steps, ["hey ", "hey", ""]);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_field_shares_the_api() -> AureaResult<()> {
        let _window = Window::new("field", 400, 300)?;
        let field = TextField::with_content("hello")?;
        field.set_selection(1..3)?;
        assert_eq!(field.selection()?, 1..3);
        field.replace_selection("EE")?;
        assert_eq!(field.text()?, "hEElo");
        field.set_features(TextFeatures::FIELD.with_read_only(true))?;
        assert!(field.features().read_only);
        Ok(())
    }
}
