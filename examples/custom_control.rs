//! A native control Aurea has no element for, embedded in a Stack: a month calendar.

use std::os::raw::c_void;

use aurea::elements::{Container, Element, Label, NativeElement, Orientation, Shared, Stack};
use aurea::render::Rect;
use aurea::{AureaError, AureaResult, Window};

struct Calendar(NativeElement);

impl Calendar {
    fn new() -> AureaResult<Self> {
        let handle = native_calendar(NativeElement::creation_parent());
        if handle.is_null() {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(Self(NativeElement::new(handle)))
    }
}

impl Element for Calendar {
    fn handle(&self) -> *mut c_void {
        self.0.handle()
    }

    fn released_to_parent(&self) {
        self.0.released_to_parent();
    }

    unsafe fn invalidate_platform(&self, _rect: Option<Rect>) {}
}

#[cfg(target_os = "windows")]
fn native_calendar(parent: *mut c_void) -> *mut c_void {
    use std::ptr::null_mut;

    #[repr(C)]
    struct InitCommonControls {
        size: u32,
        classes: u32,
    }

    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn InitCommonControlsEx(init: *const InitCommonControls) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
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

    const ICC_DATE_CLASSES: u32 = 0x100;
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    let class: Vec<u16> = "SysMonthCal32".encode_utf16().chain([0]).collect();
    let init = InitCommonControls {
        size: 8,
        classes: ICC_DATE_CLASSES,
    };
    unsafe {
        InitCommonControlsEx(&raw const init);
        CreateWindowExW(
            0,
            class.as_ptr(),
            null_mut(),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            240,
            180,
            parent,
            null_mut(),
            null_mut(),
            null_mut(),
        )
    }
}

#[cfg(target_os = "linux")]
fn native_calendar(_parent: *mut c_void) -> *mut c_void {
    unsafe extern "C" {
        fn gtk_calendar_new() -> *mut c_void;
        fn gtk_widget_show(widget: *mut c_void);
    }
    unsafe {
        let calendar = gtk_calendar_new();
        gtk_widget_show(calendar);
        calendar
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn native_calendar(_parent: *mut c_void) -> *mut c_void {
    std::ptr::null_mut()
}

fn main() -> AureaResult<()> {
    let mut window = Window::new("Custom control", 420, 320)?;
    let calendar = Shared::new(Calendar::new()?);
    calendar.on_destroyed(|| println!("calendar went with its window"));

    let mut root = Stack::new(Orientation::Vertical)?;
    root.add(Label::new("A native calendar inside a Stack")?)?;
    root.add_weighted(calendar.clone(), 1.0)?;
    window.set_content(root)?;
    window.show();
    window.run()?;
    drop(window);
    println!("calendar alive after close: {}", calendar.is_alive());
    Ok(())
}
