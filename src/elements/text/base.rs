//! The core every text widget wraps: content, selection, edits, undo, features and change reports,
//! written once over the native text calls.

use super::change::{TextChange, diff};
use super::features::TextFeatures;
use super::offsets::{Unit, to_bytes, to_units};
use crate::elements::native::{CallbackWatch, NativeElement, read_text};
use crate::ffi::{
    NG_ERROR_UNSUPPORTED, ng_platform_set_text_content, ng_platform_text_can_undo,
    ng_platform_text_get_selection, ng_platform_text_replace, ng_platform_text_set_feature,
    ng_platform_text_set_selection, ng_platform_text_undo, ng_platform_text_view_invalidate,
};
use crate::{AureaError, AureaResult, Platform};
use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::ops::Range;
use std::os::raw::{c_int, c_void};
use std::rc::Rc;

type Listener = Rc<dyn Fn(&TextChange, &str)>;

#[derive(Default)]
struct Changes {
    last: RefCell<Option<String>>,
    revision: Cell<u64>,
    listeners: RefCell<Vec<Listener>>,
}

impl Changes {
    fn user_edit(&self, handle: *mut c_void) {
        self.revision.set(self.revision.get() + 1);
        let Some(last) = self.last.borrow_mut().take() else {
            return;
        };
        let Ok(now) = read_text(handle) else {
            *self.last.borrow_mut() = Some(last);
            return;
        };
        let change = diff(&last, &now).map(|(range, inserted)| TextChange {
            range,
            inserted,
            revision: self.revision.get(),
        });
        *self.last.borrow_mut() = Some(now.clone());
        if let Some(change) = change {
            let listeners: Vec<Listener> = self.listeners.borrow().clone();
            for listener in listeners {
                listener(&change, &now);
            }
        }
    }

    fn own_edit(&self, handle: *mut c_void) {
        self.revision.set(self.revision.get() + 1);
        let mut last = self.last.borrow_mut();
        if last.is_some() {
            *last = read_text(handle).ok();
        }
    }
}

pub struct TextBase {
    native: NativeElement,
    multiline: bool,
    unit: Unit,
    features: Cell<TextFeatures>,
    changes: Rc<Changes>,
    _watch: Option<CallbackWatch>,
}

fn check(result: c_int, operation: &'static str) -> AureaResult<()> {
    match result {
        0 => Ok(()),
        NG_ERROR_UNSUPPORTED => Err(AureaError::Unsupported {
            operation,
            platform: Platform::current(),
        }),
        _ => Err(AureaError::ElementOperationFailed),
    }
}

impl TextBase {
    pub(crate) fn new(
        handle: *mut c_void,
        multiline: bool,
        features: TextFeatures,
    ) -> AureaResult<Self> {
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        let native = NativeElement::new(handle);
        let changes = Rc::new(Changes::default());
        let heard = Rc::clone(&changes);
        let watch = native.watch_change(move || heard.user_edit(handle)).ok();
        let base = Self {
            native,
            multiline,
            unit: Unit::of_platform(),
            features: Cell::new(features),
            changes,
            _watch: watch,
        };
        for (feature, on) in features.switches(multiline) {
            base.switch(feature, on)?;
        }
        Ok(base)
    }

    pub(crate) fn native(&self) -> &NativeElement {
        &self.native
    }

    fn handle(&self) -> *mut c_void {
        self.native.handle()
    }

    pub(crate) fn invalidate(&self) {
        unsafe { ng_platform_text_view_invalidate(self.handle()) };
    }

    pub(crate) fn text(&self) -> AureaResult<String> {
        read_text(self.handle())
    }

    pub(crate) fn set_text(&self, text: &str) -> AureaResult<()> {
        let text = CString::new(text).map_err(|_| AureaError::InvalidTitle)?;
        check(
            unsafe { ng_platform_set_text_content(self.handle(), text.as_ptr()) },
            "set_text",
        )?;
        self.changes.own_edit(self.handle());
        Ok(())
    }

    pub(crate) fn revision(&self) -> u64 {
        self.changes.revision.get()
    }

    fn to_native(&self, text: &str, range: &Range<usize>) -> AureaResult<(c_int, c_int)> {
        let unit = |byte| {
            to_units(text, byte, self.unit)
                .and_then(|u| c_int::try_from(u).ok())
                .ok_or(AureaError::InvalidTextRange)
        };
        if range.start > range.end {
            return Err(AureaError::InvalidTextRange);
        }
        Ok((unit(range.start)?, unit(range.end)?))
    }

    pub(crate) fn selection(&self) -> AureaResult<Range<usize>> {
        let (mut start, mut end): (c_int, c_int) = (0, 0);
        check(
            unsafe { ng_platform_text_get_selection(self.handle(), &raw mut start, &raw mut end) },
            "selection",
        )?;
        let text = self.text()?;
        let byte = |u: c_int| to_bytes(&text, usize::try_from(u).unwrap_or(0), self.unit);
        Ok(byte(start)..byte(end))
    }

    pub(crate) fn set_selection(&self, range: Range<usize>) -> AureaResult<()> {
        let (start, end) = self.to_native(&self.text()?, &range)?;
        check(
            unsafe { ng_platform_text_set_selection(self.handle(), start, end) },
            "set_selection",
        )
    }

    pub(crate) fn replace(&self, range: Range<usize>, with: &str) -> AureaResult<()> {
        let (start, end) = self.to_native(&self.text()?, &range)?;
        let with = CString::new(with).map_err(|_| AureaError::InvalidTitle)?;
        check(
            unsafe { ng_platform_text_replace(self.handle(), start, end, with.as_ptr()) },
            "replace",
        )?;
        self.changes.own_edit(self.handle());
        Ok(())
    }

    pub(crate) fn undo(&self, redo: bool) -> AureaResult<()> {
        check(
            unsafe { ng_platform_text_undo(self.handle(), c_int::from(redo)) },
            if redo { "redo" } else { "undo" },
        )?;
        self.changes.own_edit(self.handle());
        Ok(())
    }

    pub(crate) fn can_undo(&self, redo: bool) -> bool {
        unsafe { ng_platform_text_can_undo(self.handle(), c_int::from(redo)) != 0 }
    }

    pub(crate) fn features(&self) -> TextFeatures {
        self.features.get()
    }

    pub(crate) fn set_features(&self, features: TextFeatures) -> AureaResult<()> {
        for (feature, on) in features.changed_from(self.features.get(), self.multiline) {
            self.switch(feature, on)?;
        }
        self.features.set(features);
        Ok(())
    }

    fn switch(&self, feature: c_int, on: bool) -> AureaResult<()> {
        check(
            unsafe { ng_platform_text_set_feature(self.handle(), feature, c_int::from(on)) },
            "set_features",
        )
    }

    pub(crate) fn on_change(
        &self,
        listener: impl Fn(&TextChange, &str) + 'static,
    ) -> AureaResult<()> {
        if self._watch.is_none() {
            return Err(AureaError::Unsupported {
                operation: "on_change",
                platform: Platform::current(),
            });
        }
        let mut last = self.changes.last.borrow_mut();
        if last.is_none() {
            *last = Some(self.text()?);
        }
        self.changes.listeners.borrow_mut().push(Rc::new(listener));
        Ok(())
    }
}
