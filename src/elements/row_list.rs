//! Native list whose rows are any elements.

use super::native::NativeElement;
use super::table::{Handler, call};
use super::traits::Element;
use crate::registry::elements::{
    next_row_list_id, register_row_list_callbacks, unregister_row_list_callbacks,
};
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::any::Any;
use std::boxed::Box as StdBox;
use std::cell::RefCell;
use std::os::raw::{c_int, c_void};
use std::rc::Rc;

/// A scrolling list of rows, one selectable at a time, where each row is an element: a stack of
/// labels and controls, for example. Each row carries an id chosen by the app, which stays with it
/// as rows are added and removed around it.
pub struct RowList {
    handle: NativeElement,
    id: u32,
    rows: Rc<RefCell<Vec<u64>>>,
    elements: Vec<StdBox<dyn Any>>,
    on_select: Handler<Option<u64>>,
    on_activate: Handler<u64>,
}

fn index(at: usize) -> AureaResult<c_int> {
    c_int::try_from(at).map_err(|_| AureaError::ElementOperationFailed)
}

impl RowList {
    /// An empty list.
    pub fn new() -> AureaResult<Self> {
        let id = next_row_list_id();
        let handle = unsafe { ng_platform_create_row_list(id) };
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        let rows: Rc<RefCell<Vec<u64>>> = Rc::default();
        let on_select: Handler<Option<u64>> = Rc::default();
        let on_activate: Handler<u64> = Rc::default();
        let id_at = {
            let rows = Rc::clone(&rows);
            move |index: i32| {
                usize::try_from(index)
                    .ok()
                    .and_then(|i| rows.borrow().get(i).copied())
            }
        };
        let selected = {
            let (id_at, on_select) = (id_at.clone(), Rc::clone(&on_select));
            move |index| call(&on_select, id_at(index))
        };
        let activated = {
            let on_activate = Rc::clone(&on_activate);
            move |index| {
                if let Some(row) = id_at(index) {
                    call(&on_activate, row);
                }
            }
        };
        register_row_list_callbacks(id, selected, activated);
        Ok(Self {
            handle: NativeElement::new(handle),
            id,
            rows,
            elements: Vec::new(),
            on_select,
            on_activate,
        })
    }

    /// Calls `handler` with the row the user selects, or `None` when the selection clears.
    /// Changes made through this API are not reported.
    pub fn on_select(&self, handler: impl Fn(Option<u64>) + 'static) {
        *self.on_select.borrow_mut() = Some(Rc::new(handler));
    }

    /// Calls `handler` with the row the user opens, by double click or Enter.
    pub fn on_activate(&self, handler: impl Fn(u64) + 'static) {
        *self.on_activate.borrow_mut() = Some(Rc::new(handler));
    }

    fn index_of(&self, row: u64) -> AureaResult<usize> {
        self.rows
            .borrow()
            .iter()
            .position(|&r| r == row)
            .ok_or(AureaError::ElementOperationFailed)
    }

    /// Adds `element` as a row at the end. `row` must not be in the list yet.
    pub fn add_row<E: Element + 'static>(&mut self, row: u64, element: E) -> AureaResult<()> {
        let end = self.rows.borrow().len();
        self.insert_row(end, row, element)
    }

    /// Adds `element` as a row at position `index`.
    pub fn insert_row<E: Element + 'static>(
        &mut self,
        at: usize,
        row: u64,
        element: E,
    ) -> AureaResult<()> {
        if self.rows.borrow().contains(&row) || at > self.rows.borrow().len() {
            return Err(AureaError::ElementOperationFailed);
        }
        status("RowList", unsafe {
            ng_platform_row_list_insert(self.handle.handle(), index(at)?, element.handle())
        })?;
        element.released_to_parent();
        self.rows.borrow_mut().insert(at, row);
        self.elements.insert(at, StdBox::new(element));
        Ok(())
    }

    /// Removes `row`; its element is destroyed.
    pub fn remove_row(&mut self, row: u64) -> AureaResult<()> {
        let at = self.index_of(row)?;
        status("RowList", unsafe {
            ng_platform_row_list_remove(self.handle.handle(), index(at)?)
        })?;
        self.rows.borrow_mut().remove(at);
        self.elements.remove(at);
        Ok(())
    }

    /// Removes every row.
    pub fn clear(&mut self) -> AureaResult<()> {
        status("RowList", unsafe {
            ng_platform_row_list_clear(self.handle.handle())
        })?;
        self.rows.borrow_mut().clear();
        self.elements.clear();
        Ok(())
    }

    /// The rows, top to bottom.
    pub fn rows(&self) -> Vec<u64> {
        self.rows.borrow().clone()
    }

    /// The selected row.
    pub fn selected(&self) -> Option<u64> {
        let at = unsafe { ng_platform_row_list_get_selected(self.handle.handle()) };
        usize::try_from(at)
            .ok()
            .and_then(|i| self.rows.borrow().get(i).copied())
    }

    /// Selects `row`, or nothing, and scrolls it into view.
    pub fn set_selected(&mut self, row: Option<u64>) -> AureaResult<()> {
        let at = match row {
            Some(row) => index(self.index_of(row)?)?,
            None => -1,
        };
        status("RowList", unsafe {
            ng_platform_row_list_set_selected(self.handle.handle(), at)
        })
    }
}

impl Element for RowList {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {}
}

impl Drop for RowList {
    fn drop(&mut self) {
        unregister_row_list_callbacks(self.id);
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;
    use crate::elements::{Button, Container, Label, Orientation, Shared, Stack};
    use crate::{AureaResult, Window};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn list_in(window: &mut Window) -> AureaResult<Shared<RowList>> {
        let list = Shared::new(RowList::new()?);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(list.clone(), 1.0)?;
        window.set_content(root)?;
        Ok(list)
    }

    /// A row of a name, a detail and a button; returns the row and its name label.
    fn row(name: &str, detail: &str) -> AureaResult<(Stack, Shared<Label>)> {
        let label = Shared::new(Label::new(name)?);
        let mut row = Stack::new(Orientation::Horizontal)?;
        row.add(label.clone())?;
        row.add_weighted(Label::new(detail)?, 1.0)?;
        row.add(Button::new("Open")?)?;
        Ok((row, label))
    }

    fn settle(window: &Window) {
        for _ in 0..5 {
            window.poll_events();
        }
    }

    #[cfg(windows)]
    mod native {
        use std::os::raw::c_void;
        #[repr(C)]
        #[derive(Default)]
        struct Rect {
            left: i32,
            top: i32,
            right: i32,
            bottom: i32,
        }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetParent(hwnd: *mut c_void) -> *mut c_void;
            fn GetWindowRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
            fn ScreenToClient(hwnd: *mut c_void, point: *mut [i32; 2]) -> i32;
            fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
        }
        const WM_LBUTTONDOWN: u32 = 0x0201;
        const WM_KEYDOWN: u32 = 0x0100;
        const VK_RETURN: usize = 0x0D;
        const VK_DOWN: usize = 0x28;

        /// The row holding `inner`, an element added as a row.
        pub fn row_of(inner: *mut c_void) -> *mut c_void {
            unsafe { GetParent(inner) }
        }

        pub fn click(list: *mut c_void, inner: *mut c_void) {
            let mut r = Rect::default();
            unsafe { GetWindowRect(row_of(inner), &raw mut r) };
            let mut at = [r.left + 2, i32::midpoint(r.top, r.bottom)];
            unsafe { ScreenToClient(list, &raw mut at) };
            let l = (at[0] & 0xFFFF) as isize | (((at[1] & 0xFFFF) as isize) << 16);
            unsafe { SendMessageW(list, WM_LBUTTONDOWN, 1, l) };
        }

        pub fn key_down(list: *mut c_void) {
            unsafe { SendMessageW(list, WM_KEYDOWN, VK_DOWN, 0) };
        }

        pub fn key_enter(list: *mut c_void) {
            unsafe { SendMessageW(list, WM_KEYDOWN, VK_RETURN, 0) };
        }
    }

    #[cfg(target_os = "linux")]
    mod native {
        use std::ffi::CString;
        use std::os::raw::{c_char, c_int, c_void};
        unsafe extern "C" {
            fn gtk_bin_get_child(bin: *mut c_void) -> *mut c_void;
            fn gtk_widget_get_parent(widget: *mut c_void) -> *mut c_void;
            fn gtk_list_box_select_row(list: *mut c_void, row: *mut c_void);
            fn gtk_list_box_get_selected_row(list: *mut c_void) -> *mut c_void;
            fn gtk_list_box_get_row_at_index(list: *mut c_void, index: c_int) -> *mut c_void;
            fn gtk_list_box_row_get_index(row: *mut c_void) -> c_int;
            fn g_signal_emit_by_name(instance: *mut c_void, signal: *const c_char, ...);
        }

        fn list_box(list: *mut c_void) -> *mut c_void {
            unsafe { gtk_bin_get_child(gtk_bin_get_child(list)) }
        }

        pub fn row_of(inner: *mut c_void) -> *mut c_void {
            unsafe { gtk_widget_get_parent(inner) }
        }

        pub fn click(list: *mut c_void, inner: *mut c_void) {
            unsafe { gtk_list_box_select_row(list_box(list), row_of(inner)) };
        }

        pub fn key_down(list: *mut c_void) {
            let b = list_box(list);
            let at = unsafe { gtk_list_box_row_get_index(gtk_list_box_get_selected_row(b)) };
            unsafe { gtk_list_box_select_row(b, gtk_list_box_get_row_at_index(b, at + 1)) };
        }

        pub fn key_enter(list: *mut c_void) {
            let activate = CString::new("activate").unwrap_or_default();
            let row = unsafe { gtk_list_box_get_selected_row(list_box(list)) };
            unsafe { g_signal_emit_by_name(row, activate.as_ptr()) };
        }
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn rows_keep_their_ids_and_elements() -> AureaResult<()> {
        let mut window = Window::new("rows", 500, 300)?;
        let list = list_in(&mut window)?;
        let (a, _) = row("ft4232h", "USB")?;
        let (b, b_label) = row("mcp2221a", "I2C")?;
        let (c, _) = row("ch347", "SPI")?;
        {
            let mut l = list.borrow_mut();
            l.add_row(10, a)?;
            l.add_row(20, b)?;
            l.insert_row(0, 5, c)?;
            assert_eq!(l.rows(), [5, 10, 20]);
        }
        b_label.borrow_mut().set_text("mcp2221a · 한글")?;
        list.borrow_mut().remove_row(20)?;
        assert!(!b_label.is_alive(), "a removed row's element is destroyed");
        assert_eq!(list.borrow().rows(), [5, 10]);
        list.borrow_mut().clear()?;
        assert!(list.borrow().rows().is_empty());
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn bad_rows_are_refused() -> AureaResult<()> {
        let mut window = Window::new("rows refuse", 500, 300)?;
        let list = list_in(&mut window)?;
        let mut l = list.borrow_mut();
        l.add_row(5, row("one", "")?.0)?;
        assert!(l.add_row(5, row("dup", "")?.0).is_err(), "ids are unique");
        assert!(l.insert_row(9, 6, row("past the end", "")?.0).is_err());
        assert!(l.remove_row(99).is_err());
        assert!(l.set_selected(Some(99)).is_err());
        assert_eq!(l.rows(), [5]);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn only_the_users_selection_is_reported() -> AureaResult<()> {
        let mut window = Window::new("rows select", 500, 300)?;
        let list = list_in(&mut window)?;
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&seen);
        list.borrow()
            .on_select(move |row| log.borrow_mut().push(row));
        let mut inner = Vec::new();
        for (id, name) in [(7, "a"), (8, "b"), (9, "c")] {
            let (r, _) = row(name, "")?;
            inner.push(r.handle());
            list.borrow_mut().add_row(id, r)?;
        }
        list.borrow_mut().set_selected(Some(8))?;
        list.borrow_mut().remove_row(7)?;
        assert_eq!(
            list.borrow().selected(),
            Some(8),
            "selection follows its row"
        );
        settle(&window);
        assert!(seen.borrow().is_empty(), "{:?}", seen.borrow());

        let l = list.handle();
        native::click(l, inner[2]);
        settle(&window);
        assert_eq!(seen.borrow().as_slice(), [Some(9)]);
        list.borrow_mut().set_selected(Some(8))?;
        native::key_down(l);
        settle(&window);
        assert_eq!(seen.borrow().as_slice(), [Some(9), Some(9)]);
        assert_eq!(list.borrow().selected(), Some(9));
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn enter_activates_the_selected_row() -> AureaResult<()> {
        let mut window = Window::new("rows activate", 500, 300)?;
        let list = list_in(&mut window)?;
        let opened = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&opened);
        list.borrow()
            .on_activate(move |row| log.borrow_mut().push(row));
        list.borrow_mut().add_row(41, row("first", "")?.0)?;
        list.borrow_mut().add_row(42, row("second", "")?.0)?;
        list.borrow_mut().set_selected(Some(42))?;
        native::key_enter(list.handle());
        settle(&window);
        assert_eq!(opened.borrow().as_slice(), [42]);
        Ok(())
    }
}
