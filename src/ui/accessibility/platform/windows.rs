//! Subclass the canvas through comctl32 so attachment works for an existing, visible canvas and
//! removal cannot overwrite another component's window proc.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use accesskit::TreeUpdate;
use accesskit_windows::{Adapter as NativeAdapter, HWND, LPARAM, LRESULT, WPARAM};

use super::super::Handler;
use crate::render::Canvas;
use crate::{AureaError, AureaResult};

type SubclassProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT;

#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(hwnd: HWND, callback: SubclassProc, id: usize, data: usize) -> i32;
    fn RemoveWindowSubclass(hwnd: HWND, callback: SubclassProc, id: usize) -> i32;
    fn DefSubclassProc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
}

struct State {
    native: RefCell<NativeAdapter>,
    handler: RefCell<Handler>,
    destroyed: Cell<bool>,
    hwnd: HWND,
}

pub(in crate::ui::accessibility) struct Adapter(Box<State>);

impl Adapter {
    pub(in crate::ui::accessibility) fn new(
        canvas: &Canvas,
        handler: Handler,
    ) -> AureaResult<Self> {
        let hwnd = HWND(canvas.native_handle());
        let state = Box::new(State {
            native: RefCell::new(NativeAdapter::new(hwnd, false, handler.clone())),
            handler: RefCell::new(handler),
            destroyed: Cell::new(false),
            hwnd,
        });
        let pointer = (&*state as *const State).cast::<c_void>() as usize;
        if unsafe { SetWindowSubclass(hwnd, subclass, pointer, pointer) } == 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(Self(state))
    }

    pub(in crate::ui::accessibility) fn update(&mut self, update: TreeUpdate) {
        if self.0.destroyed.get() {
            return;
        }
        let events = self.0.native.borrow_mut().update_if_active(|| update);
        if let Some(events) = events {
            events.raise();
        }
    }

    pub(in crate::ui::accessibility) fn focused(&mut self, focused: bool) {
        if self.0.destroyed.get() {
            return;
        }
        let events = self
            .0
            .native
            .borrow_mut()
            .update_window_focus_state(focused);
        if let Some(events) = events {
            events.raise();
        }
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        if !self.0.destroyed.get() {
            let id = (&*self.0 as *const State) as usize;
            unsafe {
                RemoveWindowSubclass(self.0.hwnd, subclass, id);
            }
        }
    }
}

unsafe extern "system" fn subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let state = unsafe { &*(data as *const State) };
        match message {
            0x003D => {
                let result = state.native.borrow_mut().handle_wm_getobject(
                    wparam,
                    lparam,
                    &mut *state.handler.borrow_mut(),
                );
                result.map(Into::into)
            }
            0x0082 => {
                state.destroyed.set(true);
                unsafe {
                    RemoveWindowSubclass(hwnd, subclass, id);
                }
                None
            }
            _ => None,
        }
    }));
    match result {
        Ok(Some(result)) => result,
        _ => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
    }
}
