use super::native::NativeElement;
use super::traits::Element;
use crate::registry::elements::{
    next_button_id, register_button_callback, unregister_button_callback,
};
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::{ffi::CString, os::raw::c_void};

pub struct Button {
    handle: NativeElement,
    _title: CString,
    _id: u32,
}

impl Button {
    pub fn new(title: &str) -> AureaResult<Self> {
        Self::with_callback(title, || {})
    }

    pub fn with_callback<F>(title: &str, callback: F) -> AureaResult<Self>
    where
        F: Fn() + 'static,
    {
        let id = next_button_id();

        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
        let handle = unsafe { ng_platform_create_button(title.as_ptr(), id) };

        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }

        register_button_callback(id, callback);

        Ok(Self {
            handle: NativeElement::new(handle),
            _title: title,
            _id: id,
        })
    }
}

impl Button {
    /// Id this button's callback is registered under.
    ///
    /// Public so an integration test can check that dropping the button drops
    /// the registration; there is no other reason to want it.
    #[doc(hidden)]
    pub fn callback_id(&self) -> u32 {
        self._id
    }
}

impl Element for Button {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {
        unsafe {
            ng_platform_button_invalidate(self.handle.handle());
        }
    }
}

impl Drop for Button {
    fn drop(&mut self) {
        // The registry held the closure for the life of the process otherwise,
        // and it keeps alive whatever the application captured in it.
        unregister_button_callback(self._id);
    }
}

#[cfg(test)]
mod callback_lifetime_tests {
    use crate::registry::elements::invoke_button_callback;
    use crate::{AureaResult, Window};

    /// A dropped widget takes its callback registration with it, and everything the
    /// application captured in it.
    ///
    /// This lives here rather than as a unit test because it creates a real native
    /// button: GTK aborts the process when asked to build a widget with no display
    /// connection, which took the whole suite down on a headless Linux box.
    #[test]
    #[ignore = "creates a native widget; run with --ignored"]
    fn dropping_a_button_drops_its_callback() -> AureaResult<()> {
        use crate::elements::Button;
        use std::cell::Cell;
        use std::rc::Rc;

        let _window = Window::new("callback lifetime", 200, 100)?;

        let fired = Rc::new(Cell::new(0));
        let f = Rc::clone(&fired);
        let button = Button::with_callback("go", move || f.set(f.get() + 1))?;
        let id = button.callback_id();

        invoke_button_callback(id);
        assert_eq!(fired.get(), 1, "callback should run while the button lives");

        drop(button);
        invoke_button_callback(id);

        assert_eq!(fired.get(), 1, "callback should be gone with the button");
        assert_eq!(
            Rc::strong_count(&fired),
            1,
            "the registry still holds what the callback captured"
        );
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod win32_click_tests {

    //! A real click has to reach the callback.
    //!
    //! The unit tests invoke the FFI entry point directly, which proves the
    //! registry works and nothing about the path a click actually takes. That path
    //! was broken: `WM_COMMAND` carries the control id in `LOWORD(wParam)`, so a
    //! button command base of 100000 truncated to 34465, failed the "is this a
    //! button" test, and every click was dispatched to the menu registry instead.
    //!
    //! These need a display and create real native windows, so they are `#[ignore]`d.
    //! Run them with:
    //!
    //! ```text
    //! cargo test --lib elements::button::win32_click_tests -- --ignored --test-threads=1
    //! ```

    use crate::elements::{Button, Container, Element, Orientation, Stack};
    use crate::{AureaResult, Window};
    use std::cell::Cell;
    use std::os::raw::c_void;
    use std::rc::Rc;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageA(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
    }

    /// Tells a button to behave as though it had been clicked, which makes it
    /// notify its parent exactly as a real click does.
    const BM_CLICK: u32 = 0x00F5;

    fn click(hwnd: *mut c_void) {
        unsafe { SendMessageA(hwnd, BM_CLICK, 0, 0) };
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn clicking_a_button_runs_its_callback() -> AureaResult<()> {
        let mut window = Window::new("click", 400, 200)?;

        let fired = Rc::new(Cell::new(0));
        let counter = Rc::clone(&fired);
        let button = Button::with_callback("Go", move || counter.set(counter.get() + 1))?;
        let hwnd = button.handle();

        let mut stack = Stack::new(Orientation::Vertical)?;
        stack.add(button)?;
        window.set_content(stack)?;
        window.show();

        click(hwnd);

        assert_eq!(fired.get(), 1, "the click never reached the callback");
        Ok(())
    }

    /// Buttons sit inside nested containers in real layouts, and the notification
    /// has to be forwarded up through each of them.
    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_click_survives_nested_containers() -> AureaResult<()> {
        let mut window = Window::new("nested click", 400, 200)?;

        let fired = Rc::new(Cell::new(false));
        let flag = Rc::clone(&fired);
        let button = Button::with_callback("Go", move || flag.set(true))?;
        let hwnd = button.handle();

        let mut inner = Stack::new(Orientation::Horizontal)?;
        inner.add(button)?;
        let mut outer = Stack::new(Orientation::Vertical)?;
        outer.add(inner)?;
        window.set_content(outer)?;
        window.show();

        click(hwnd);

        assert!(fired.get(), "the click was lost between the containers");
        Ok(())
    }

    /// Several buttons must stay distinguishable: their ids share one 16-bit field
    /// with menu item ids.
    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn each_button_runs_its_own_callback() -> AureaResult<()> {
        let mut window = Window::new("many buttons", 400, 200)?;

        let hits = Rc::new(Cell::new(0usize));
        let mut stack = Stack::new(Orientation::Vertical)?;
        let mut handles = Vec::new();

        for index in 0..5 {
            let counter = Rc::clone(&hits);
            let button = Button::with_callback(&format!("B{index}"), move || {
                // Each button contributes a distinct bit, so a mix-up shows up as
                // the wrong total rather than the right one by luck.
                counter.set(counter.get() | (1 << index));
            })?;
            handles.push(button.handle());
            stack.add(button)?;
        }
        window.set_content(stack)?;
        window.show();

        for hwnd in &handles {
            click(*hwnd);
        }

        assert_eq!(hits.get(), 0b1_1111, "got {:#b}", hits.get());
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod gtk_click_tests {

    //! A real click has to reach the callback, on GTK.
    //!
    //! The Windows counterpart in `native_input.rs` exists because the click path
    //! was broken there while every unit test passed: those call the FFI entry
    //! point directly, which proves the registry works and nothing about the route
    //! a click actually takes. GTK carries the button id as object data rather
    //! than packed into a message word, so it cannot fail the same way — but the
    //! route was untested, which is how the Windows bug survived.
    //!
    //! These need a display and create real native windows, so they are
    //! `#[ignore]`d. Run them with:
    //!
    //! ```text
    //! cargo test --lib elements::button::gtk_click_tests -- --ignored --test-threads=1
    //! ```

    use crate::elements::{Button, Container, Element, Orientation, Stack};
    use crate::{AureaResult, Window};
    use std::cell::Cell;
    use std::os::raw::c_void;
    use std::rc::Rc;

    unsafe extern "C" {
        fn gtk_button_clicked(button: *mut c_void);
    }

    /// Emits the button's `clicked` signal, which is what a real press does once
    /// GTK has decided a press and release both landed on the widget.
    fn click(handle: *mut c_void) {
        unsafe { gtk_button_clicked(handle) };
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn clicking_a_button_runs_its_callback() -> AureaResult<()> {
        let mut window = Window::new("click", 400, 200)?;

        let fired = Rc::new(Cell::new(0));
        let counter = Rc::clone(&fired);
        let button = Button::with_callback("Go", move || counter.set(counter.get() + 1))?;
        let handle = button.handle();

        let mut stack = Stack::new(Orientation::Vertical)?;
        stack.add(button)?;
        window.set_content(stack)?;
        window.show();

        click(handle);

        assert_eq!(fired.get(), 1, "the click never reached the callback");
        Ok(())
    }

    /// Buttons sit inside nested containers in real layouts, and the signal has to
    /// survive the reparenting each container does.
    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_click_survives_nested_containers() -> AureaResult<()> {
        let mut window = Window::new("nested click", 400, 200)?;

        let fired = Rc::new(Cell::new(false));
        let flag = Rc::clone(&fired);
        let button = Button::with_callback("Go", move || flag.set(true))?;
        let handle = button.handle();

        let mut inner = Stack::new(Orientation::Horizontal)?;
        inner.add(button)?;
        let mut outer = Stack::new(Orientation::Vertical)?;
        outer.add(inner)?;
        window.set_content(outer)?;
        window.show();

        click(handle);

        assert!(fired.get(), "the click was lost between the containers");
        Ok(())
    }

    /// Several buttons must stay distinguishable.
    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn each_button_runs_its_own_callback() -> AureaResult<()> {
        let mut window = Window::new("many buttons", 400, 200)?;

        let hits = Rc::new(Cell::new(0usize));
        let mut stack = Stack::new(Orientation::Vertical)?;
        let mut handles = Vec::new();

        for index in 0..5 {
            let counter = Rc::clone(&hits);
            let button = Button::with_callback(&format!("B{index}"), move || {
                // Each button contributes a distinct bit, so a mix-up shows up as
                // the wrong total rather than the right one by luck.
                counter.set(counter.get() | (1 << index));
            })?;
            handles.push(button.handle());
            stack.add(button)?;
        }
        window.set_content(stack)?;
        window.show();

        for handle in &handles {
            click(*handle);
        }

        assert_eq!(hits.get(), 0b1_1111, "got {:#b}", hits.get());
        Ok(())
    }
}
