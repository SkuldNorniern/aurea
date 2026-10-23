//! Native table of text rows under column headings.

use super::native::NativeElement;
use super::traits::Element;
use crate::ffi::{c_text, c_title};
use crate::registry::elements::{
    next_table_id, register_table_callbacks, unregister_table_callbacks,
};
use crate::render::Rect;
use crate::{AureaError, AureaResult, ffi::*};
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::rc::Rc;

pub(super) type Handler<T> = Rc<RefCell<Option<Rc<dyn Fn(T)>>>>;

/// Rows of text under column headings, one selectable at a time. Each row carries an id chosen by
/// the app, which stays with it as rows are added and removed around it.
pub struct Table {
    handle: NativeElement,
    id: u32,
    columns: usize,
    rows: Rc<RefCell<Vec<u64>>>,
    on_select: Handler<Option<u64>>,
    on_activate: Handler<u64>,
}

fn check(result: c_int) -> AureaResult<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(AureaError::ElementOperationFailed)
    }
}

pub(super) fn call<T>(handler: &Handler<T>, value: T) {
    let current = handler.borrow().clone();
    if let Some(handler) = current {
        handler(value);
    }
}

impl Table {
    /// A table with these column headings and no rows.
    pub fn new<I, S>(columns: I) -> AureaResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let id = next_table_id();
        let handle = unsafe { ng_platform_create_table(id) };
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
        register_table_callbacks(id, selected, activated);
        let mut table = Self {
            handle: NativeElement::new(handle),
            id,
            columns: 0,
            rows,
            on_select,
            on_activate,
        };
        for column in columns {
            let title = c_title(column.as_ref())?;
            check(unsafe { ng_platform_table_add_column(handle, title.as_ptr()) })?;
            table.columns += 1;
        }
        if table.columns == 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(table)
    }

    /// Calls `handler` with the row the user selects, or `None` when they clear the selection.
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

    fn cells<S: AsRef<str>>(&self, cells: &[S]) -> AureaResult<Vec<CString>> {
        if cells.len() > self.columns {
            return Err(AureaError::ElementOperationFailed);
        }
        cells.iter().map(|c| c_text(c.as_ref())).collect()
    }

    fn send<S: AsRef<str>>(
        &self,
        index: usize,
        cells: &[S],
        op: unsafe extern "C" fn(*mut c_void, c_int, *const *const c_char, c_int) -> c_int,
    ) -> AureaResult<()> {
        let owned = self.cells(cells)?;
        let pointers: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        let index = c_int::try_from(index).map_err(|_| AureaError::ElementOperationFailed)?;
        let count =
            c_int::try_from(pointers.len()).map_err(|_| AureaError::ElementOperationFailed)?;
        check(unsafe { op(self.handle.handle(), index, pointers.as_ptr(), count) })
    }

    /// Adds a row at the end; missing cells are empty. `row` must not be in the table yet.
    pub fn add_row<S: AsRef<str>>(&mut self, row: u64, cells: &[S]) -> AureaResult<()> {
        let end = self.rows.borrow().len();
        self.insert_row(end, row, cells)
    }

    /// Adds a row at position `index`.
    pub fn insert_row<S: AsRef<str>>(
        &mut self,
        index: usize,
        row: u64,
        cells: &[S],
    ) -> AureaResult<()> {
        if self.rows.borrow().contains(&row) || index > self.rows.borrow().len() {
            return Err(AureaError::ElementOperationFailed);
        }
        self.send(index, cells, ng_platform_table_insert_row)?;
        self.rows.borrow_mut().insert(index, row);
        Ok(())
    }

    /// Replaces the cells of `row`.
    pub fn set_row<S: AsRef<str>>(&mut self, row: u64, cells: &[S]) -> AureaResult<()> {
        let index = self.index_of(row)?;
        self.send(index, cells, ng_platform_table_set_row)
    }

    /// Removes `row`.
    pub fn remove_row(&mut self, row: u64) -> AureaResult<()> {
        let index = self.index_of(row)?;
        let at = c_int::try_from(index).map_err(|_| AureaError::ElementOperationFailed)?;
        check(unsafe { ng_platform_table_remove_row(self.handle.handle(), at) })?;
        self.rows.borrow_mut().remove(index);
        Ok(())
    }

    /// Removes every row.
    pub fn clear(&mut self) -> AureaResult<()> {
        check(unsafe { ng_platform_table_clear(self.handle.handle()) })?;
        self.rows.borrow_mut().clear();
        Ok(())
    }

    /// The rows, top to bottom.
    pub fn rows(&self) -> Vec<u64> {
        self.rows.borrow().clone()
    }

    /// The selected row.
    pub fn selected(&self) -> Option<u64> {
        let index = unsafe { ng_platform_table_get_selected(self.handle.handle()) };
        usize::try_from(index)
            .ok()
            .and_then(|i| self.rows.borrow().get(i).copied())
    }

    /// Selects `row`, or nothing, and scrolls it into view.
    pub fn set_selected(&mut self, row: Option<u64>) -> AureaResult<()> {
        let index = match row {
            Some(row) => c_int::try_from(self.index_of(row)?)
                .map_err(|_| AureaError::ElementOperationFailed)?,
            None => -1,
        };
        check(unsafe { ng_platform_table_set_selected(self.handle.handle(), index) })
    }
}

impl Element for Table {
    fn handle(&self) -> *mut c_void {
        self.handle.handle()
    }

    fn released_to_parent(&self) {
        self.handle.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {}
}

impl Drop for Table {
    fn drop(&mut self) {
        unregister_table_callbacks(self.id);
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;
    use crate::elements::{Container, Orientation, Shared, Stack};
    use crate::{AureaResult, Window};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn table_in(window: &mut Window) -> AureaResult<Shared<Table>> {
        let table = Shared::new(Table::new(["Name", "Bus", "State"])?);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(table.clone(), 1.0)?;
        window.set_content(root)?;
        Ok(table)
    }

    fn settle(window: &Window) {
        for _ in 0..5 {
            window.poll_events();
        }
    }

    #[cfg(windows)]
    mod native {
        use std::os::raw::c_void;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetWindow(hwnd: *mut c_void, cmd: u32) -> *mut c_void;
            fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
        }
        #[repr(C)]
        #[derive(Default)]
        struct Item {
            mask: u32,
            item: i32,
            sub_item: i32,
            state: u32,
            state_mask: u32,
            text: usize,
            text_max: i32,
            rest: [usize; 6],
        }
        const LVM_GETITEMCOUNT: u32 = 0x1000 + 4;
        const LVM_SETITEMSTATE: u32 = 0x1000 + 43;
        const LVM_GETITEMTEXTW: u32 = 0x1000 + 115;
        const LVIS_SELECTED: u32 = 2;
        const WM_KEYDOWN: u32 = 0x0100;
        const VK_RETURN: usize = 0x0D;
        const GW_CHILD: u32 = 5;

        fn list(table: *mut c_void) -> *mut c_void {
            unsafe { GetWindow(table, GW_CHILD) }
        }

        pub fn count(table: *mut c_void) -> usize {
            usize::try_from(unsafe { SendMessageW(list(table), LVM_GETITEMCOUNT, 0, 0) })
                .unwrap_or(0)
        }

        pub fn cell(table: *mut c_void, row: usize, column: i32) -> String {
            let mut buffer = [0u16; 128];
            let mut item = Item {
                sub_item: column,
                text: buffer.as_mut_ptr() as usize,
                text_max: 128,
                ..Item::default()
            };
            let len = unsafe {
                SendMessageW(list(table), LVM_GETITEMTEXTW, row, (&raw mut item) as isize)
            };
            String::from_utf16_lossy(&buffer[..usize::try_from(len).unwrap_or(0)])
        }

        pub fn user_selects(table: *mut c_void, row: usize) {
            let mut item = Item {
                state: LVIS_SELECTED,
                state_mask: LVIS_SELECTED,
                ..Item::default()
            };
            unsafe { SendMessageW(list(table), LVM_SETITEMSTATE, row, (&raw mut item) as isize) };
        }

        pub fn user_activates(table: *mut c_void, _row: usize) {
            unsafe { SendMessageW(list(table), WM_KEYDOWN, VK_RETURN, 0) };
        }
    }

    #[cfg(target_os = "linux")]
    mod native {
        use std::ffi::CStr;
        use std::os::raw::{c_char, c_int, c_void};
        use std::ptr::null_mut;
        unsafe extern "C" {
            fn gtk_bin_get_child(bin: *mut c_void) -> *mut c_void;
            fn gtk_tree_view_get_model(view: *mut c_void) -> *mut c_void;
            fn gtk_tree_view_get_selection(view: *mut c_void) -> *mut c_void;
            fn gtk_tree_view_get_column(view: *mut c_void, n: c_int) -> *mut c_void;
            fn gtk_tree_view_row_activated(
                view: *mut c_void,
                path: *mut c_void,
                column: *mut c_void,
            );
            fn gtk_tree_selection_select_path(selection: *mut c_void, path: *mut c_void);
            fn gtk_tree_model_iter_n_children(model: *mut c_void, iter: *mut c_void) -> c_int;
            fn gtk_tree_model_iter_nth_child(
                model: *mut c_void,
                iter: *mut [usize; 4],
                parent: *mut c_void,
                n: c_int,
            ) -> c_int;
            fn gtk_tree_model_get(model: *mut c_void, iter: *mut [usize; 4], ...);
            fn gtk_tree_path_new_from_indices(first: c_int, ...) -> *mut c_void;
            fn gtk_tree_path_free(path: *mut c_void);
            fn g_free(p: *mut c_void);
        }

        fn view(table: *mut c_void) -> *mut c_void {
            unsafe { gtk_bin_get_child(table) }
        }

        fn int(row: usize) -> c_int {
            c_int::try_from(row).unwrap_or(c_int::MAX)
        }

        pub fn count(table: *mut c_void) -> usize {
            let model = unsafe { gtk_tree_view_get_model(view(table)) };
            if model.is_null() {
                return 0;
            }
            usize::try_from(unsafe { gtk_tree_model_iter_n_children(model, null_mut()) })
                .unwrap_or(0)
        }

        pub fn cell(table: *mut c_void, row: usize, column: i32) -> String {
            let model = unsafe { gtk_tree_view_get_model(view(table)) };
            let mut iter = [0usize; 4];
            let mut text: *mut c_char = null_mut();
            unsafe {
                gtk_tree_model_iter_nth_child(model, &raw mut iter, null_mut(), int(row));
                gtk_tree_model_get(model, &raw mut iter, column, &raw mut text, -1);
                let read = CStr::from_ptr(text).to_string_lossy().into_owned();
                g_free(text.cast());
                read
            }
        }

        fn with_path(row: usize, f: impl FnOnce(*mut c_void)) {
            let path = unsafe { gtk_tree_path_new_from_indices(int(row), -1) };
            f(path);
            unsafe { gtk_tree_path_free(path) };
        }

        pub fn user_selects(table: *mut c_void, row: usize) {
            with_path(row, |path| unsafe {
                gtk_tree_selection_select_path(gtk_tree_view_get_selection(view(table)), path);
            });
        }

        pub fn user_activates(table: *mut c_void, row: usize) {
            with_path(row, |path| unsafe {
                let v = view(table);
                gtk_tree_view_row_activated(v, path, gtk_tree_view_get_column(v, 0));
            });
        }
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn rows_keep_their_ids_and_cells() -> AureaResult<()> {
        let mut window = Window::new("table", 500, 300)?;
        let table = table_in(&mut window)?;
        let h = table.handle();
        {
            let mut t = table.borrow_mut();
            t.add_row(10, &["ft4232h", "USB", "ready"])?;
            t.add_row(20, &["mcp2221a", "I2C"])?;
            t.insert_row(0, 5, &["ch347", "SPI", "busy"])?;
            assert_eq!(t.rows(), [5, 10, 20]);
            t.set_row(20, &["mcp2221a", "I2C", "한글 ready"])?;
            t.remove_row(10)?;
            assert_eq!(t.rows(), [5, 20]);
        }
        assert_eq!(native::count(h), 2);
        assert_eq!(native::cell(h, 0, 0), "ch347");
        assert_eq!(native::cell(h, 1, 1), "I2C");
        assert_eq!(native::cell(h, 1, 2), "한글 ready");
        table.borrow_mut().clear()?;
        assert_eq!(native::count(h), 0);
        assert!(table.borrow().rows().is_empty());
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn bad_rows_are_refused() -> AureaResult<()> {
        let mut window = Window::new("table refuse", 500, 300)?;
        let table = table_in(&mut window)?;
        let mut t = table.borrow_mut();
        t.add_row(5, &["one"])?;
        assert!(t.add_row(5, &["dup"]).is_err(), "ids are unique");
        assert!(t.remove_row(99).is_err());
        assert!(t.set_selected(Some(99)).is_err());
        assert!(t.add_row(30, &["a", "b", "c", "too many"]).is_err());
        assert!(t.insert_row(9, 31, &["past the end"]).is_err());
        assert!(
            Table::new(Vec::<&str>::new()).is_err(),
            "a table needs a column"
        );
        assert_eq!(t.rows(), [5]);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn only_the_users_selection_is_reported() -> AureaResult<()> {
        let mut window = Window::new("table select", 500, 300)?;
        let table = table_in(&mut window)?;
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&seen);
        table
            .borrow()
            .on_select(move |row| log.borrow_mut().push(row));
        {
            let mut t = table.borrow_mut();
            for (id, name) in [(7, "a"), (8, "b"), (9, "c")] {
                t.add_row(id, &[name])?;
            }
            t.set_selected(Some(8))?;
            assert_eq!(t.selected(), Some(8));
            t.remove_row(7)?;
            assert_eq!(t.selected(), Some(8), "selection follows its row");
        }
        settle(&window);
        assert!(seen.borrow().is_empty(), "{:?}", seen.borrow());

        native::user_selects(table.handle(), 1);
        settle(&window);
        assert_eq!(seen.borrow().as_slice(), [Some(9)]);
        assert_eq!(table.borrow().selected(), Some(9));
        table.borrow_mut().set_selected(None)?;
        assert_eq!(table.borrow().selected(), None);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn activating_a_row_reports_its_id() -> AureaResult<()> {
        let mut window = Window::new("table activate", 500, 300)?;
        let table = table_in(&mut window)?;
        let opened = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&opened);
        table
            .borrow()
            .on_activate(move |row| log.borrow_mut().push(row));
        table.borrow_mut().add_row(41, &["first"])?;
        table.borrow_mut().add_row(42, &["second"])?;
        table.borrow_mut().set_selected(Some(42))?;
        native::user_activates(table.handle(), 1);
        settle(&window);
        assert_eq!(opened.borrow().as_slice(), [42]);
        Ok(())
    }
}
