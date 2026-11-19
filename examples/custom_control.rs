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

    const WS_CHILD: u32 = 0x4000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    let class: Vec<u16> = "SysMonthCal32".encode_utf16().chain([0]).collect();
    unsafe {
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

#[cfg(target_os = "macos")]
fn native_calendar(_parent: *mut c_void) -> *mut c_void {
    use std::ffi::c_char;
    use std::mem::transmute;

    #[repr(C)]
    struct NSRect([f64; 4]);

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }

    // NSDatePickerStyleClockAndCalendar, NSDatePickerElementFlagYearMonthDay
    const CALENDAR: usize = 1;
    const DAYS: usize = 0xe0;
    unsafe {
        let sel = |name: &[u8]| sel_registerName(name.as_ptr().cast());
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            transmute(objc_msgSend as unsafe extern "C" fn());
        let send_rect: unsafe extern "C" fn(*mut c_void, *mut c_void, NSRect) -> *mut c_void =
            transmute(objc_msgSend as unsafe extern "C" fn());
        let send_int: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) =
            transmute(objc_msgSend as unsafe extern "C" fn());
        let send_obj: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            transmute(objc_msgSend as unsafe extern "C" fn());

        let picker = send(objc_getClass(c"NSDatePicker".as_ptr()), sel(b"alloc\0"));
        let picker = send_rect(
            picker,
            sel(b"initWithFrame:\0"),
            NSRect([0.0, 0.0, 240.0, 180.0]),
        );
        if picker.is_null() {
            return picker;
        }
        send_int(picker, sel(b"setDatePickerStyle:\0"), CALENDAR);
        send_int(picker, sel(b"setDatePickerElements:\0"), DAYS);
        let now = send(objc_getClass(c"NSDate".as_ptr()), sel(b"date\0"));
        send_obj(picker, sel(b"setDateValue:\0"), now);
        picker
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn native_calendar(_parent: *mut c_void) -> *mut c_void {
    use std::ptr::null_mut;
    null_mut()
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
