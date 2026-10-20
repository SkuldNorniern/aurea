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
mod shared;
mod sidebar_list;
mod slider;
mod spacer;
mod split_view;
mod swiftui_host;
mod tab_bar;
mod text_editor;
mod text_field;
mod text_view;
mod traits;

pub use button::Button;
pub use checkbox::Checkbox;
pub use combo_box::ComboBox;
pub use container::{Orientation, Stack};
pub use divider::Divider;
pub use image_view::{ImageScaling, ImageView};
pub use label::Label;
pub use native::NativeElement;
pub use progress_bar::ProgressBar;
pub use shared::Shared;
pub use sidebar_list::SidebarList;
pub use slider::Slider;
pub use spacer::Spacer;
pub use split_view::{SplitOrientation, SplitView};
pub use swiftui_host::{SwiftUIHost, SwiftUIHostNotAvailable};
pub use tab_bar::TabBar;
pub use text_editor::TextEditor;
pub use text_field::TextField;
pub use text_view::TextView;
pub use traits::{Container, Element};

#[cfg(all(test, target_os = "linux"))]
mod gtk_tests {
    //! Native GTK widgets, read back through GTK.

    use super::*;
    use crate::{AureaResult, Window};
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
    use crate::{AureaResult, Window};
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
        fn IsWindow(hwnd: *mut c_void) -> i32;
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

    const TEXT: &str = "연결 · 日本語 🙂 …";
    const BM_GETCHECK: u32 = 0x00F0;
    const BM_CLICK: u32 = 0x00F5;
    const EM_REPLACESEL: u32 = 0x00C2;
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;

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
