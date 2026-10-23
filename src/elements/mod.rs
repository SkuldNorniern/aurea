//! Native UI elements (widgets) and container traits.

mod button;
mod checkbox;
mod combo_box;
mod container;
mod divider;
mod image_view;
mod label;
mod native;
mod progress_bar;
mod scroll_view;
mod shared;
mod sidebar_list;
mod slider;
mod spacer;
mod split_view;
mod swiftui_host;
mod tab_bar;
mod table;
mod text;
mod traits;

pub use button::Button;
pub use checkbox::Checkbox;
pub use combo_box::ComboBox;
pub use container::{Orientation, Stack};
pub use divider::Divider;
pub use image_view::{ImageScaling, ImageView};
pub use label::{Label, TextStyle};
pub use native::NativeElement;
pub use progress_bar::ProgressBar;
pub use scroll_view::{ScrollView, ScrollbarPolicy};
pub use shared::Shared;
pub use sidebar_list::SidebarList;
pub use slider::Slider;
pub use spacer::Spacer;
pub use split_view::{SplitOrientation, SplitView};
pub use swiftui_host::{SwiftUIHost, SwiftUIHostNotAvailable};
pub use tab_bar::TabBar;
pub use table::Table;
pub use text::{TextChange, TextControl, TextEditor, TextFeatures, TextField, TextView};
pub use traits::{Container, Element};

#[cfg(all(test, target_os = "linux"))]
mod gtk_tests {
    //! Native GTK widgets, read back through GTK.

    use super::*;
    use crate::{AureaError, AureaResult, Window};
    use std::cell::RefCell;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_int, c_void};
    use std::rc::Rc;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    unsafe extern "C" {
        fn gtk_widget_get_allocated_height(widget: *mut c_void) -> c_int;
        fn gtk_label_get_text(label: *mut c_void) -> *const c_char;
        fn gtk_toggle_button_get_active(button: *mut c_void) -> c_int;
        fn gtk_button_clicked(button: *mut c_void);
        fn gtk_container_get_children(container: *mut c_void) -> *mut c_void;
        fn g_list_nth_data(list: *mut c_void, n: u32) -> *mut c_void;
        fn g_list_length(list: *mut c_void) -> u32;
        fn g_list_free(list: *mut c_void);
        fn gtk_combo_box_set_active(combo: *mut c_void, index: c_int);
        fn gtk_widget_get_visible(widget: *mut c_void) -> c_int;
        fn gtk_list_box_select_row(list: *mut c_void, row: *mut c_void);
        fn gtk_widget_is_sensitive(widget: *mut c_void) -> c_int;
        fn gtk_label_get_attributes(label: *mut c_void) -> *mut c_void;
        fn gtk_widget_get_allocated_width(widget: *mut c_void) -> c_int;
        fn gtk_widget_draw(widget: *mut c_void, cr: *mut c_void);
        fn cairo_image_surface_create(format: c_int, w: c_int, h: c_int) -> *mut c_void;
        fn cairo_create(surface: *mut c_void) -> *mut c_void;
        fn cairo_destroy(cr: *mut c_void);
        fn cairo_surface_flush(surface: *mut c_void);
        fn cairo_surface_destroy(surface: *mut c_void);
        fn cairo_image_surface_get_data(surface: *mut c_void) -> *mut u8;
        fn cairo_image_surface_get_stride(surface: *mut c_void) -> c_int;
        fn g_signal_emit_by_name(instance: *mut c_void, signal: *const c_char, ...);
    }

    const TEXT: &str = "연결 · 日本語 🙂 …";

    fn settle(window: &Window) {
        window.show();
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            window.poll_events();
            sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_scroll_view_scrolls_tall_content() -> AureaResult<()> {
        unsafe extern "C" {
            fn gtk_scrolled_window_get_vadjustment(scroll: *mut c_void) -> *mut c_void;
            fn gtk_adjustment_get_upper(adjustment: *mut c_void) -> f64;
            fn gtk_adjustment_get_page_size(adjustment: *mut c_void) -> f64;
        }
        let mut window = Window::new("scroll", 300, 200)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        for i in 0..40 {
            stack.add(Label::new(&format!("row {i}"))?)?;
        }
        let scroll = Shared::new(ScrollView::with_content(stack)?);
        window.set_content(scroll.clone())?;
        settle(&window);
        let adjustment = unsafe { gtk_scrolled_window_get_vadjustment(scroll.handle()) };
        let (upper, page) = unsafe {
            (
                gtk_adjustment_get_upper(adjustment),
                gtk_adjustment_get_page_size(adjustment),
            )
        };
        assert!(page > 0.0 && upper > 4.0 * page, "{upper} {page}");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn an_oversized_window_fits_its_work_area() -> AureaResult<()> {
        #[repr(C)]
        #[derive(Default)]
        struct Area {
            x: c_int,
            y: c_int,
            width: c_int,
            height: c_int,
        }
        unsafe extern "C" {
            fn gdk_display_get_default() -> *mut c_void;
            fn gdk_display_get_monitor(display: *mut c_void, n: c_int) -> *mut c_void;
            fn gdk_monitor_get_workarea(monitor: *mut c_void, area: *mut Area);
        }
        let mut window = Window::new("oversized", 20_000, 20_000)?;
        let root = Stack::new(Orientation::Vertical)?;
        let r = root.handle();
        window.set_content(root)?;
        settle(&window);
        let mut work = Area::default();
        unsafe {
            gdk_monitor_get_workarea(
                gdk_display_get_monitor(gdk_display_get_default(), 0),
                &raw mut work,
            )
        };
        let (w, h) = unsafe {
            (
                gtk_widget_get_allocated_width(r),
                gtk_widget_get_allocated_height(r),
            )
        };
        assert!(
            w <= work.width && h <= work.height,
            "{w}x{h} in {}x{}",
            work.width,
            work.height
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn wrapped_labels_keep_the_window_height() -> AureaResult<()> {
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
        let r = root.handle();
        window.set_content(root)?;
        settle(&window);
        assert_eq!(unsafe { gtk_widget_get_allocated_height(r) }, 300);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_scroll_bar_follows_its_policy() -> AureaResult<()> {
        unsafe extern "C" {
            fn gtk_scrolled_window_get_policy(scroll: *mut c_void, h: *mut c_int, v: *mut c_int);
        }
        const GTK_POLICY_ALWAYS: c_int = 0;
        const GTK_POLICY_AUTOMATIC: c_int = 1;
        let _window = Window::new("scroll policy", 300, 200)?;
        let mut scroll = ScrollView::with_content(Label::new("short")?)?;
        let vertical = |scroll: &ScrollView| {
            let (mut h, mut v) = (0, 0);
            unsafe { gtk_scrolled_window_get_policy(scroll.handle(), &raw mut h, &raw mut v) };
            v
        };
        assert_eq!(vertical(&scroll), GTK_POLICY_AUTOMATIC);
        scroll.set_scrollbar(ScrollbarPolicy::Always)?;
        assert_eq!(vertical(&scroll), GTK_POLICY_ALWAYS);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn an_element_takes_an_accessible_name() -> AureaResult<()> {
        unsafe extern "C" {
            fn gtk_widget_get_accessible(widget: *mut c_void) -> *mut c_void;
            fn atk_object_get_name(object: *mut c_void) -> *const c_char;
            fn gtk_bin_get_child(bin: *mut c_void) -> *mut c_void;
        }
        let name = |widget: *mut c_void| unsafe {
            CStr::from_ptr(atk_object_get_name(gtk_widget_get_accessible(widget)))
                .to_string_lossy()
                .into_owned()
        };
        let _window = Window::new("names", 300, 200)?;
        let button = Button::new("+")?;
        button.set_accessible_name("New tab")?;
        assert_eq!(name(button.handle()), "New tab");
        let editor = TextEditor::new()?;
        editor.set_accessible_name("Source")?;
        assert_eq!(
            name(unsafe { gtk_bin_get_child(editor.handle()) }),
            "Source"
        );
        let table = Table::new(["Adapter"])?;
        table.set_accessible_name("Adapters")?;
        assert_eq!(
            name(unsafe { gtk_bin_get_child(table.handle()) }),
            "Adapters"
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_scroll_view_reveals_the_focused_control() -> AureaResult<()> {
        unsafe extern "C" {
            fn gtk_scrolled_window_get_vadjustment(scroll: *mut c_void) -> *mut c_void;
            fn gtk_adjustment_get_value(adjustment: *mut c_void) -> f64;
            fn gtk_widget_grab_focus(widget: *mut c_void);
        }
        let mut window = Window::new("scroll focus", 300, 200)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        for i in 0..20 {
            stack.add(Button::new(&format!("button {i}"))?)?;
        }
        let last = Button::new("last")?;
        let l = last.handle();
        stack.add(last)?;
        let scroll = Shared::new(ScrollView::with_content(stack)?);
        window.set_content(scroll.clone())?;
        settle(&window);
        let adjustment = unsafe { gtk_scrolled_window_get_vadjustment(scroll.handle()) };
        assert_eq!(unsafe { gtk_adjustment_get_value(adjustment) }, 0.0);

        unsafe { gtk_widget_grab_focus(l) };
        settle(&window);
        assert!(unsafe { gtk_adjustment_get_value(adjustment) } > 0.0);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn labels_wrap_from_the_leading_edge() -> AureaResult<()> {
        unsafe extern "C" {
            fn gtk_label_get_line_wrap(label: *mut c_void) -> c_int;
            fn gtk_label_get_xalign(label: *mut c_void) -> f32;
        }
        let _window = Window::new("wrap", 300, 200)?;
        let label = Label::new("long help text")?;
        assert_eq!(unsafe { gtk_label_get_line_wrap(label.handle()) }, 1);
        assert!(unsafe { gtk_label_get_xalign(label.handle()) }.abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn an_indeterminate_bar_keeps_pulsing() -> AureaResult<()> {
        unsafe extern "C" {
            fn g_object_get_data(object: *mut c_void, key: *const c_char) -> *mut c_void;
        }
        let window = Window::new("pulse", 300, 200)?;
        let mut bar = ProgressBar::new()?;
        let key = c"aurea-pulse".as_ptr();
        bar.set_indeterminate(true)?;
        settle(&window);
        assert!(!unsafe { g_object_get_data(bar.handle(), key) }.is_null());
        bar.set_indeterminate(false)?;
        assert!(unsafe { g_object_get_data(bar.handle(), key) }.is_null());
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_window_keeps_its_minimum_size() -> AureaResult<()> {
        let window = Window::new("min size", 400, 300)?;
        window.set_min_size(500, 350)?;
        settle(&window);
        assert_eq!(window.size(), (500, 350));
        window.set_size(100, 100);
        settle(&window);
        settle(&window);
        assert_eq!(window.size(), (500, 350));
        Ok(())
    }

    fn tabs(bar: *mut c_void) -> Vec<*mut c_void> {
        unsafe {
            let list = gtk_container_get_children(bar);
            let found = (0..g_list_length(list))
                .map(|i| g_list_nth_data(list, i))
                .collect();
            g_list_free(list);
            found
        }
    }

    fn active(bar: *mut c_void) -> Vec<bool> {
        tabs(bar)
            .into_iter()
            .map(|b| unsafe { gtk_toggle_button_get_active(b) } != 0)
            .collect()
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn weighted_views_share_the_height() -> AureaResult<()> {
        let mut window = Window::new("weights", 820, 620)?;
        let profile = TextView::with_content(false, "1\n2\n3\n4\n5\n6")?;
        let log = TextView::with_content(false, "log")?;
        let (a, b) = (profile.handle(), log.handle());
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(Label::new("Profile")?)?;
        root.add_weighted(profile, 1.0)?;
        root.add(Button::new("Action")?)?;
        root.add_weighted(log, 1.0)?;
        window.set_content(root)?;
        settle(&window);
        let (a, b) = unsafe {
            (
                gtk_widget_get_allocated_height(a),
                gtk_widget_get_allocated_height(b),
            )
        };
        assert!(a > 200 && b > 200, "weighted views got {a} and {b} px");
        Ok(())
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn tab_bar_tracks_its_selection() -> AureaResult<()> {
        let picked = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&picked);
        let mut window = Window::new("tabs", 600, 300)?;
        let bar = Shared::new(TabBar::with_callbacks_and_tabs(
            move |i| seen.borrow_mut().push(i),
            |_| {},
            ["General", "System", "Storage"],
        )?);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(bar.clone())?;
        window.set_content(root)?;
        let handle = bar.handle();

        bar.borrow_mut().set_selected(2)?;
        assert_eq!(active(handle), [false, false, true]);
        unsafe { gtk_button_clicked(tabs(handle)[2]) };
        assert_eq!(
            active(handle),
            [false, false, true],
            "clicking the open tab closed it"
        );
        unsafe { gtk_button_clicked(tabs(handle)[1]) };
        assert_eq!(*picked.borrow(), [1]);

        bar.borrow_mut().remove_tab(0)?;
        assert_eq!(bar.borrow().get_selected(), 0);
        assert_eq!(active(handle), [true, false]);
        assert!(bar.borrow_mut().remove_tab(5).is_err());
        assert!(bar.borrow_mut().set_selected(-1).is_err());
        Ok(())
    }

    fn recorder<T: 'static>() -> (Rc<RefCell<Vec<T>>>, impl Fn(T) + 'static) {
        let got = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&got);
        (got, move |v| seen.borrow_mut().push(v))
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn value_controls_report_only_the_user() -> AureaResult<()> {
        let _window = Window::new("changes", 400, 300)?;
        let (checks, on_check) = recorder();
        let (picks, on_pick) = recorder();
        let (moves, on_move) = recorder();
        let mut check = Checkbox::new("Share")?;
        check.on_change(on_check)?;
        let mut combo = ComboBox::with_items(["a", "b", "c"])?;
        combo.on_change(on_pick)?;
        let mut slider = Slider::new(0.0, 1.0)?;
        slider.on_change(on_move)?;

        check.set_checked(true)?;
        combo.set_selected(1)?;
        slider.set_value(0.5)?;
        assert!(
            checks.borrow().is_empty() && picks.borrow().is_empty() && moves.borrow().is_empty()
        );

        unsafe {
            gtk_button_clicked(check.handle());
            gtk_combo_box_set_active(combo.handle(), 2);
        }
        assert_eq!(*checks.borrow(), [false]);
        assert_eq!(*picks.borrow(), [2]);
        let mut stop: c_int = 0;
        unsafe {
            g_signal_emit_by_name(
                slider.handle(),
                c"change-value".as_ptr(),
                1,
                0.37,
                &raw mut stop,
            );
        }
        assert!(
            (slider.get_value() - 0.37).abs() < 1e-9,
            "dragged to {}",
            slider.get_value()
        );
        assert_eq!(moves.borrow().len(), 1);
        Ok(())
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn a_sidebar_shows_rows_and_reports_the_user() -> AureaResult<()> {
        let mut window = Window::new("sidebar", 400, 300)?;
        let (picked, on_pick) = recorder();
        let list = Shared::new(SidebarList::with_callback(on_pick)?);
        list.borrow_mut().add_section("Machines")?;
        list.borrow_mut().add_item("one", 0)?;
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(list.clone(), 1.0)?;
        window.set_content(root)?;
        list.borrow_mut().add_item("late", 0)?;
        let rows = tabs(list.handle());
        let shown = |w: *mut c_void| unsafe { gtk_widget_get_visible(w) } != 0;
        assert!(
            rows.iter().all(|&r| shown(r) && shown(tabs(r)[0])),
            "rows hidden"
        );

        list.borrow_mut().set_selected(1)?;
        assert!(picked.borrow().is_empty());
        assert!(list.borrow_mut().set_selected(2).is_err());
        unsafe { gtk_list_box_select_row(list.handle(), rows[1]) };
        assert_eq!(*picked.borrow(), [0]);
        Ok(())
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn a_disabled_stack_greys_its_children() -> AureaResult<()> {
        let mut window = Window::new("enabled", 400, 300)?;
        let button = Button::new("Go")?;
        let b = button.handle();
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(button)?;
        let root = Shared::new(root);
        window.set_content(root.clone())?;
        root.set_enabled(false)?;
        assert_eq!(unsafe { gtk_widget_is_sensitive(b) }, 0);
        root.set_enabled(true)?;
        assert_ne!(unsafe { gtk_widget_is_sensitive(b) }, 0);
        Ok(())
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn a_shared_element_hears_its_window_close() -> AureaResult<()> {
        let mut window = Window::new("destroy", 400, 300)?;
        let label = Shared::new(Label::new("Ready")?);
        let (heard, on_heard) = recorder();
        label.on_destroyed(move || on_heard(()));
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(label.clone())?;
        window.set_content(root)?;
        drop(window);
        assert_eq!(heard.borrow().len(), 1);
        assert!(matches!(
            label.try_borrow_mut(),
            Err(AureaError::ElementDestroyed)
        ));
        Ok(())
    }

    fn bmp(w: u32, h: u32, bgr: [u8; 3]) -> Vec<u8> {
        let row = (w * 3).div_ceil(4) * 4;
        let size = 54 + row * h;
        let mut out = Vec::new();
        out.extend_from_slice(b"BM");
        for v in [size, 0, 54, 40] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&[0; 24]);
        for _ in 0..h {
            let start = out.len();
            for _ in 0..w {
                out.extend_from_slice(&bgr);
            }
            out.resize(start + row as usize, 0);
        }
        out
    }

    fn render(widget: *mut c_void) -> (Vec<u32>, usize, usize) {
        unsafe {
            let w = gtk_widget_get_allocated_width(widget);
            let h = gtk_widget_get_allocated_height(widget);
            let surface = cairo_image_surface_create(0, w, h);
            let cr = cairo_create(surface);
            gtk_widget_draw(widget, cr);
            cairo_destroy(cr);
            cairo_surface_flush(surface);
            let stride = usize::try_from(cairo_image_surface_get_stride(surface)).unwrap_or(0) / 4;
            let (w, h) = (
                usize::try_from(w).unwrap_or(0),
                usize::try_from(h).unwrap_or(0),
            );
            let data = cairo_image_surface_get_data(surface).cast::<u32>();
            let pixels = (0..h)
                .flat_map(|y| (0..w).map(move |x| *data.add(y * stride + x)))
                .collect();
            cairo_surface_destroy(surface);
            (pixels, w, h)
        }
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn an_image_keeps_its_aspect() -> AureaResult<()> {
        let mut window = Window::new("image", 400, 300)?;
        let view = ImageView::with_data(&bmp(40, 20, [0, 0, 255]))?;
        let v = view.handle();
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(view, 1.0)?;
        window.set_content(root)?;
        settle(&window);
        let (pixels, w, h) = render(v);
        let at = |x: usize, y: usize| pixels[y * w + x];
        assert_eq!(at(w / 2, h / 2), 0xFFFF_0000, "no image in the middle");
        let reach = h.min(w / 2) / 4;
        assert!(
            reach > 10 && at(w / 2, h / 2 + reach) == 0xFFFF_0000,
            "not scaled"
        );
        let bar = if w > 2 * h {
            at(1, h / 2)
        } else {
            at(w / 2, 1)
        };
        assert_eq!(bar >> 24, 0, "the image covers the letterbox");
        Ok(())
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn a_heading_label_carries_its_style() -> AureaResult<()> {
        let _window = Window::new("styles", 400, 300)?;
        let body = Label::new("body")?;
        let heading = Label::with_style("heading", TextStyle::Heading)?;
        assert!(unsafe { gtk_label_get_attributes(body.handle()) }.is_null());
        assert!(!unsafe { gtk_label_get_attributes(heading.handle()) }.is_null());
        Ok(())
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn a_text_view_ignores_its_own_set() -> AureaResult<()> {
        let _window = Window::new("text", 400, 300)?;
        let (got, on_text) = recorder();
        let mut view = TextView::with_callback(true, on_text)?;
        view.set_content("from code")?;
        assert!(got.borrow().is_empty());
        Ok(())
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn hidden_and_sized_children_lay_out() -> AureaResult<()> {
        let mut window = Window::new("layout", 400, 300)?;
        let first = Shared::new(Label::new("first")?);
        let button = Button::new("Go")?;
        let b = button.handle();
        button.set_min_size(0, 90)?;
        let mut root = Stack::new(Orientation::Vertical)?;
        root.set_spacing(0)?;
        root.add(first.clone())?;
        root.add(button)?;
        window.set_content(root)?;
        first.set_visible(false)?;
        settle(&window);
        assert!(unsafe { gtk_widget_get_allocated_height(b) } >= 90);
        assert_eq!(unsafe { gtk_widget_get_visible(first.handle()) }, 0);
        Ok(())
    }

    #[test]
    #[ignore = "needs a display; run with --ignored"]
    fn a_shared_label_outlives_its_window() -> AureaResult<()> {
        let mut window = Window::new("label", 400, 300)?;
        let label = Shared::new(Label::new("Ready")?);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(label.clone())?;
        window.set_content(root)?;
        label.borrow_mut().set_text(TEXT)?;
        let shown = unsafe { CStr::from_ptr(gtk_label_get_text(label.handle())) };
        assert_eq!(shown.to_str().ok(), Some(TEXT));
        drop(window);
        let _ = label.borrow_mut().set_text("gone");
        Ok(())
    }
}

#[cfg(all(test, target_os = "windows"))]
mod win32_tests {
    //! Native Win32 controls, read back through the wide API.

    use super::*;
    use crate::render::Rect as AreaRect;
    use crate::{AureaError, AureaResult, Window};
    use std::cell::RefCell;
    use std::os::raw::c_void;
    use std::ptr::null_mut;
    use std::rc::Rc;

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
        fn GetWindowTextW(hwnd: *mut c_void, text: *mut u16, len: i32) -> i32;
        fn SetWindowTextW(hwnd: *mut c_void, text: *const u16) -> i32;
        fn GetWindowRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
        fn GetDlgItem(hwnd: *mut c_void, id: i32) -> *mut c_void;
        fn GetWindow(hwnd: *mut c_void, cmd: u32) -> *mut c_void;
        fn IsWindow(hwnd: *mut c_void) -> i32;
        fn IsWindowEnabled(hwnd: *mut c_void) -> i32;
        fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        fn ReleaseDC(hwnd: *mut c_void, dc: *mut c_void) -> i32;
        fn SendMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> isize;
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: *mut c_void,
            menu: *mut c_void,
            instance: *mut c_void,
            param: *mut c_void,
        ) -> *mut c_void;
    }

    unsafe extern "C" {
        fn ng_windows_detached_parent() -> *mut c_void;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateCompatibleDC(dc: *mut c_void) -> *mut c_void;
        fn CreateCompatibleBitmap(dc: *mut c_void, w: i32, h: i32) -> *mut c_void;
        fn SelectObject(dc: *mut c_void, object: *mut c_void) -> *mut c_void;
        fn GetPixel(dc: *mut c_void, x: i32, y: i32) -> u32;
        fn DeleteObject(object: *mut c_void) -> i32;
        fn DeleteDC(dc: *mut c_void) -> i32;
        fn GetObjectW(object: *mut c_void, size: i32, out: *mut c_void) -> i32;
    }

    const WM_PRINTCLIENT: u32 = 0x0318;
    const PRF_CLIENT: isize = 4;

    const TEXT: &str = "연결 · 日本語 🙂 …";
    const BM_GETCHECK: u32 = 0x00F0;
    const BM_CLICK: u32 = 0x00F5;
    const EM_REPLACESEL: u32 = 0x00C2;
    const EM_SETSEL: u32 = 0x00B1;
    const WM_CHAR: u32 = 0x0102;
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const WM_COMMAND: u32 = 0x0111;
    const GW_CHILD: u32 = 5;
    const WM_HSCROLL: u32 = 0x0114;
    const CB_SETCURSEL: u32 = 0x014E;
    const CBN_SELCHANGE: usize = 1;
    const TBM_SETPOS: u32 = 0x0405;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    fn native_text(hwnd: *mut c_void) -> String {
        let mut buf = [0u16; 256];
        let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), 256) };
        String::from_utf16_lossy(&buf[..usize::try_from(len).unwrap_or(0)])
    }

    fn rect(hwnd: *mut c_void) -> Rect {
        let mut r = Rect::default();
        unsafe { GetWindowRect(hwnd, &raw mut r) };
        r
    }

    #[test]
    fn wheel_steps_follow_the_scroll_setting() {
        unsafe extern "C" {
            fn ng_windows_wheel_step(
                delta: i32,
                lines: u32,
                line: i32,
                page: i32,
                rest: *mut i32,
            ) -> i32;
        }
        let step = |delta, lines, rest: &mut i32| unsafe {
            ng_windows_wheel_step(delta, lines, 16, 200, rest)
        };
        let mut rest = 0;
        assert_eq!(step(-120, 3, &mut rest), 48);
        assert_eq!(step(120, 3, &mut rest), -48);
        assert_eq!(step(-120, u32::MAX, &mut rest), 200);
        assert_eq!(step(120, u32::MAX, &mut rest), -200);
        assert_eq!(step(-120, 40, &mut rest), 200);
        assert_eq!(step(-120, 0, &mut rest), 0);
        let mut moved = 0;
        for _ in 0..120 {
            moved += step(-1, 3, &mut rest);
        }
        assert_eq!(moved, 48);
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_window_keeps_its_minimum_size() -> AureaResult<()> {
        let window = Window::new("min size", 400, 300)?;
        window.set_min_size(500, 350)?;
        assert_eq!(window.size(), (500, 350));
        window.set_size(100, 100);
        assert_eq!(window.size(), (500, 350));
        window.set_min_size(0, 0)?;
        window.set_size(300, 200);
        assert_eq!(window.size(), (300, 200));
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn enter_clicks_the_focused_button() -> AureaResult<()> {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SetFocus(hwnd: *mut c_void) -> *mut c_void;
            fn PostMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> i32;
        }
        const WM_KEYDOWN: u32 = 0x0100;
        const VK_RETURN: usize = 0x0D;
        let mut window = Window::new("enter", 300, 200)?;
        let clicks = Rc::new(RefCell::new(0));
        let counted = clicks.clone();
        let button = Button::with_callback("Go", move || *counted.borrow_mut() += 1)?;
        let b = button.handle();
        let checkbox = Checkbox::new("Check")?;
        let c = checkbox.handle();
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(button)?;
        root.add(checkbox)?;
        window.set_content(root)?;
        window.show();
        unsafe { SetFocus(b) };
        unsafe { PostMessageW(b, WM_KEYDOWN, VK_RETURN, 0) };
        window.poll_events();
        assert_eq!(*clicks.borrow(), 1);
        unsafe { SetFocus(c) };
        unsafe { PostMessageW(c, WM_KEYDOWN, VK_RETURN, 0) };
        window.poll_events();
        assert!(!checkbox_checked(c));
        Ok(())
    }

    fn checkbox_checked(hwnd: *mut c_void) -> bool {
        const BM_GETCHECK: u32 = 0x00F0;
        unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == 1 }
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn tab_moves_focus_between_controls() -> AureaResult<()> {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SetFocus(hwnd: *mut c_void) -> *mut c_void;
            fn GetFocus() -> *mut c_void;
            fn PostMessageW(hwnd: *mut c_void, msg: u32, w: usize, l: isize) -> i32;
            fn GetKeyboardState(state: *mut u8) -> i32;
            fn SetKeyboardState(state: *const u8) -> i32;
        }
        const WM_KEYDOWN: u32 = 0x0100;
        const VK_TAB: usize = 0x09;
        const VK_SHIFT: usize = 0x10;
        let mut window = Window::new("tab order", 400, 300)?;
        let field = TextField::new()?;
        let button = Button::new("Go")?;
        let checkbox = Checkbox::new("Check")?;
        let view = TextView::new(false)?;
        let editor = TextEditor::new()?;
        let order = [
            field.handle(),
            button.handle(),
            checkbox.handle(),
            view.handle(),
            editor.handle(),
        ];
        let mut root = Stack::new(Orientation::Vertical)?;
        let mut row = Stack::new(Orientation::Horizontal)?;
        row.add(field)?;
        row.add(button)?;
        root.add(row)?;
        root.add(checkbox)?;
        root.add(view)?;
        root.add(editor)?;
        window.set_content(root)?;
        window.show();
        let tab = |window: &Window| {
            unsafe { PostMessageW(GetFocus(), WM_KEYDOWN, VK_TAB, 0) };
            window.poll_events();
            unsafe { GetFocus() }
        };
        unsafe { SetFocus(order[0]) };
        for pair in order.windows(2) {
            assert_eq!(tab(&window), pair[1]);
        }
        assert_eq!(tab(&window), order[4], "the editor keeps Tab");

        let mut keys = [0u8; 256];
        unsafe { GetKeyboardState(keys.as_mut_ptr()) };
        keys[VK_SHIFT] = 0x80;
        unsafe { SetKeyboardState(keys.as_ptr()) };
        unsafe { SetFocus(order[1]) };
        let back = tab(&window);
        keys[VK_SHIFT] = 0;
        unsafe { SetKeyboardState(keys.as_ptr()) };
        assert_eq!(back, order[0], "Shift+Tab goes back");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn ampersands_show_as_typed() -> AureaResult<()> {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetWindowLongPtrW(hwnd: *mut c_void, index: i32) -> isize;
        }
        const GWL_STYLE: i32 = -16;
        const SS_NOPREFIX: isize = 0x80;
        let _window = Window::new("ampersand", 400, 300)?;
        let label = Label::new("Save & Load")?;
        let button = Button::new("Save & Load")?;
        let checkbox = Checkbox::new("Save & Load")?;
        let style = unsafe { GetWindowLongPtrW(label.handle(), GWL_STYLE) };
        assert_ne!(style & SS_NOPREFIX, 0);
        assert_eq!(native_text(label.handle()), "Save & Load");
        assert_eq!(native_text(button.handle()), "Save && Load");
        assert_eq!(native_text(checkbox.handle()), "Save && Load");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn controls_keep_unicode() -> AureaResult<()> {
        let _window = Window::new("unicode", 400, 300)?;
        let button = Button::new(TEXT)?;
        let label = Label::new(TEXT)?;
        let checkbox = Checkbox::new(TEXT)?;
        let field = TextField::with_content(TEXT)?;
        let view = TextView::with_content(true, TEXT)?;
        for hwnd in [
            button.handle(),
            label.handle(),
            checkbox.handle(),
            field.handle(),
        ] {
            assert_eq!(native_text(hwnd), TEXT);
        }
        assert_eq!(view.get_content()?, TEXT);

        unsafe { SetWindowTextW(field.handle(), wide("한글 🙂").as_ptr()) };
        assert_eq!(field.get_content()?, "한글 🙂");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn text_field_rejects_bad_initial_text() -> AureaResult<()> {
        let _window = Window::new("field", 400, 300)?;
        assert!(TextField::with_content("left\0right").is_err());
        assert_eq!(TextField::with_content("")?.get_content()?, "");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_shared_label_changes_after_adding() -> AureaResult<()> {
        let mut window = Window::new("label", 400, 300)?;
        let label = Shared::new(Label::new("Ready")?);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(label.clone())?;
        window.set_content(root)?;
        label.borrow_mut().set_text(TEXT)?;
        assert_eq!(native_text(label.handle()), TEXT);
        drop(window);
        assert_eq!(
            unsafe { IsWindow(label.handle()) },
            0,
            "label outlived its window"
        );
        assert!(label.borrow_mut().set_text("gone").is_err());
        Ok(())
    }

    type Picked = Rc<RefCell<Vec<i32>>>;

    fn tabs_in(window: &mut Window) -> AureaResult<(Shared<TabBar>, Picked)> {
        let picked = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&picked);
        let tabs = Shared::new(TabBar::with_callbacks_and_tabs(
            move |i| seen.borrow_mut().push(i),
            |_| {},
            ["General", "System", "Storage"],
        )?);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(tabs.clone())?;
        window.set_content(root)?;
        Ok((tabs, picked))
    }

    fn tab(bar: &Shared<TabBar>, i: i32) -> *mut c_void {
        unsafe { GetDlgItem(bar.handle(), 9000 + i) }
    }

    fn checked(bar: &Shared<TabBar>, i: i32) -> bool {
        unsafe { SendMessageW(tab(bar, i), BM_GETCHECK, 0, 0) == 1 }
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn tab_bar_shows_its_selection() -> AureaResult<()> {
        let mut window = Window::new("tabs", 600, 300)?;
        let (tabs, picked) = tabs_in(&mut window)?;
        assert_eq!(tabs.borrow().get_selected(), 0);
        tabs.borrow_mut().set_selected(2)?;
        assert_eq!(tabs.borrow().get_selected(), 2);
        assert!(checked(&tabs, 2) && !checked(&tabs, 0));

        let (a, b, c) = (
            rect(tab(&tabs, 0)),
            rect(tab(&tabs, 1)),
            rect(tab(&tabs, 2)),
        );
        assert!(a.right <= b.left && b.right <= c.left, "tabs overlap");
        let bar = rect(tabs.handle());
        assert!(bar.bottom - bar.top >= 20, "tab bar has no height");

        unsafe { SendMessageW(tab(&tabs, 1), BM_CLICK, 0, 0) };
        assert_eq!(*picked.borrow(), [1]);
        assert!(checked(&tabs, 1) && !checked(&tabs, 2));
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn tab_bar_removes_tabs() -> AureaResult<()> {
        let mut window = Window::new("tabs", 600, 300)?;
        let (tabs, _) = tabs_in(&mut window)?;
        tabs.borrow_mut().set_selected(1)?;
        tabs.borrow_mut().remove_tab(0)?;
        assert_eq!(tabs.borrow().get_selected(), 0, "System moved to 0");
        assert_eq!(native_text(tab(&tabs, 0)), "System");
        assert!(checked(&tabs, 0));
        assert!(tabs.borrow_mut().remove_tab(5).is_err());
        assert!(tabs.borrow_mut().set_selected(-1).is_err());
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_text_view_reports_changes() -> AureaResult<()> {
        let got = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&got);
        let mut window = Window::new("changes", 400, 300)?;
        let view = Shared::new(TextView::with_callback(true, move |s| {
            seen.borrow_mut().push(s);
        })?);
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(view.clone(), 1.0)?;
        window.set_content(root)?;
        let typed = wide(TEXT);
        unsafe { SendMessageW(view.handle(), EM_REPLACESEL, 1, typed.as_ptr() as isize) };
        assert_eq!(got.borrow().last().map(String::as_str), Some(TEXT));
        Ok(())
    }

    #[repr(C)]
    struct CharFormat {
        size: u32,
        mask: u32,
        effects: u32,
        height: i32,
        offset: i32,
        color: u32,
        charset: u8,
        pitch: u8,
        face: [u16; 32],
        rest: [u8; 26],
    }

    impl CharFormat {
        fn new() -> Self {
            Self {
                size: u32::try_from(size_of::<Self>()).unwrap_or(0),
                mask: 0,
                effects: 0,
                height: 0,
                offset: 0,
                color: 0,
                charset: 0,
                pitch: 0,
                face: [0; 32],
                rest: [0; 26],
            }
        }
    }

    const EM_GETCHARFORMAT: u32 = 0x043A;
    const EM_SETCHARFORMAT: u32 = 0x0444;
    const SCF_DEFAULT: usize = 0;
    const SCF_ALL: usize = 4;

    fn char_format(hwnd: *mut c_void) -> (u32, i32, String) {
        let mut f = CharFormat::new();
        unsafe { SendMessageW(hwnd, EM_GETCHARFORMAT, SCF_DEFAULT, (&raw mut f) as isize) };
        let face =
            String::from_utf16_lossy(&f.face[..f.face.iter().position(|&c| c == 0).unwrap_or(32)]);
        (f.color, f.height, face)
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_styled_text_view_keeps_its_look_when_attached() -> AureaResult<()> {
        const CFM_COLOR_SIZE_FACE: u32 = 0x4000_0000 | 0x8000_0000 | 0x2000_0000;
        let mut window = Window::new("text look", 400, 300)?;
        let view = TextView::with_content(false, "light on dark")?;
        let v = view.handle();
        let mut f = CharFormat::new();
        f.mask = CFM_COLOR_SIZE_FACE;
        f.height = 220;
        f.color = 0x00EC_E2D7;
        for (to, from) in f.face.iter_mut().zip("Consolas".encode_utf16()) {
            *to = from;
        }
        unsafe {
            SendMessageW(v, EM_SETCHARFORMAT, SCF_DEFAULT, (&raw mut f) as isize);
            SendMessageW(v, EM_SETCHARFORMAT, SCF_ALL, (&raw mut f) as isize);
        }
        let styled = char_format(v);
        assert_eq!(styled, (0x00EC_E2D7, 220, "Consolas".to_owned()));
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(view, 1.0)?;
        window.set_content(root)?;
        assert_eq!(char_format(v), styled);

        unsafe extern "C" {
            fn ng_windows_set_font(hwnd: *mut c_void, font: *mut c_void, redraw: i32, keep: i32);
        }
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn GetStockObject(object: i32) -> *mut c_void;
        }
        const DEFAULT_GUI_FONT: i32 = 17;
        unsafe { ng_windows_set_font(v, GetStockObject(DEFAULT_GUI_FONT), 1, 1) };
        assert_eq!(char_format(v), styled, "a new font for a new scale");
        Ok(())
    }

    struct Custom(*mut c_void);

    impl Element for Custom {
        fn handle(&self) -> *mut c_void {
            self.0
        }
        fn released_to_parent(&self) {}
        unsafe fn invalidate_platform(&self, _rect: Option<AreaRect>) {}
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn weighted_children_fill_across() -> AureaResult<()> {
        let mut window = Window::new("fill", 600, 400)?;
        let custom = unsafe {
            CreateWindowExW(
                0,
                wide("SCROLLBAR").as_ptr(),
                null_mut(),
                WS_CHILD | WS_VISIBLE,
                0,
                0,
                40,
                40,
                ng_windows_detached_parent(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        let mut root = Stack::new(Orientation::Vertical)?;
        let root_handle = root.handle();
        root.add(Label::new("title")?)?;
        root.add_weighted(Custom(custom), 1.0)?;
        window.set_content(root)?;
        let (outer, inner) = (rect(root_handle), rect(custom));
        assert!(inner.right - inner.left > (outer.right - outer.left) / 2);
        assert!(
            inner.bottom - inner.top > 200,
            "weighted child did not grow"
        );
        Ok(())
    }

    fn recorder<T: 'static>() -> (Rc<RefCell<Vec<T>>>, impl Fn(T) + 'static) {
        let got = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&got);
        (got, move |v| seen.borrow_mut().push(v))
    }

    fn as_user(msg: u32, code: usize, control: *mut c_void) {
        unsafe {
            SendMessageW(
                ng_windows_detached_parent(),
                msg,
                code << 16,
                control as isize,
            )
        };
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_sidebar_shows_its_selection() -> AureaResult<()> {
        let mut window = Window::new("sidebar", 400, 300)?;
        let (picked, on_pick) = recorder();
        let list = Shared::new(SidebarList::with_callback(on_pick)?);
        list.borrow_mut().add_section("Machines")?;
        list.borrow_mut().add_items(["one", "two", "three"], 0)?;
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add_weighted(list.clone(), 1.0)?;
        window.set_content(root)?;

        const LB_SETCURSEL: u32 = 0x0186;
        const LB_GETCURSEL: u32 = 0x0188;
        const LBN_SELCHANGE: usize = 1;
        let rows = unsafe { GetWindow(list.handle(), GW_CHILD) };
        let row = || unsafe { SendMessageW(rows, LB_GETCURSEL, 0, 0) };
        list.borrow_mut().set_selected(1)?;
        assert_eq!(row(), 2, "item 1 sits under the section");
        assert!(list.borrow_mut().set_selected(3).is_err());
        let click = |r: usize| unsafe {
            SendMessageW(rows, LB_SETCURSEL, r, 0);
            SendMessageW(
                list.handle(),
                WM_COMMAND,
                LBN_SELCHANGE << 16,
                rows as isize,
            );
        };
        click(3);
        assert_eq!(*picked.borrow(), [2]);
        assert_eq!(list.borrow().get_selected(), 2);
        click(0);
        assert_eq!(row(), 3, "a section took the selection");
        assert_eq!(*picked.borrow(), [2]);
        let (inner, all) = (rect(rows), rect(list.handle()));
        assert_eq!(inner.right - inner.left, all.right - all.left);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_split_divider_drags() -> AureaResult<()> {
        const WM_MOUSEMOVE: u32 = 0x0200;
        const WM_LBUTTONDOWN: u32 = 0x0201;
        const WM_LBUTTONUP: u32 = 0x0202;
        let mut window = Window::new("drag", 800, 400)?;
        let left = Label::new("left")?;
        let l = left.handle();
        let split = Shared::new(SplitView::with_children(
            SplitOrientation::Horizontal,
            left,
            Label::new("right")?,
        )?);
        window.set_content(split.clone())?;
        split.borrow().set_divider_position(0, 300.0)?;
        let width = |h| rect(h).right - rect(h).left;
        let start = width(l);
        let at = |x: i32| ((10 << 16) | (x & 0xFFFF)) as isize;
        let s = split.handle();
        unsafe {
            SendMessageW(s, WM_LBUTTONDOWN, 1, at(start + 2));
            SendMessageW(s, WM_MOUSEMOVE, 1, at(start + 202));
            SendMessageW(s, WM_LBUTTONUP, 0, at(start + 202));
        }
        assert_eq!(width(l), start + 200);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_scroll_view_scrolls_tall_content() -> AureaResult<()> {
        const WM_VSCROLL: u32 = 0x0115;
        const SB_PAGEDOWN: usize = 3;
        const SB_BOTTOM: usize = 7;
        let mut window = Window::new("scroll", 300, 200)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        for i in 0..40 {
            stack.add(Label::new(&format!("row {i}"))?)?;
        }
        let content = stack.handle();
        let scroll = Shared::new(ScrollView::with_content(stack)?);
        window.set_content(scroll.clone())?;
        let s = scroll.handle();
        let (view, top) = (rect(s), rect(content));
        assert_eq!(top.top, view.top);
        assert!(top.bottom - top.top > 4 * (view.bottom - view.top));

        unsafe { SendMessageW(s, WM_VSCROLL, SB_PAGEDOWN, 0) };
        assert!(rect(content).top < view.top);
        unsafe { SendMessageW(s, WM_VSCROLL, SB_BOTTOM, 0) };
        assert_eq!(rect(content).bottom, view.bottom);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_dpi_change_relays_out_scrolled_content() -> AureaResult<()> {
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn CreateFontW(
                height: i32,
                width: i32,
                escapement: i32,
                orientation: i32,
                weight: i32,
                italic: u32,
                underline: u32,
                strike: u32,
                charset: u32,
                precision: u32,
                clip: u32,
                quality: u32,
                pitch: u32,
                face: *const u16,
            ) -> *mut c_void;
        }
        const WM_SETFONT: u32 = 0x0030;
        const WM_DPICHANGED: u32 = 0x02E0;
        let mut window = Window::new("dpi scroll", 300, 200)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        let mut labels = Vec::new();
        for i in 0..10 {
            let label = Label::new(&format!("row {i}"))?;
            labels.push(label.handle());
            stack.add(label)?;
        }
        let content = stack.handle();
        window.set_content(Shared::new(ScrollView::with_content(stack)?))?;
        let height = |h| rect(h).bottom - rect(h).top;
        let before = height(content);

        const WM_GETFONT: u32 = 0x0031;
        let mut logfont = [0i32; 23];
        unsafe {
            let font = SendMessageW(labels[0], WM_GETFONT, 0, 0) as *mut c_void;
            GetObjectW(font, 92, logfont.as_mut_ptr().cast());
        }
        let three_times = -3 * logfont[0].abs();
        let big = unsafe {
            CreateFontW(
                three_times,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                1,
                0,
                0,
                0,
                0,
                null_mut(),
            )
        };
        for label in labels {
            unsafe { SendMessageW(label, WM_SETFONT, big as usize, 0) };
        }
        let root = window.handle();
        let mut at = rect(root);
        unsafe { SendMessageW(root, WM_DPICHANGED, 0x0060_0060, (&raw mut at) as isize) };
        assert!(height(content) > before);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_scroll_view_reveals_the_focused_control() -> AureaResult<()> {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SetFocus(hwnd: *mut c_void) -> *mut c_void;
        }
        let mut window = Window::new("scroll focus", 300, 200)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        for i in 0..20 {
            stack.add(Button::new(&format!("button {i}"))?)?;
        }
        let last = Button::new("last")?;
        let l = last.handle();
        stack.add(last)?;
        let scroll = Shared::new(ScrollView::with_content(stack)?);
        window.set_content(scroll.clone())?;
        let view = rect(scroll.handle());
        assert!(rect(l).top > view.bottom);

        window.show();
        unsafe { SetFocus(l) };
        for _ in 0..5 {
            window.poll_events();
        }
        assert!(rect(l).top >= view.top && rect(l).bottom <= view.bottom);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_scroll_bar_shows_by_policy() -> AureaResult<()> {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetWindowLongPtrW(hwnd: *mut c_void, index: i32) -> isize;
        }
        const GWL_STYLE: i32 = -16;
        const WS_VSCROLL: isize = 0x0020_0000;
        let mut window = Window::new("scroll policy", 300, 400)?;
        let scroll = Shared::new(ScrollView::with_content(Label::new("short")?)?);
        window.set_content(scroll.clone())?;
        let s = scroll.handle();
        let bar = || unsafe { GetWindowLongPtrW(s, GWL_STYLE) } & WS_VSCROLL != 0;
        assert!(!bar());
        scroll.borrow_mut().set_scrollbar(ScrollbarPolicy::Always)?;
        assert!(bar());
        scroll.borrow_mut().set_scrollbar(ScrollbarPolicy::Auto)?;
        assert!(!bar());

        let mut stack = Stack::new(Orientation::Vertical)?;
        for i in 0..40 {
            stack.add(Label::new(&format!("row {i}"))?)?;
        }
        scroll.borrow_mut().set_content(stack)?;
        assert!(bar());
        Ok(())
    }

    fn font_fits(field: *mut c_void) -> (i32, i32) {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetClientRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
        }
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn GetTextMetricsW(dc: *mut c_void, metrics: *mut i32) -> i32;
        }
        const WM_GETFONT: u32 = 0x0031;
        let mut client = Rect::default();
        let mut metrics = [0i32; 15];
        unsafe {
            GetClientRect(field, &raw mut client);
            let dc = GetDC(field);
            let font = SendMessageW(field, WM_GETFONT, 0, 0) as *mut c_void;
            let old = SelectObject(dc, font);
            GetTextMetricsW(dc, metrics.as_mut_ptr());
            SelectObject(dc, old);
            ReleaseDC(field, dc);
        }
        (client.bottom - client.top, metrics[0])
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_lone_text_field_fits_its_font() -> AureaResult<()> {
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn CreateFontW(
                height: i32,
                width: i32,
                escapement: i32,
                orientation: i32,
                weight: i32,
                italic: u32,
                underline: u32,
                strike: u32,
                charset: u32,
                precision: u32,
                clip: u32,
                quality: u32,
                pitch: u32,
                face: *const u16,
            ) -> *mut c_void;
        }
        const WM_SETFONT: u32 = 0x0030;
        const WM_DPICHANGED: u32 = 0x02E0;
        let mut window = Window::new("field height", 400, 300)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        let field = TextField::with_content("Agjpqy 한글 日本語")?;
        let f = field.handle();
        stack.add(field)?;
        window.set_content(stack)?;
        let (client, font) = font_fits(f);
        assert!(client >= font, "{client} < {font}");

        let big = unsafe { CreateFontW(-40, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, null_mut()) };
        unsafe { SendMessageW(f, WM_SETFONT, big as usize, 0) };
        let root = window.handle();
        let mut at = rect(root);
        unsafe { SendMessageW(root, WM_DPICHANGED, 0x0060_0060, (&raw mut at) as isize) };
        let (client, font) = font_fits(f);
        assert!(client >= font, "{client} < {font} after refont");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_canvas_keeps_its_logical_size() -> AureaResult<()> {
        use crate::render::{Canvas, RendererBackend};
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetDpiForWindow(hwnd: *mut c_void) -> u32;
        }
        let mut window = Window::new("canvas size", 400, 400)?;
        let mut stack = Stack::new(Orientation::Horizontal)?;
        let canvas = Canvas::new(120, 80, RendererBackend::Cpu)?;
        let c = canvas.handle();
        stack.add(canvas)?;
        stack.add(Label::new("after")?)?;
        window.set_content(stack)?;
        let dpi = unsafe { GetDpiForWindow(c) } as i32;
        let r = rect(c);
        assert_eq!(r.right - r.left, 120 * dpi / 96);
        assert_eq!(r.bottom - r.top, 80 * dpi / 96);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_weighted_field_keeps_its_height_in_a_row() -> AureaResult<()> {
        let mut window = Window::new("field row", 400, 300)?;
        let mut row = Stack::new(Orientation::Horizontal)?;
        let field = TextField::with_content("Agjpqy")?;
        let button = Button::new("Go")?;
        let (f, b) = (field.handle(), button.handle());
        row.add_weighted(field, 1.0)?;
        row.add(button)?;
        window.set_content(row)?;
        let (field, button) = (rect(f), rect(b));
        let (client, font) = font_fits(f);
        assert!(client >= font, "{client} < {font}");
        assert!(field.bottom - field.top < button.bottom - button.top);
        assert!(field.top >= button.top && field.bottom <= button.bottom);
        Ok(())
    }

    #[repr(C)]
    #[derive(Default)]
    struct Variant {
        vt: u16,
        reserved: [u16; 3],
        value: i32,
        rest: [u32; 3],
    }

    #[derive(Debug)]
    struct Accessible {
        role: i32,
        name: String,
        value: String,
        state: i32,
    }

    /// What a screen reader reads through MSAA.
    fn accessible(hwnd: *mut c_void) -> Accessible {
        type Get = unsafe extern "system" fn(*mut c_void, Variant, *mut Variant) -> i32;
        type GetText = unsafe extern "system" fn(*mut c_void, Variant, *mut *mut u16) -> i32;
        #[link(name = "oleacc")]
        unsafe extern "system" {
            fn AccessibleObjectFromWindow(
                hwnd: *mut c_void,
                object: u32,
                iid: *const [u8; 16],
                out: *mut *mut c_void,
            ) -> i32;
        }
        #[link(name = "oleaut32")]
        unsafe extern "system" {
            fn SysStringLen(text: *const u16) -> u32;
            fn SysFreeString(text: *mut u16);
        }
        use std::mem::transmute;
        use std::slice::from_raw_parts;
        const OBJID_CLIENT: u32 = 0xFFFF_FFFC;
        const VT_I4: u16 = 3;
        const IID_IACCESSIBLE: [u8; 16] = [
            0xe0, 0x36, 0x87, 0x61, 0x3d, 0x3c, 0xcf, 0x11, 0x81, 0x0c, 0x00, 0xaa, 0x00, 0x38,
            0x9b, 0x71,
        ];
        let me = || Variant {
            vt: VT_I4,
            ..Variant::default()
        };
        unsafe {
            let mut acc = null_mut();
            assert_eq!(
                AccessibleObjectFromWindow(hwnd, OBJID_CLIENT, &IID_IACCESSIBLE, &raw mut acc),
                0
            );
            let vtable = *(acc as *const *const usize);
            let release: unsafe extern "system" fn(*mut c_void) -> u32 = transmute(*vtable.add(2));
            let text = |slot: usize| {
                let get: GetText = transmute(*vtable.add(slot));
                let mut text = null_mut();
                if get(acc, me(), &raw mut text) != 0 || text.is_null() {
                    return String::new();
                }
                let read =
                    String::from_utf16_lossy(from_raw_parts(text, SysStringLen(text) as usize));
                SysFreeString(text);
                read
            };
            let number = |slot: usize| {
                let get: Get = transmute(*vtable.add(slot));
                let mut out = Variant::default();
                get(acc, me(), &raw mut out);
                out.value
            };
            let read = Accessible {
                role: number(13),
                name: text(10),
                value: text(11),
                state: number(14),
            };
            release(acc);
            read
        }
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn screen_readers_skip_stacks_and_read_slider_values() -> AureaResult<()> {
        const ROLE_SYSTEM_PANE: i32 = 0x10;
        const ROLE_SYSTEM_SLIDER: i32 = 0x33;
        let mut window = Window::new("accessible", 400, 300)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        let slider = Slider::with_value(512.0, 16384.0, 4096.0)?;
        let (st, s) = (stack.handle(), slider.handle());
        stack.add(slider)?;
        window.set_content(stack)?;
        assert_eq!(accessible(st).role, ROLE_SYSTEM_PANE);
        let slider = accessible(s);
        assert_eq!(
            (slider.role, slider.value.as_str()),
            (ROLE_SYSTEM_SLIDER, "4096")
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn screen_readers_skip_empty_labels() -> AureaResult<()> {
        const ROLE_SYSTEM_PANE: i32 = 0x10;
        const ROLE_SYSTEM_STATICTEXT: i32 = 0x29;
        let mut window = Window::new("spacers", 400, 300)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        let spacer = Spacer::new()?;
        let label = Shared::new(Label::new("shown")?);
        let (sp, l) = (spacer.handle(), label.handle());
        stack.add(spacer)?;
        stack.add(label.clone())?;
        window.set_content(stack)?;
        assert_eq!(accessible(sp).role, ROLE_SYSTEM_PANE);
        assert_eq!(accessible(l).role, ROLE_SYSTEM_STATICTEXT);
        label.borrow_mut().set_text("")?;
        assert_eq!(accessible(l).role, ROLE_SYSTEM_PANE);
        label.borrow_mut().set_text("back")?;
        assert_eq!(accessible(l).role, ROLE_SYSTEM_STATICTEXT);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn tabs_read_as_page_tabs() -> AureaResult<()> {
        const ROLE_SYSTEM_PAGETAB: i32 = 0x25;
        const ROLE_SYSTEM_PAGETABLIST: i32 = 0x3C;
        const STATE_SYSTEM_SELECTED: i32 = 0x2;
        const STATE_SYSTEM_CHECKED: i32 = 0x10;
        const GW_CHILD: u32 = 5;
        const GW_HWNDNEXT: u32 = 2;
        let mut window = Window::new("tabs", 400, 300)?;
        let mut tabs = TabBar::new()?;
        tabs.add_tab("One")?;
        tabs.add_tab("Two")?;
        let bar = tabs.handle();
        let tabs = Shared::new(tabs);
        window.set_content(tabs.clone())?;
        tabs.borrow_mut().set_selected(1)?;
        assert_eq!(accessible(bar).role, ROLE_SYSTEM_PAGETABLIST);
        let first = unsafe { GetWindow(bar, GW_CHILD) };
        let (one, two) = (
            accessible(first),
            accessible(unsafe { GetWindow(first, GW_HWNDNEXT) }),
        );
        assert_eq!((one.role, one.name.as_str()), (ROLE_SYSTEM_PAGETAB, "One"));
        assert_eq!(one.state & STATE_SYSTEM_SELECTED, 0);
        assert_eq!(
            two.state & (STATE_SYSTEM_SELECTED | STATE_SYSTEM_CHECKED),
            STATE_SYSTEM_SELECTED
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn an_element_takes_an_accessible_name() -> AureaResult<()> {
        let mut window = Window::new("names", 400, 300)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        let editor = TextEditor::new()?;
        let button = Button::new("+")?;
        editor.set_accessible_name("Source")?;
        button.set_accessible_name("New tab")?;
        let (e, b) = (editor.handle(), button.handle());
        stack.add_weighted(editor, 1.0)?;
        stack.add(button)?;
        window.set_content(stack)?;
        assert_eq!(accessible(e).name, "Source");
        assert_eq!(accessible(b).name, "New tab");
        Shared::new(Button::new("x")?).set_accessible_name("")?;

        let table = Table::new(["Adapter"])?;
        table.set_accessible_name("Adapters")?;
        assert_eq!(
            accessible(unsafe { GetWindow(table.handle(), 5) }).name,
            "Adapters"
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_slider_thumb_fits() -> AureaResult<()> {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SetWindowPos(
                hwnd: *mut c_void,
                after: *mut c_void,
                x: i32,
                y: i32,
                w: i32,
                h: i32,
                flags: u32,
            ) -> i32;
        }
        const TBM_GETTHUMBRECT: u32 = 0x0419;
        const SWP_NOMOVE_NOZORDER: u32 = 0x0006;
        let mut window = Window::new("slider height", 400, 300)?;
        let mut stack = Stack::new(Orientation::Vertical)?;
        let slider = Slider::new(0.0, 1.0)?;
        let s = slider.handle();
        stack.add(slider)?;
        window.set_content(stack)?;
        let thumb = || {
            let mut t = Rect::default();
            unsafe { SendMessageW(s, TBM_GETTHUMBRECT, 0, (&raw mut t) as isize) };
            t.bottom - t.top
        };
        let (laid_out, r) = (thumb(), rect(s));
        unsafe {
            SetWindowPos(
                s,
                null_mut(),
                0,
                0,
                r.right - r.left,
                400,
                SWP_NOMOVE_NOZORDER,
            )
        };
        assert_eq!(laid_out, thumb(), "thumb squashed");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_disabled_stack_greys_its_children() -> AureaResult<()> {
        let mut window = Window::new("enabled", 400, 300)?;
        let button = Button::new("Go")?;
        let field = TextField::new()?;
        let (b, f) = (button.handle(), field.handle());
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(button)?;
        root.add(field)?;
        let root = Shared::new(root);
        window.set_content(root.clone())?;
        root.set_enabled(false)?;
        assert!(unsafe { IsWindowEnabled(b) == 0 && IsWindowEnabled(f) == 0 });
        root.set_enabled(true)?;
        assert!(unsafe { IsWindowEnabled(b) != 0 && IsWindowEnabled(f) != 0 });
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_shared_element_hears_its_window_close() -> AureaResult<()> {
        let mut window = Window::new("destroy", 400, 300)?;
        let label = Shared::new(Label::new("Ready")?);
        let (heard, on_heard) = recorder();
        label.on_destroyed(move || on_heard(()));
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(label.clone())?;
        window.set_content(root)?;
        assert!(label.is_alive());
        drop(window);
        assert_eq!(heard.borrow().len(), 1);
        assert!(!label.is_alive());
        assert!(matches!(
            label.try_borrow_mut(),
            Err(AureaError::ElementDestroyed)
        ));
        drop(Shared::new(Label::new("loose")?));
        Ok(())
    }

    fn bmp(w: u32, h: u32, bgr: [u8; 3]) -> Vec<u8> {
        let row = (w * 3).div_ceil(4) * 4;
        let size = 54 + row * h;
        let mut out = Vec::new();
        out.extend_from_slice(b"BM");
        for v in [size, 0, 54, 40] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&[0; 24]);
        for _ in 0..h {
            let start = out.len();
            for _ in 0..w {
                out.extend_from_slice(&bgr);
            }
            out.resize(start + row as usize, 0);
        }
        out
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn an_image_keeps_its_aspect() -> AureaResult<()> {
        let _window = Window::new("image", 400, 300)?;
        let view = ImageView::with_data(&bmp(40, 20, [0, 0, 255]))?;
        let pixel = |x, y| unsafe {
            let dc = CreateCompatibleDC(null_mut());
            let screen = GetDC(null_mut());
            let bitmap = CreateCompatibleBitmap(screen, 200, 150);
            ReleaseDC(null_mut(), screen);
            let old = SelectObject(dc, bitmap);
            SendMessageW(view.handle(), WM_PRINTCLIENT, dc as usize, PRF_CLIENT);
            let color = GetPixel(dc, x, y);
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            color
        };
        assert_eq!(pixel(100, 75), 0x0000_00FF, "no image in the middle");
        assert_ne!(pixel(100, 5), 0x0000_00FF, "stretched over the letterbox");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn line_breaks_round_trip() -> AureaResult<()> {
        let _window = Window::new("lines", 400, 300)?;
        let mut view = TextView::new(true)?;
        view.set_content(
            "one
two
",
        )?;
        assert_eq!(
            view.get_content()?,
            "one
two
"
        );
        let mut editor = TextEditor::new()?;
        editor.set_content(
            "a
b",
        )?;
        assert_eq!(
            editor.get_content()?,
            "a
b"
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn long_text_is_kept_whole() -> AureaResult<()> {
        let _window = Window::new("long", 400, 300)?;
        let text = "0123456789".repeat(10_000);
        let mut view = TextView::new(true)?;
        view.set_content(&text)?;
        assert_eq!(view.get_content()?.len(), text.len());
        let mut editor = TextEditor::new()?;
        let text = "a".repeat(32_767);
        editor.set_content(&text)?;
        assert_eq!(editor.get_content()?.len(), text.len());
        unsafe {
            SendMessageW(editor.handle(), EM_SETSEL, usize::MAX >> 1, -1);
            SendMessageW(editor.handle(), WM_CHAR, usize::from(b'x'), 0);
        }
        assert_eq!(
            editor.get_content()?.len(),
            text.len() + 1,
            "typing stopped at the limit"
        );
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_heading_label_is_larger_and_bold() -> AureaResult<()> {
        const WM_GETFONT: u32 = 0x0031;
        let _window = Window::new("styles", 400, 300)?;
        let font = |label: &Label| {
            let mut logfont = [0i32; 23];
            unsafe {
                let font = SendMessageW(label.handle(), WM_GETFONT, 0, 0) as *mut c_void;
                GetObjectW(font, 92, logfont.as_mut_ptr().cast());
            }
            (logfont[0].abs(), logfont[4])
        };
        let body = Label::with_style("body", TextStyle::Body)?;
        let heading = Label::with_style("heading", TextStyle::Heading)?;
        let ((body_height, _), (heading_height, weight)) = (font(&body), font(&heading));
        assert!(
            heading_height > body_height,
            "{heading_height} vs {body_height}"
        );
        assert!(weight >= 600, "weight {weight}");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_slider_keeps_fractions() -> AureaResult<()> {
        let _window = Window::new("slider", 400, 300)?;
        let mut slider = Slider::with_value(0.0, 1.0, 0.37)?;
        assert!(
            (slider.get_value() - 0.37).abs() < 1e-3,
            "{}",
            slider.get_value()
        );
        slider.set_value(2.0)?;
        assert!((slider.get_value() - 1.0).abs() < 1e-9);
        assert!(Slider::new(f64::NAN, 1.0).is_err());
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn value_controls_report_the_user() -> AureaResult<()> {
        let mut window = Window::new("changes", 400, 300)?;
        let (checks, on_check) = recorder();
        let (picks, on_pick) = recorder();
        let (moves, on_move) = recorder();
        let mut check = Checkbox::new("Share")?;
        check.on_change(on_check)?;
        let mut combo = ComboBox::with_items(["a", "b", "c"])?;
        combo.on_change(on_pick)?;
        let mut slider = Slider::new(0.0, 1.0)?;
        slider.on_change(on_move)?;
        let (c, k, s) = (check.handle(), combo.handle(), slider.handle());
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(check)?;
        root.add(combo)?;
        root.add(slider)?;
        window.set_content(root)?;

        unsafe { SendMessageW(c, BM_CLICK, 0, 0) };
        assert_eq!(*checks.borrow(), [true]);
        unsafe { SendMessageW(k, CB_SETCURSEL, 2, 0) };
        as_user(WM_COMMAND, CBN_SELCHANGE, k);
        assert_eq!(*picks.borrow(), [2]);
        unsafe { SendMessageW(s, TBM_SETPOS, 1, 2500) };
        as_user(WM_HSCROLL, 0, s);
        assert_eq!(moves.borrow().len(), 1);
        assert!((moves.borrow()[0] - 0.25).abs() < 1e-3);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_hidden_child_leaves_no_gap() -> AureaResult<()> {
        let mut window = Window::new("hidden", 400, 300)?;
        let first = Shared::new(Label::new("first")?);
        let second = Label::new("second")?;
        let s = second.handle();
        let mut root = Stack::new(Orientation::Vertical)?;
        let r = root.handle();
        root.set_padding(0)?;
        root.add(first.clone())?;
        root.add(second)?;
        window.set_content(root)?;
        let before = rect(s).top;
        first.set_visible(false)?;
        assert!(rect(s).top < before, "the gap stayed");
        assert_eq!(rect(s).top, rect(r).top, "padding 0 still padded");
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn a_min_size_holds() -> AureaResult<()> {
        let mut window = Window::new("min", 400, 300)?;
        let button = Button::new("Go")?;
        let b = button.handle();
        button.set_min_size(0, 90)?;
        let mut root = Stack::new(Orientation::Vertical)?;
        root.add(button)?;
        window.set_content(root)?;
        assert!(rect(b).bottom - rect(b).top >= 90);
        Ok(())
    }

    #[test]
    #[ignore = "creates native windows; run with --ignored"]
    fn split_panes_lay_out_their_stacks() -> AureaResult<()> {
        let mut window = Window::new("split", 800, 400)?;
        let view = TextView::new(true)?;
        let inner = view.handle();
        let mut pane = Stack::new(Orientation::Vertical)?;
        pane.add(Label::new("left")?)?;
        pane.add_weighted(view, 1.0)?;
        let split = Shared::new(SplitView::with_children(
            SplitOrientation::Horizontal,
            pane,
            Label::new("right")?,
        )?);
        window.set_content(split.clone())?;
        split.borrow().set_divider_position(0, 300.0)?;
        let narrow = rect(inner).right - rect(inner).left;
        split.borrow().set_divider_position(0, 600.0)?;
        let wide = rect(inner).right - rect(inner).left;
        assert!(
            wide - narrow >= 250,
            "pane stack kept {narrow} px, then {wide} px"
        );
        Ok(())
    }
}
