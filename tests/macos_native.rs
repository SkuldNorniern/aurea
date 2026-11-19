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

    /// A rect property such as a view's `frame`, as x, y, width, height.
    pub fn rect(object: Id, selector: &CStr) -> (f64, f64, f64, f64) {
        let selector = sel(selector);
        #[cfg(target_arch = "x86_64")]
        let frame = {
            let mut frame = Rect::default();
            let send: unsafe extern "C" fn(*mut Rect, Id, Id) =
                unsafe { transmute(objc_msgSend_stret as unsafe extern "C" fn()) };
            unsafe { send(&raw mut frame, object, selector) };
            frame
        };
        #[cfg(not(target_arch = "x86_64"))]
        let frame = {
            let send: unsafe extern "C" fn(Id, Id) -> Rect =
                unsafe { transmute(objc_msgSend as unsafe extern "C" fn()) };
            unsafe { send(object, selector) }
        };
        (frame.x, frame.y, frame.width, frame.height)
    }

    /// The main screen's visible frame, less the menu bar and Dock.
    pub fn visible_size() -> (f64, f64) {
        let (_, _, width, height) = rect(id(class(c"NSScreen"), c"mainScreen"), c"visibleFrame");
        (width, height)
    }
}

#[cfg(target_os = "macos")]
mod checks {
    use super::appkit::{self, Id};
    use aurea::elements::{
        Button, Checkbox, Container, Label, Orientation, RowList, ScrollView, ScrollbarPolicy,
        Shared, SidebarList, Slider, Spacer, Stack, Table, TextControl, TextEditor, TextFeatures,
        TextView,
    };
    use aurea::render::Image;
    use aurea::{AureaError, AureaResult, CursorIcon, Element, RadioGroup, Window};
    use std::cell::RefCell;
    use std::rc::Rc;

    pub fn menus_and_cursors_survive_being_rebuilt() -> AureaResult<()> {
        let mut window = Window::new("menus", 320, 240)?;
        let root = window.create_menu_bar()?;
        let mut menu = root.add_submenu("Layers")?;
        for _ in 0..300 {
            let mut child = menu.add_submenu("Adjustment")?;
            let mut nested = child.add_submenu("More")?;
            let item = nested.add_item("Levels", || {})?;
            menu.clear()?;
            assert!(child.add_separator().is_err());
            assert!(nested.clear().is_err());
            assert!(item.set_enabled(false).is_err());
        }
        let image = Image::new(24, 24, [255, 255, 255, 128].repeat(24 * 24));
        for _ in 0..300 {
            window.set_cursor_image(&image, (12, 12))?;
            window.set_cursor(CursorIcon::ResizeNwSe)?;
            window.set_cursor(CursorIcon::ResizeNeSw)?;
        }
        Ok(())
    }

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

    pub fn a_window_with_a_filling_canvas_shrinks_both_ways() -> AureaResult<()> {
        use aurea::render::{Canvas, RendererBackend};
        let mut window = Window::new("shrink", 760, 370)?;
        let mut root = Stack::new(Orientation::Vertical)?;
        root.set_padding(0)?;
        root.add(Canvas::new(760, 370, RendererBackend::Cpu)?)?;
        window.set_content(root)?;
        settle(&window);
        window.set_size(400, 200);
        settle(&window);
        assert_eq!(window.size(), (400, 200), "its first size is no floor");
        window.set_size(900, 500);
        settle(&window);
        assert_eq!(window.size(), (900, 500));
        Ok(())
    }

    pub fn the_input_method_can_be_kept_out_and_let_back() -> AureaResult<()> {
        let window = Window::new("ime", 300, 200)?;
        settle(&window);
        let content = appkit::id(window.handle() as appkit::Id, c"contentView");
        assert!(
            !appkit::id(content, c"inputContext").is_null(),
            "on by default"
        );
        window.set_ime_allowed(false)?;
        assert!(appkit::id(content, c"inputContext").is_null(), "kept out");
        window.set_ime_allowed(true)?;
        assert!(!appkit::id(content, c"inputContext").is_null(), "let back");
        Ok(())
    }

    pub fn radios_and_ticks_keep_their_size_in_a_stack() -> AureaResult<()> {
        let mut window = Window::new("radios", 420, 340)?;
        let mut root = Stack::new(Orientation::Vertical)?;
        let group = RadioGroup::new(&["Low", "Medium", "High"])?;
        let radios = appkit::id(group.handle(), c"arrangedSubviews");
        root.add(group)?;
        let tick = Checkbox::new("Share usage data")?;
        let tick_view = tick.handle();
        root.add(tick)?;
        window.set_content(root)?;
        settle(&window);
        // the stack has room to spare: none of it may go to stretching a radio or a tick
        let views = (0..3)
            .map(|i| appkit::id_int(radios, c"objectAtIndex:", i))
            .chain([tick_view]);
        for view in views {
            let (_, _, _, height) = appkit::rect(view, c"frame");
            let label = appkit::string(appkit::id(view, c"title"));
            assert!(
                height < 30.0,
                "{label} is {height} tall: stretched or wrapped"
            );
        }
        Ok(())
    }

    // The window checks the Windows and Linux unit tests make, under the same names.

    pub fn a_container_takes_ownership_of_what_it_is_given() -> AureaResult<()> {
        let _window = Window::new("ownership", 200, 100)?;
        let loose = Label::new("loose")?;
        assert!(!loose.handle().is_null());
        drop(loose);
        let mut stack = Stack::new(Orientation::Vertical)?;
        let adopted = Button::new("adopted")?;
        let handle = adopted.handle();
        stack.add(adopted)?;
        assert!(!stack.handle().is_null());
        assert!(!handle.is_null());
        drop(stack);
        Ok(())
    }

    pub fn ownership_survives_repetition() -> AureaResult<()> {
        let mut window = Window::new("stress", 300, 200)?;
        for round in 0..200 {
            drop(Label::new("loose")?);
            let mut stack = Stack::new(Orientation::Vertical)?;
            stack.add(Button::new("adopted")?)?;
            stack.add(Label::new("also adopted")?)?;
            drop(stack);
            let mut outer = Stack::new(Orientation::Vertical)?;
            let mut inner = Stack::new(Orientation::Horizontal)?;
            inner.add(Label::new("deep")?)?;
            outer.add(inner)?;
            drop(outer);
            let mut content = Stack::new(Orientation::Vertical)?;
            content.add(Label::new(&format!("round {round}"))?)?;
            window.set_content(content)?;
        }
        window.set_title("done")?;
        Ok(())
    }

    pub fn wrapper_elements_forward_ownership() -> AureaResult<()> {
        use aurea::elements::Divider;
        let _window = Window::new("wrappers", 300, 200)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        stack.add(Spacer::new()?)?;
        stack.add(Divider::horizontal(200)?)?;
        drop(stack);
        Ok(())
    }

    pub fn dividers_and_spacers_can_be_created() -> AureaResult<()> {
        use aurea::elements::Divider;
        let _window = Window::new("dividers", 200, 100)?;
        assert!(Divider::horizontal(100).is_ok());
        assert!(Divider::vertical(50).is_ok());
        assert!(Spacer::new().is_ok());
        Ok(())
    }

    pub fn images_report_their_upright_size() -> AureaResult<()> {
        use aurea::elements::ImageView;
        const PNG_3X2: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 3, 0, 0, 0, 2,
            8, 2, 0, 0, 0, 18, 22, 241, 77, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 96, 192, 4,
            0, 0, 20, 0, 1, 238, 90, 105, 9, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        const JPEG_4X2_TURNED: &[u8] = &[
            255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 225, 0,
            34, 69, 120, 105, 102, 0, 0, 77, 77, 0, 42, 0, 0, 0, 8, 0, 1, 1, 18, 0, 3, 0, 0, 0, 1,
            0, 6, 0, 0, 0, 0, 0, 0, 255, 219, 0, 67, 0, 16, 11, 12, 14, 12, 10, 16, 14, 13, 14, 18,
            17, 16, 19, 24, 40, 26, 24, 22, 22, 24, 49, 35, 37, 29, 40, 58, 51, 61, 60, 57, 51, 56,
            55, 64, 72, 92, 78, 64, 68, 87, 69, 55, 56, 80, 109, 81, 87, 95, 98, 103, 104, 103, 62,
            77, 113, 121, 112, 100, 120, 92, 101, 103, 99, 255, 219, 0, 67, 1, 17, 18, 18, 24, 21,
            24, 47, 26, 26, 47, 99, 66, 56, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
            99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
            99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 255, 192, 0, 17, 8,
            0, 2, 0, 4, 3, 1, 34, 0, 2, 17, 1, 3, 17, 1, 255, 196, 0, 21, 0, 1, 1, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 255, 196, 0, 20, 16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 255, 196, 0, 21, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 4, 6, 255, 196, 0, 20, 17, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255,
            218, 0, 12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 142, 0, 42, 135, 255, 217,
        ];
        let _window = Window::new("image", 200, 100)?;
        let mut view = ImageView::new()?;
        assert_eq!(view.image_size(), None);
        view.load_from_data(PNG_3X2)?;
        assert_eq!(view.image_size(), Some((3, 2)));
        view.load_from_data(JPEG_4X2_TURNED)?;
        assert_eq!(view.image_size(), Some((2, 4)));
        assert!(view.load_from_data(b"not an image").is_err());
        assert_eq!(view.image_size(), Some((2, 4)));
        Ok(())
    }

    pub fn one_radio_is_picked_at_a_time() -> AureaResult<()> {
        let _window = Window::new("radios", 200, 100)?;
        let mut group = RadioGroup::new(&["Low", "Mid", "High"])?;
        assert_eq!(group.selected(), Some(0));
        group.select(1)?;
        assert_eq!(group.selected(), Some(1));
        group.select(2)?;
        assert_eq!(group.selected(), Some(2));
        assert!(group.select(9).is_err());
        assert_eq!(group.selected(), Some(2));
        Ok(())
    }

    pub fn selecting_a_radio_stays_quiet() -> AureaResult<()> {
        let _window = Window::new("radios", 200, 100)?;
        let heard = Rc::new(RefCell::new(Vec::new()));
        let mut group = RadioGroup::new(&["Low", "Mid"])?;
        let sink = Rc::clone(&heard);
        group.on_change(move |i| sink.borrow_mut().push(i))?;
        group.select(1)?;
        assert!(heard.borrow().is_empty());
        Ok(())
    }

    pub fn a_scroll_view_replaces_its_content() -> AureaResult<()> {
        let _window = Window::new("scroll ownership", 300, 200)?;
        let mut scroll = ScrollView::with_content(Label::new("first")?)?;
        scroll.set_content(Label::new("second")?)?;
        drop(scroll);
        Ok(())
    }

    pub fn a_nan_value_leaves_the_slider_where_it_was() -> AureaResult<()> {
        let _window = Window::new("slider", 200, 100)?;
        let mut slider = Slider::with_value(0.0, 10.0, f64::NAN)?;
        assert_eq!(slider.get_value(), 0.0);
        slider.set_value(4.0)?;
        assert!(slider.set_value(f64::NAN).is_err());
        assert!((slider.get_value() - 4.0).abs() < 0.01);
        slider.set_value(99.0)?;
        assert_eq!(slider.get_value(), 10.0);
        Ok(())
    }

    pub fn a_spin_box_stays_in_range_and_quiet() -> AureaResult<()> {
        use aurea::elements::SpinBox;
        let _window = Window::new("spin", 200, 100)?;
        let mut spin = SpinBox::new(0.0, 10.0, 0.5)?;
        let heard = Rc::new(RefCell::new(0));
        let sink = Rc::clone(&heard);
        spin.on_change(move |_| *sink.borrow_mut() += 1)?;
        assert_eq!(spin.value(), 0.0);
        spin.set_value(2.5)?;
        assert_eq!(spin.value(), 2.5);
        spin.set_value(99.0)?;
        assert_eq!(spin.value(), 10.0);
        spin.set_value(-5.0)?;
        assert_eq!(spin.value(), 0.0);
        assert_eq!(*heard.borrow(), 0, "set_value is not the user's change");
        Ok(())
    }

    pub fn a_split_view_takes_ownership_of_what_it_is_given() -> AureaResult<()> {
        use aurea::elements::{SplitOrientation, SplitView};
        let _window = Window::new("split ownership", 300, 200)?;
        let mut split = SplitView::new(SplitOrientation::Horizontal)?;
        split.add(Label::new("left")?)?;
        split.add(Label::new("right")?)?;
        assert!(
            split.add(Label::new("third")?).is_err(),
            "two panes, as on every platform"
        );
        split.set_divider_position(0, 120.0)?;
        assert!(split.set_divider_position(1, 120.0).is_err());
        assert!(split.set_divider_position(0, -1.0).is_err());
        assert!(split.set_divider_position(0, f32::NAN).is_err());
        assert!(split.set_divider_position(0, f32::INFINITY).is_err());
        drop(split);
        Ok(())
    }

    pub fn text_fields_search_tooltip_and_password() -> AureaResult<()> {
        use aurea::elements::TextField;
        let secret = |field: &TextField| {
            let cell = appkit::id(field.handle(), c"cell");
            appkit::boolean_id(
                cell,
                c"isKindOfClass:",
                appkit::class(c"NSSecureTextFieldCell"),
            )
        };
        let _window = Window::new("text fields", 200, 100)?;
        let field = TextField::new()?;
        assert_eq!(field.get_content()?, "");
        field.set_tooltip("What goes here")?;
        field.set_tooltip("Something else")?;
        field.set_tooltip("")?;
        let mut search = TextField::search()?;
        search.set_placeholder("Find")?;
        search.set_content("layer")?;
        assert_eq!(search.get_content()?, "layer");
        let password = TextField::with_content("hunter2")?;
        password.set_password(true)?;
        assert!(secret(&password));
        assert_eq!(password.get_content()?, "hunter2");
        password.set_password(false)?;
        assert!(!secret(&password));
        assert_eq!(password.get_content()?, "hunter2");
        Ok(())
    }

    pub fn embedding_creates_a_canvas() -> AureaResult<()> {
        use aurea::embed::{aurea_embed_create_canvas, aurea_embed_destroy_canvas};
        let handle = aurea_embed_create_canvas(100, 100);
        assert!(!handle.is_null());
        aurea_embed_destroy_canvas(handle);
        Ok(())
    }

    pub fn a_menu_bar_belongs_to_its_window() -> AureaResult<()> {
        let mut window = Window::new("menus", 300, 200)?;
        {
            let bar = window.create_menu_bar()?;
            let mut file = bar.add_submenu("File")?;
            file.add_item("New", || {})?;
        }
        {
            let bar = window.create_menu_bar()?;
            bar.add_submenu("Edit")?;
        }
        drop(window);
        Ok(())
    }

    pub fn a_ticked_row_shows_its_chord() -> AureaResult<()> {
        use aurea::{Chord, KeyCode};
        let mut window = Window::new("ticked", 300, 200)?;
        let bar = window.create_menu_bar()?;
        let mut view = bar.add_submenu("View")?;
        let snap =
            view.add_check_item_with_chord("Snap", Chord::key(KeyCode::Semicolon), true, || {})?;
        snap.set_checked(false)?;
        snap.set_label("Snap to Grid")?;
        Ok(())
    }

    pub fn clearing_a_parent_invalidates_descendants_and_releases_callbacks() -> AureaResult<()> {
        use aurea::{MenuShortcut, ShortcutKey};
        let mut window = Window::new("nested menus", 300, 200)?;
        let bar = window.create_menu_bar()?;
        let mut root = bar.add_submenu("Layers")?;
        for _ in 0..300 {
            let mut child = root.add_submenu("Adjustment")?;
            let mut grandchild = child.add_submenu("More")?;
            let captured = Rc::new(());
            let weak = Rc::downgrade(&captured);
            let item = grandchild.add_item_with_shortcut(
                "Levels",
                MenuShortcut::new(ShortcutKey::Char('L')).primary(),
                move || {
                    let _ = &captured;
                },
            )?;
            assert!(weak.upgrade().is_some());
            root.clear()?;
            assert!(weak.upgrade().is_none());
            assert!(child.add_separator().is_err());
            assert!(grandchild.clear().is_err());
            assert!(item.set_enabled(false).is_err());
            assert!(item.remove().is_err());
        }
        root.add_item("Still alive", || {})?;
        Ok(())
    }

    pub fn canvases_draw_retained_immediate_and_per_frame() -> AureaResult<()> {
        use aurea::FrameScheduler;
        use aurea::render::{
            Canvas, Color, DrawingContext, Paint, PaintStyle, Rect, RendererBackend,
        };
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        const BG: Color = Color {
            r: 240,
            g: 240,
            b: 240,
            a: 255,
        };
        let square = |ctx: &mut dyn DrawingContext| {
            let fill = Paint::new()
                .color(Color::rgb(100, 150, 200))
                .style(PaintStyle::Fill);
            ctx.draw_rect(Rect::new(10.0, 10.0, 100.0, 100.0), &fill)
        };
        let drawn = |canvas: &Canvas| {
            let bg = 0xFF00_0000 | (240 << 16) | (240 << 8) | 240;
            canvas
                .with_frame_pixels(|pixels, _, _| pixels.iter().filter(|&&p| p != bg).count())
                .unwrap_or(0)
        };
        let shown = |title: &str| -> AureaResult<(Window, Canvas)> {
            let mut window = Window::new(title, 400, 300)?;
            let canvas = Canvas::new(400, 300, RendererBackend::Cpu)?;
            canvas.set_background_color(BG);
            let mut layout = Stack::new(Orientation::Vertical)?;
            layout.add(canvas.clone())?;
            window.set_content(layout)?;
            window.show();
            Ok((window, canvas))
        };
        let frames = |window: &Window, n: usize| -> AureaResult<()> {
            for _ in 0..n {
                window.poll_events();
                window.process_frames()?;
            }
            Ok(())
        };

        // retained, through the layout's resize, capturing its own canvas
        let (window, canvas) = shown("retained")?;
        let ran = Rc::new(RefCell::new(0));
        let (count, own) = (Rc::clone(&ran), canvas.clone());
        canvas.set_draw_callback(move |ctx| {
            *count.borrow_mut() += 1;
            let (w, _) = own.size();
            square(ctx)?;
            ctx.draw_rect(
                Rect::new(0.0, 0.0, w as f32 / 4.0, 4.0),
                &Paint::new().color(Color::rgb(200, 120, 60)),
            )
        })?;
        frames(&window, 3)?;
        assert!(*ran.borrow() > 0, "the callback never ran");
        assert!(drawn(&canvas) > 1000, "retained canvas went blank");
        drop((window, canvas));

        // immediate, kept through invalidate_all
        let (window, mut canvas) = shown("immediate")?;
        frames(&window, 20)?;
        canvas.draw(square)?;
        let before = drawn(&canvas);
        assert!(before > 1000, "immediate draw produced nothing");
        canvas.invalidate_all();
        frames(&window, 3)?;
        assert_eq!(
            drawn(&canvas),
            before,
            "invalidate_all wiped an immediate canvas"
        );
        drop((window, canvas));

        // a ticker redraws each frame, and stops when it says so
        let (window, canvas) = shown("on_frame")?;
        frames(&window, 20)?;
        let draws = Rc::new(RefCell::new(0));
        let counted = Rc::clone(&draws);
        canvas.set_draw_callback(move |ctx| {
            *counted.borrow_mut() += 1;
            let n = *counted.borrow() % 40;
            let fill = Paint::new()
                .color(Color::rgb(100, 150, 200))
                .style(PaintStyle::Fill);
            ctx.draw_rect(Rect::new(n as f32, 10.0, 20.0, 20.0), &fill)
        })?;
        let ticks = Arc::new(AtomicUsize::new(0));
        let tick = Arc::clone(&ticks);
        let ticker = canvas.on_frame(move |_| {
            tick.fetch_add(1, Ordering::Relaxed);
            true
        });
        let once = Arc::new(AtomicUsize::new(0));
        let one = Arc::clone(&once);
        canvas.on_frame(move |_| {
            one.fetch_add(1, Ordering::Relaxed);
            false
        });
        frames(&window, 4)?;
        FrameScheduler::unregister_ticker(ticker);
        assert!(
            ticks.load(Ordering::Relaxed) >= 3,
            "the ticker should run each frame"
        );
        assert!(
            *draws.borrow() >= 3,
            "the canvas redrew {} time(s)",
            draws.borrow()
        );
        assert_eq!(
            once.load(Ordering::Relaxed),
            1,
            "a ticker that says stop runs once"
        );
        drop((window, canvas));

        // clones share one native element, freed once
        for _ in 0..100 {
            let canvas = Canvas::new(64, 64, RendererBackend::Cpu)?;
            assert!(!canvas.handle().is_null());
            let (a, b) = (canvas.clone(), canvas.clone());
            drop((canvas, a, b));
        }
        Ok(())
    }

    pub fn custom_cursors_replace_and_validate_pixels() -> AureaResult<()> {
        let window = Window::new("cursor lifecycle", 80, 80)?;
        let image = Image::new(16, 16, [255, 0, 0, 128].repeat(256));
        assert!(window.set_cursor_image(&image, (16, 0)).is_err());
        assert!(
            window
                .set_cursor_image(&Image::new(16, 16, vec![0; 4]), (0, 0))
                .is_err()
        );
        for _ in 0..300 {
            window.set_cursor_image(&image, (8, 8))?;
            window.set_cursor_image(&image, (0, 0))?;
            window.set_cursor(CursorIcon::ResizeNwSe)?;
            window.set_cursor(CursorIcon::ResizeNeSw)?;
        }
        window.set_cursor_image(&image, (8, 8))?;
        drop(window);
        Ok(())
    }

    pub fn replacing_window_content_destroys_the_old_content() -> AureaResult<()> {
        let mut window = Window::new("replace", 300, 200)?;
        let first = Label::new("first")?;
        let first_handle = first.handle();
        window.set_content(first)?;
        let second = Label::new("second")?;
        assert_ne!(first_handle, second.handle());
        window.set_content(second)?;
        window.set_title("replaced")?;
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
        radios_and_ticks_keep_their_size_in_a_stack,
        a_container_takes_ownership_of_what_it_is_given,
        ownership_survives_repetition,
        wrapper_elements_forward_ownership,
        dividers_and_spacers_can_be_created,
        images_report_their_upright_size,
        one_radio_is_picked_at_a_time,
        selecting_a_radio_stays_quiet,
        a_scroll_view_replaces_its_content,
        a_nan_value_leaves_the_slider_where_it_was,
        a_spin_box_stays_in_range_and_quiet,
        a_split_view_takes_ownership_of_what_it_is_given,
        text_fields_search_tooltip_and_password,
        embedding_creates_a_canvas,
        a_menu_bar_belongs_to_its_window,
        a_ticked_row_shows_its_chord,
        clearing_a_parent_invalidates_descendants_and_releases_callbacks,
        canvases_draw_retained_immediate_and_per_frame,
        custom_cursors_replace_and_validate_pixels,
        replacing_window_content_destroys_the_old_content,
        a_window_keeps_its_minimum_size,
        a_styled_text_view_keeps_its_look_when_attached,
        menus_and_cursors_survive_being_rebuilt,
        a_window_with_a_filling_canvas_shrinks_both_ways,
        the_input_method_can_be_kept_out_and_let_back,
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
