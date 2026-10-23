//! The native element checks the Windows and Linux unit tests make, on AppKit. AppKit only works
//! on the main thread, which libtest never runs tests on, so this binary runs them itself. Like the
//! other native tests they create windows and run only with `--ignored`.

#[cfg(target_os = "macos")]
mod appkit {
    //! Just enough of the Objective-C runtime to read and poke AppKit views.

    use std::ffi::{CStr, CString, c_char, c_void};
    use std::mem::transmute;
    use std::ptr::null_mut;

    pub type Id = *mut c_void;

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Id;
        fn objc_msgSend();
        #[cfg(target_arch = "x86_64")]
        fn objc_msgSend_stret();
    }

    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {}

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateKeyboardEvent(source: Id, key: u16, down: bool) -> Id;
        fn CFRelease(object: Id);
    }

    pub fn class(name: &CStr) -> Id {
        unsafe { objc_getClass(name.as_ptr()) }
    }

    fn sel(name: &CStr) -> Id {
        unsafe { sel_registerName(name.as_ptr()) }
    }

    macro_rules! sender {
        ($name:ident, ($($arg:ident: $ty:ty),*) -> $ret:ty) => {
            pub fn $name(object: Id, selector: &CStr $(, $arg: $ty)*) -> $ret {
                let send: unsafe extern "C" fn(Id, Id $(, $ty)*) -> $ret =
                    unsafe { transmute(objc_msgSend as unsafe extern "C" fn()) };
                unsafe { send(object, sel(selector) $(, $arg)*) }
            }
        };
    }

    sender!(id, () -> Id);
    sender!(id_id, (a: Id) -> Id);
    sender!(id_id_id_int, (a: Id, b: Id, c: isize) -> Id);
    sender!(id_int, (a: isize) -> Id);
    sender!(id_int_int_bool, (a: isize, b: isize, c: bool) -> Id);
    sender!(int, () -> isize);
    sender!(boolean, () -> bool);
    sender!(void_id, (a: Id) -> ());
    sender!(void_id_bool, (a: Id, b: bool) -> ());
    sender!(id_id_float, (a: Id, b: f64) -> Id);
    sender!(id_float4, (a: f64, b: f64, c: f64, d: f64) -> Id);
    sender!(float, () -> f64);
    sender!(boolean_id, (a: Id) -> bool);

    pub fn string(text: Id) -> String {
        if text.is_null() {
            return String::new();
        }
        let utf8 = id(text, c"UTF8String").cast::<c_char>();
        if utf8.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(utf8) }
            .to_string_lossy()
            .into_owned()
    }

    pub fn ns_string(text: &str) -> Id {
        let text = CString::new(text).unwrap_or_default();
        id_id(
            class(c"NSString"),
            c"stringWithUTF8String:",
            text.as_ptr().cast_mut().cast(),
        )
    }

    pub fn document(scroll: Id) -> Id {
        id(scroll, c"documentView")
    }

    /// A real key press, the way the window server delivers it.
    pub fn key(code: u16) -> Id {
        let event = unsafe { CGEventCreateKeyboardEvent(null_mut(), code, true) };
        let wrapped = id_id(class(c"NSEvent"), c"eventWithCGEvent:", event);
        unsafe { CFRelease(event) };
        wrapped
    }

    pub const KEY_RETURN: u16 = 36;
    pub const KEY_DOWN: u16 = 125;

    /// Selects `row` the way a click does: the table reports it to its delegate.
    pub fn user_selects(table: Id, row: usize) {
        let rows = id_int(
            class(c"NSIndexSet"),
            c"indexSetWithIndex:",
            isize::try_from(row).unwrap_or(0),
        );
        void_id_bool(
            table,
            c"selectRowIndexes:byExtendingSelection:",
            rows,
            false,
        );
    }

    pub fn accessibility_label(view: Id) -> String {
        string(id(view, c"accessibilityLabel"))
    }
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    /// The main screen's visible frame, less the menu bar and Dock.
    pub fn visible_size() -> (f64, f64) {
        let screen = id(class(c"NSScreen"), c"mainScreen");
        let selector = sel(c"visibleFrame");
        #[cfg(target_arch = "x86_64")]
        let frame = {
            let mut frame = Rect::default();
            let send: unsafe extern "C" fn(*mut Rect, Id, Id) =
                unsafe { transmute(objc_msgSend_stret as unsafe extern "C" fn()) };
            unsafe { send(&raw mut frame, screen, selector) };
            frame
        };
        #[cfg(not(target_arch = "x86_64"))]
        let frame = {
            let send: unsafe extern "C" fn(Id, Id) -> Rect =
                unsafe { transmute(objc_msgSend as unsafe extern "C" fn()) };
            unsafe { send(screen, selector) }
        };
        (frame.width, frame.height)
    }
}

#[cfg(target_os = "macos")]
mod checks {
    use super::appkit::{self, Id};
    use aurea::elements::{
        Button, Container, Label, Orientation, RowList, ScrollView, ScrollbarPolicy, Shared,
        SidebarList, Slider, Spacer, Stack, Table, TextControl, TextEditor, TextFeatures, TextView,
    };
    use aurea::{AureaError, AureaResult, Element, Window};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn settle(window: &Window) {
        for _ in 0..5 {
            window.poll_events();
        }
    }

    fn recorder<T: 'static>() -> (Rc<RefCell<Vec<T>>>, impl Fn(T) + 'static) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&seen);
        (seen, move |value| log.borrow_mut().push(value))
    }

    fn rooted<E: Element + 'static>(window: &mut Window, element: E) -> AureaResult<Shared<E>> {
        let shared = Shared::new(element);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(shared.clone(), 1.0)?;
        window.set_content(root)?;
        Ok(shared)
    }

    fn row(name: &str, detail: &str) -> AureaResult<(Stack, Shared<Label>)> {
        let label = Shared::new(Label::new(name)?);
        let mut row = Stack::new(Orientation::Horizontal)?;
        row.add(label.clone())?;
        row.add_weighted(Label::new(detail)?, 1.0)?;
        row.add(Button::new("Open")?)?;
        Ok((row, label))
    }

    pub fn rows_keep_their_ids_and_elements() -> AureaResult<()> {
        let mut window = Window::new("rows", 500, 300)?;
        let list = rooted(&mut window, RowList::new()?)?;
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
        let table = appkit::document(list.handle());
        assert_eq!(appkit::int(table, c"numberOfRows"), 3);
        b_label.borrow_mut().set_text("mcp2221a · 한글")?;
        list.borrow_mut().remove_row(20)?;
        assert!(!b_label.is_alive(), "a removed row's element is destroyed");
        assert_eq!(list.borrow().rows(), [5, 10]);
        assert_eq!(appkit::int(table, c"numberOfRows"), 2);
        list.borrow_mut().clear()?;
        assert!(list.borrow().rows().is_empty());
        assert_eq!(appkit::int(table, c"numberOfRows"), 0);
        Ok(())
    }

    pub fn bad_rows_are_refused() -> AureaResult<()> {
        let mut window = Window::new("rows refuse", 500, 300)?;
        let list = rooted(&mut window, RowList::new()?)?;
        let mut l = list.borrow_mut();
        l.add_row(5, row("one", "")?.0)?;
        assert!(l.add_row(5, row("dup", "")?.0).is_err(), "ids are unique");
        assert!(l.insert_row(9, 6, row("past the end", "")?.0).is_err());
        assert!(l.remove_row(99).is_err());
        assert!(l.set_selected(Some(99)).is_err());
        assert_eq!(l.rows(), [5]);
        Ok(())
    }

    pub fn only_the_users_row_selection_is_reported() -> AureaResult<()> {
        let mut window = Window::new("rows select", 500, 300)?;
        let list = rooted(&mut window, RowList::new()?)?;
        let (seen, log) = recorder();
        list.borrow().on_select(log);
        for (id, name) in [(7, "a"), (8, "b"), (9, "c")] {
            list.borrow_mut().add_row(id, row(name, "")?.0)?;
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

        let table = appkit::document(list.handle());
        appkit::user_selects(table, 1);
        settle(&window);
        assert_eq!(seen.borrow().as_slice(), [Some(9)]);
        list.borrow_mut().set_selected(Some(8))?;
        appkit::void_id(table, c"keyDown:", appkit::key(appkit::KEY_DOWN));
        settle(&window);
        assert_eq!(seen.borrow().as_slice(), [Some(9), Some(9)]);
        assert_eq!(list.borrow().selected(), Some(9));
        Ok(())
    }

    pub fn enter_activates_the_selected_row() -> AureaResult<()> {
        let mut window = Window::new("rows activate", 500, 300)?;
        let list = rooted(&mut window, RowList::new()?)?;
        let (opened, log) = recorder();
        list.borrow().on_activate(log);
        list.borrow_mut().add_row(41, row("first", "")?.0)?;
        list.borrow_mut().add_row(42, row("second", "")?.0)?;
        list.borrow_mut().set_selected(Some(42))?;
        let table = appkit::document(list.handle());
        appkit::void_id(table, c"keyDown:", appkit::key(appkit::KEY_RETURN));
        settle(&window);
        assert_eq!(opened.borrow().as_slice(), [42]);
        Ok(())
    }

    pub fn voiceover_reads_a_row_as_its_labels() -> AureaResult<()> {
        let mut window = Window::new("rows voiceover", 500, 300)?;
        let list = rooted(&mut window, RowList::new()?)?;
        let mut stack = Stack::new(Orientation::Horizontal)?;
        stack.add(Label::new("ft4232h")?)?;
        stack.add(Spacer::new()?)?;
        stack.add(Label::new("4 channels")?)?;
        list.borrow_mut().add_row(1, stack)?;
        settle(&window);
        let table = appkit::document(list.handle());
        let cell = appkit::id_int_int_bool(table, c"viewAtColumn:row:makeIfNecessary:", 0, 0, true);
        assert_eq!(appkit::accessibility_label(cell), "ft4232h, 4 channels");
        Ok(())
    }

    fn cell(table: Id, row: usize, column: usize) -> String {
        let columns = appkit::id(table, c"tableColumns");
        let column = appkit::id_int(
            columns,
            c"objectAtIndex:",
            isize::try_from(column).unwrap_or(0),
        );
        appkit::string(appkit::id_id_id_int(
            appkit::id(table, c"dataSource"),
            c"tableView:objectValueForTableColumn:row:",
            table,
            column,
            isize::try_from(row).unwrap_or(0),
        ))
    }

    pub fn table_rows_keep_their_ids_and_cells() -> AureaResult<()> {
        let mut window = Window::new("table", 500, 300)?;
        let table = rooted(&mut window, Table::new(["Name", "Bus", "State"])?)?;
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
        let view = appkit::document(table.handle());
        assert_eq!(appkit::int(view, c"numberOfRows"), 2);
        assert_eq!(cell(view, 0, 0), "ch347");
        assert_eq!(cell(view, 1, 1), "I2C");
        assert_eq!(cell(view, 1, 2), "한글 ready");
        assert_eq!(cell(view, 1, 0), "mcp2221a");
        table.borrow_mut().clear()?;
        assert_eq!(appkit::int(view, c"numberOfRows"), 0);
        Ok(())
    }

    pub fn only_the_users_table_selection_is_reported() -> AureaResult<()> {
        let mut window = Window::new("table select", 500, 300)?;
        let table = rooted(&mut window, Table::new(["Name"])?)?;
        let (seen, log) = recorder();
        table.borrow().on_select(log);
        let (opened, log) = recorder();
        table.borrow().on_activate(log);
        for (id, name) in [(7, "a"), (8, "b"), (9, "c")] {
            table.borrow_mut().add_row(id, &[name])?;
        }
        table.borrow_mut().set_selected(Some(8))?;
        table.borrow_mut().remove_row(7)?;
        assert_eq!(table.borrow().selected(), Some(8));
        settle(&window);
        assert!(seen.borrow().is_empty(), "{:?}", seen.borrow());
        let view = appkit::document(table.handle());
        appkit::user_selects(view, 1);
        settle(&window);
        assert_eq!(seen.borrow().as_slice(), [Some(9)]);
        appkit::void_id(view, c"keyDown:", appkit::key(appkit::KEY_RETURN));
        settle(&window);
        assert_eq!(opened.borrow().as_slice(), [9]);
        Ok(())
    }

    /// Types into the editor's text view as the user would.
    fn type_text(editor: &impl Element, text: &str) {
        appkit::void_id(
            appkit::document(editor.handle()),
            c"insertText:",
            appkit::ns_string(text),
        );
    }

    pub fn text_selection_and_replace_work_in_bytes() -> AureaResult<()> {
        let mut window = Window::new("select", 400, 300)?;
        let editor = rooted(&mut window, TextEditor::new()?)?;
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

    pub fn text_edits_undo_and_redo() -> AureaResult<()> {
        let mut window = Window::new("undo", 400, 300)?;
        let editor = rooted(&mut window, TextEditor::new()?)?;
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

    pub fn only_the_users_edits_are_reported() -> AureaResult<()> {
        let mut window = Window::new("changes", 400, 300)?;
        let editor = rooted(&mut window, TextEditor::new()?)?;
        let (seen, log) = recorder();
        editor.borrow().on_change(move |c| log(c.clone()))?;
        editor.borrow().set_text("ab")?;
        editor.borrow().replace(2..2, "c")?;
        assert!(seen.borrow().is_empty());
        let before = editor.borrow().revision();
        editor.borrow().set_selection(3..3)?;
        type_text(&*editor.borrow(), "한");
        let seen = seen.borrow();
        assert_eq!(seen.len(), 1, "{:?}", seen.len());
        assert_eq!(
            (seen[0].range.clone(), seen[0].inserted.as_str()),
            (3..3, "한")
        );
        assert!(seen[0].revision > before);
        Ok(())
    }

    pub fn text_features_switch_at_runtime() -> AureaResult<()> {
        let mut window = Window::new("features", 400, 300)?;
        let editor = rooted(&mut window, TextEditor::new()?)?;
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

    pub fn a_scroll_bar_follows_its_policy() -> AureaResult<()> {
        let _window = Window::new("scroll policy", 300, 200)?;
        let mut scroll = ScrollView::with_content(Label::new("short")?)?;
        let autohides =
            |scroll: &ScrollView| appkit::boolean(scroll.handle(), c"autohidesScrollers");
        assert!(autohides(&scroll));
        scroll.set_scrollbar(ScrollbarPolicy::Always)?;
        assert!(!autohides(&scroll));
        assert_eq!(
            appkit::int(scroll.handle(), c"scrollerStyle"),
            0,
            "legacy scrollers"
        );
        scroll.set_scrollbar(ScrollbarPolicy::Auto)?;
        assert!(autohides(&scroll));
        Ok(())
    }

    pub fn an_element_takes_an_accessible_name() -> AureaResult<()> {
        let _window = Window::new("names", 300, 200)?;
        let button = Button::new("+")?;
        button.set_accessible_name("New tab")?;
        assert_eq!(appkit::accessibility_label(button.handle()), "New tab");
        let editor = TextEditor::new()?;
        editor.set_accessible_name("Source")?;
        assert_eq!(
            appkit::accessibility_label(appkit::document(editor.handle())),
            "Source"
        );
        let table = Table::new(["Adapter"])?;
        table.set_accessible_name("Adapters")?;
        assert_eq!(
            appkit::accessibility_label(appkit::document(table.handle())),
            "Adapters"
        );
        let rows = RowList::new()?;
        rows.set_accessible_name("Buses")?;
        assert_eq!(
            appkit::accessibility_label(appkit::document(rows.handle())),
            "Buses"
        );
        let sidebar = SidebarList::new()?;
        sidebar.set_accessible_name("Places")?;
        assert_eq!(appkit::accessibility_label(sidebar.handle()), "Places");
        Ok(())
    }

    pub fn voiceover_skips_empty_labels() -> AureaResult<()> {
        let _window = Window::new("spacers", 300, 200)?;
        let element = |view: Id| appkit::boolean(view, c"isAccessibilityElement");
        let spacer = Spacer::new()?;
        assert!(!element(spacer.handle()));
        let mut label = Label::new("")?;
        label.set_text("shown")?;
        assert!(element(label.handle()));
        label.set_text("")?;
        assert!(!element(label.handle()));
        Ok(())
    }

    pub fn a_slider_reads_its_value() -> AureaResult<()> {
        let mut window = Window::new("slider", 300, 200)?;
        let slider = rooted(&mut window, Slider::new(512.0, 16384.0)?)?;
        slider.borrow_mut().set_value(4096.0)?;
        // AppKit exposes a slider to VoiceOver through its cell.
        let value = appkit::id(appkit::id(slider.handle(), c"cell"), c"accessibilityValue");
        let shown = appkit::string(appkit::id(value, c"description"));
        assert_eq!(shown, "4096");
        Ok(())
    }

    pub fn an_oversized_window_fits_the_screen() -> AureaResult<()> {
        let window = Window::new("oversized", 20_000, 20_000)?;
        settle(&window);
        let (w, h) = window.size();
        let (sw, sh) = appkit::visible_size();
        assert!(
            f64::from(w) <= sw && f64::from(h) <= sh,
            "{w}x{h} in {sw}x{sh}"
        );
        Ok(())
    }

    pub fn wrapped_labels_keep_the_window_height() -> AureaResult<()> {
        const LONG: &str = "Permission is hereby granted, free of charge, to any person obtaining a copy \
                            of this software and its documentation files.";
        let mut window = Window::new("labels", 600, 300)?;
        let mut root = Stack::new(Orientation::Vertical)?;
        let mut later = Label::new("short")?;
        later.set_text(LONG)?;
        root.add(later)?;
        for _ in 0..2 {
            root.add(Label::new(LONG)?)?;
        }
        window.set_content(root)?;
        settle(&window);
        assert_eq!(window.size(), (600, 300));
        Ok(())
    }

    pub fn a_styled_text_view_keeps_its_look_when_attached() -> AureaResult<()> {
        let mut window = Window::new("text look", 400, 300)?;
        let view = TextView::with_content(false, "light on dark")?;
        let text = appkit::document(view.handle());
        let font = appkit::id_id_float(
            appkit::class(c"NSFont"),
            c"fontWithName:size:",
            appkit::ns_string("Menlo"),
            15.0,
        );
        let color = appkit::id_float4(
            appkit::class(c"NSColor"),
            c"colorWithSRGBRed:green:blue:alpha:",
            0.84,
            0.89,
            0.93,
            1.0,
        );
        appkit::void_id(text, c"setFont:", font);
        appkit::void_id(text, c"setTextColor:", color);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(view, 1.0)?;
        window.set_content(root)?;
        settle(&window);
        let now = appkit::id(text, c"font");
        assert_eq!(appkit::string(appkit::id(now, c"familyName")), "Menlo");
        assert!((appkit::float(now, c"pointSize") - 15.0).abs() < f64::EPSILON);
        assert!(appkit::boolean_id(
            appkit::id(text, c"textColor"),
            c"isEqual:",
            color
        ));
        Ok(())
    }

    pub fn a_window_keeps_its_minimum_size() -> AureaResult<()> {
        let window = Window::new("min size", 400, 300)?;
        window.set_min_size(501, 351)?;
        settle(&window);
        assert_eq!(window.size(), (501, 351));
        window.set_size(100, 100);
        settle(&window);
        assert_eq!(window.size(), (501, 351));
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() {
    use aurea::AureaResult;
    use std::env;
    use std::panic::catch_unwind;
    use std::process::exit;

    type Check = fn() -> AureaResult<()>;

    macro_rules! checks {
        ($($name:ident),* $(,)?) => {
            &[$((stringify!($name), checks::$name as Check)),*]
        };
    }

    let args: Vec<String> = env::args().skip(1).collect();
    let run_ignored = args
        .iter()
        .any(|a| a == "--ignored" || a == "--include-ignored");
    let filter = args.iter().find(|a| !a.starts_with('-'));
    let tests: &[(&str, Check)] = checks![
        rows_keep_their_ids_and_elements,
        bad_rows_are_refused,
        only_the_users_row_selection_is_reported,
        enter_activates_the_selected_row,
        voiceover_reads_a_row_as_its_labels,
        table_rows_keep_their_ids_and_cells,
        only_the_users_table_selection_is_reported,
        text_selection_and_replace_work_in_bytes,
        text_edits_undo_and_redo,
        only_the_users_edits_are_reported,
        text_features_switch_at_runtime,
        a_scroll_bar_follows_its_policy,
        an_element_takes_an_accessible_name,
        voiceover_skips_empty_labels,
        a_slider_reads_its_value,
        an_oversized_window_fits_the_screen,
        wrapped_labels_keep_the_window_height,
        a_window_keeps_its_minimum_size,
        a_styled_text_view_keeps_its_look_when_attached,
    ];
    let mut failed = Vec::new();
    for (name, test) in tests {
        if filter.is_some_and(|f| !name.contains(f.as_str())) {
            continue;
        }
        if !run_ignored {
            println!("test {name} ... ignored");
            continue;
        }
        let outcome = catch_unwind(test);
        match outcome {
            Ok(Ok(())) => println!("test {name} ... ok"),
            Ok(Err(error)) => {
                println!("test {name} ... FAILED: {error:?}");
                failed.push(*name);
            }
            Err(_) => {
                println!("test {name} ... FAILED");
                failed.push(*name);
            }
        }
    }
    if !failed.is_empty() {
        println!("failures: {failed:?}");
        exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
