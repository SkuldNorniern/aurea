//! An icon in the notification area ("system tray"), or on macOS the menu bar's status area.

use crate::ffi::{c_text, c_title};
use std::os::raw::c_void;
use std::ptr;

use crate::ffi::*;
use crate::platform::ui_thread;
use crate::registry::menu::{next_menu_item_id, register_menu_callback, unregister_menu_callback};
use crate::{AureaError, AureaResult, Platform};

/// A notification area icon with a tooltip and a menu.
pub struct TrayIcon {
    handle: *mut c_void,
    ids: Vec<u32>,
}

impl TrayIcon {
    /// Adds an icon showing `tooltip` when hovered; `on_click` runs on a left click (or Enter on
    /// the icon).
    pub fn new(tooltip: &str, on_click: impl Fn() + 'static) -> AureaResult<Self> {
        ui_thread::check("TrayIcon::new");
        if !cfg!(any(windows, target_os = "macos", target_os = "linux")) {
            return Err(AureaError::Unsupported {
                operation: "add a tray icon",
                platform: Platform::current(),
            });
        }
        let tooltip = c_title(tooltip)?;
        let click = next_menu_item_id();
        let handle = unsafe { ng_platform_create_tray(tooltip.as_ptr(), click) };
        if handle.is_null() {
            // Linux: no StatusNotifierWatcher on the session bus, so no host shows the icon.
            return Err(if cfg!(target_os = "linux") {
                AureaError::Unsupported {
                    operation: "add a tray icon",
                    platform: Platform::current(),
                }
            } else {
                AureaError::ElementOperationFailed
            });
        }
        register_menu_callback(click, on_click);
        Ok(Self {
            handle,
            ids: vec![click],
        })
    }

    /// The icon, from tightly packed RGBA8 pixels; 16 to 32 pixels a side suits most screens, and
    /// both platforms scale others.
    pub fn set_icon_rgba(&self, rgba: &[u8], width: u32, height: u32) -> AureaResult<()> {
        ui_thread::check("TrayIcon::set_icon_rgba");
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(AureaError::ElementOperationFailed)?;
        if width == 0 || height == 0 || rgba.len() != expected {
            return Err(AureaError::ElementOperationFailed);
        }
        let result =
            unsafe { ng_platform_tray_set_icon_rgba(self.handle, rgba.as_ptr(), width, height) };
        if result != 0 {
            return Err(AureaError::PlatformError(result));
        }
        Ok(())
    }

    /// The text shown when the pointer rests on the icon; Windows keeps the first 127 characters.
    pub fn set_tooltip(&self, tooltip: &str) -> AureaResult<()> {
        ui_thread::check("TrayIcon::set_tooltip");
        let tooltip = c_title(tooltip)?;
        let result = unsafe { ng_platform_tray_set_tooltip(self.handle, tooltip.as_ptr()) };
        if result != 0 {
            return Err(AureaError::PlatformError(result));
        }
        Ok(())
    }

    /// Adds an item to the menu a right click opens.
    pub fn add_item(&mut self, title: &str, callback: impl Fn() + 'static) -> AureaResult<()> {
        ui_thread::check("TrayIcon::add_item");
        let title = c_title(title)?;
        let id = next_menu_item_id();
        let result = unsafe { ng_platform_tray_add_item(self.handle, title.as_ptr(), id) };
        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }
        register_menu_callback(id, callback);
        self.ids.push(id);
        Ok(())
    }

    /// Adds a line between menu items.
    pub fn add_separator(&mut self) -> AureaResult<()> {
        ui_thread::check("TrayIcon::add_separator");
        let result = unsafe { ng_platform_tray_add_item(self.handle, ptr::null(), 0) };
        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }
        Ok(())
    }

    /// Shows a notification from the icon, like "still running here". Fails where the user has
    /// turned the app's notifications off.
    pub fn notify(&self, title: &str, body: &str) -> AureaResult<()> {
        ui_thread::check("TrayIcon::notify");
        let title = c_title(title)?;
        let body = c_text(body)?;
        let result = unsafe { ng_platform_tray_notify(self.handle, title.as_ptr(), body.as_ptr()) };
        if result != 0 {
            return Err(AureaError::PlatformError(result));
        }
        Ok(())
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        for id in &self.ids {
            unregister_menu_callback(*id);
        }
        unsafe {
            ng_platform_destroy_tray(self.handle);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::TrayIcon;
    use crate::{AureaResult, Window};

    #[test]
    #[ignore = "puts an icon in the notification area; run with --ignored"]
    fn a_tray_icon_takes_items_and_tooltips() -> AureaResult<()> {
        let _window = Window::new("tray", 100, 100)?;
        let mut tray = TrayIcon::new("Aurea test", || {})?;
        tray.add_item("Save & Load", || {})?;
        tray.add_separator()?;
        tray.set_tooltip("Aurea test 2")?;
        tray.set_icon_rgba(&[255; 16 * 16 * 4], 16, 16)?;
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    //! The item on a session bus: a stand-in watcher takes its registration, and a client reads
    //! its properties and drives its activate and menu.

    use super::TrayIcon;
    use crate::{AureaError, AureaResult, Window};
    use std::cell::Cell;
    use std::env;
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_uint, c_void};
    use std::process::Command;
    use std::ptr::{null, null_mut};
    use std::rc::Rc;
    use std::slice;
    use std::sync::mpsc::{self, TryRecvError};
    use std::thread;
    use std::time::{Duration, Instant};

    unsafe extern "C" {
        fn g_bus_get_sync(
            bus_type: c_int,
            cancellable: *mut c_void,
            error: *mut *mut c_void,
        ) -> *mut c_void;
        fn g_main_context_new() -> *mut c_void;
        fn g_main_context_push_thread_default(context: *mut c_void);
        fn g_main_context_iteration(context: *mut c_void, may_block: c_int) -> c_int;
        fn g_main_loop_new(context: *mut c_void, running: c_int) -> *mut c_void;
        fn g_main_loop_run(looper: *mut c_void);
        fn g_dbus_node_info_new_for_xml(xml: *const c_char, error: *mut *mut c_void)
        -> *mut c_void;
        fn g_dbus_node_info_lookup_interface(info: *mut c_void, name: *const c_char)
        -> *mut c_void;
        fn g_dbus_connection_register_object(
            connection: *mut c_void,
            path: *const c_char,
            interface: *mut c_void,
            vtable: *const InterfaceVTable,
            user_data: *mut c_void,
            destroy: *const c_void,
            error: *mut *mut c_void,
        ) -> c_uint;
        fn g_dbus_connection_call_sync(
            connection: *mut c_void,
            bus: *const c_char,
            path: *const c_char,
            interface: *const c_char,
            method: *const c_char,
            params: *mut c_void,
            reply_type: *const c_void,
            flags: c_int,
            timeout: c_int,
            cancellable: *mut c_void,
            error: *mut *mut c_void,
        ) -> *mut c_void;
        fn g_dbus_method_invocation_return_value(invocation: *mut c_void, value: *mut c_void);
        fn g_variant_new(format: *const c_char, ...) -> *mut c_void;
        fn g_variant_get(value: *mut c_void, format: *const c_char, ...);
        fn g_variant_get_child_value(value: *mut c_void, index: usize) -> *mut c_void;
        fn g_variant_get_size(value: *mut c_void) -> usize;
        fn g_variant_get_data(value: *mut c_void) -> *const u8;
        fn g_variant_unref(value: *mut c_void);
    }

    /// GDBusInterfaceVTable. The two property slots stay null, so they are plain words here and
    /// the table can be a static.
    #[repr(C)]
    struct InterfaceVTable {
        method_call: unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            *const c_char,
            *const c_char,
            *const c_char,
            *mut c_void,
            *mut c_void,
            *mut c_void,
        ),
        get_property: usize,
        set_property: usize,
        padding: [usize; 8],
    }

    /// The start of GError, for its message.
    #[repr(C)]
    struct GError {
        _domain: u32,
        _code: c_int,
        message: *const c_char,
    }

    const SESSION_BUS: c_int = 2;
    const NAME_FLAG_DO_NOT_QUEUE: u32 = 4;

    /// Set in the child that runs a test on its own bus.
    const PRIVATE_BUS: &str = "AUREA_TRAY_TEST_PRIVATE_BUS";

    /// The tests need a session bus that no desktop shares, so each one runs again under
    /// `dbus-run-session`. Returns true in the parent, once the child has run the test; false
    /// where the test should carry on.
    fn on_private_bus(test: &str) -> bool {
        if env::var_os(PRIVATE_BUS).is_some() {
            return false;
        }
        let status = Command::new("dbus-run-session")
            .arg("--")
            .arg(env::current_exe().expect("the test binary"))
            .args(["--exact", test, "--ignored", "--nocapture"])
            .env(PRIVATE_BUS, "1")
            .status()
            .expect("dbus-run-session must be installed to test the tray");
        assert!(status.success(), "{test} failed on its own session bus");
        true
    }

    /// The watcher's one method: records the item's service name, then answers.
    #[allow(clippy::too_many_arguments)]
    unsafe extern "C" fn watcher_call(
        _connection: *mut c_void,
        _sender: *const c_char,
        _path: *const c_char,
        _interface: *const c_char,
        method: *const c_char,
        params: *mut c_void,
        invocation: *mut c_void,
        data: *mut c_void,
    ) {
        if unsafe { CStr::from_ptr(method) } == c"RegisterStatusNotifierItem" {
            let mut service: *const c_char = null();
            unsafe { g_variant_get(params, c"(&s)".as_ptr(), &mut service as *mut *const c_char) };
            let registered = unsafe { &*(data as *const mpsc::Sender<String>) };
            let name = unsafe { CStr::from_ptr(service) }
                .to_string_lossy()
                .into_owned();
            let _ = registered.send(name);
        }
        unsafe { g_dbus_method_invocation_return_value(invocation, null_mut()) };
    }

    static WATCHER_VTABLE: InterfaceVTable = InterfaceVTable {
        method_call: watcher_call,
        get_property: 0,
        set_property: 0,
        padding: [0; 8],
    };

    /// Owns `org.kde.StatusNotifierWatcher` on the session bus, served from its own thread. The
    /// receiver gets the service name of each item that registers.
    fn start_watcher() -> mpsc::Receiver<String> {
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let (registered_tx, registered_rx) = mpsc::channel::<String>();
        thread::spawn(move || unsafe {
            let context = g_main_context_new();
            g_main_context_push_thread_default(context);
            let connection = g_bus_get_sync(SESSION_BUS, null_mut(), null_mut());
            let node = g_dbus_node_info_new_for_xml(
                c"<node><interface name='org.kde.StatusNotifierWatcher'>\
                <method name='RegisterStatusNotifierItem'><arg type='s' direction='in'/></method>\
                </interface></node>"
                    .as_ptr(),
                null_mut(),
            );
            let interface =
                g_dbus_node_info_lookup_interface(node, c"org.kde.StatusNotifierWatcher".as_ptr());
            let sender = Box::into_raw(Box::new(registered_tx)) as *mut c_void;
            g_dbus_connection_register_object(
                connection,
                c"/StatusNotifierWatcher".as_ptr(),
                interface,
                &WATCHER_VTABLE,
                sender,
                null(),
                null_mut(),
            );
            let name = g_variant_new(
                c"(su)".as_ptr(),
                c"org.kde.StatusNotifierWatcher".as_ptr(),
                NAME_FLAG_DO_NOT_QUEUE,
            );
            let mut error: *mut c_void = null_mut();
            let reply = g_dbus_connection_call_sync(
                connection,
                c"org.freedesktop.DBus".as_ptr(),
                c"/org/freedesktop/DBus".as_ptr(),
                c"org.freedesktop.DBus".as_ptr(),
                c"RequestName".as_ptr(),
                name,
                null(),
                0,
                -1,
                null_mut(),
                &mut error,
            );
            let refusal = if reply.is_null() {
                format!("{:?}", CStr::from_ptr((*(error as *const GError)).message))
            } else {
                String::new()
            };
            assert!(
                !reply.is_null(),
                "the watcher name was not taken: {refusal}"
            );
            ready_tx.send(()).expect("the test waits for the watcher");
            g_main_loop_run(g_main_loop_new(context, 0));
        });
        ready_rx.recv().expect("the watcher starts");
        registered_rx
    }

    /// What a client read from the item.
    #[derive(Debug)]
    struct Snapshot {
        title: String,
        icon_width: c_int,
        icon_height: c_int,
        icon_bytes: usize,
        first_pixel: [u8; 4],
    }

    /// A synchronous call; the caller owns the reply.
    unsafe fn call(
        connection: *mut c_void,
        service: &CStr,
        path: &CStr,
        interface: &CStr,
        method: &CStr,
        params: *mut c_void,
    ) -> *mut c_void {
        let reply = unsafe {
            g_dbus_connection_call_sync(
                connection,
                service.as_ptr(),
                path.as_ptr(),
                interface.as_ptr(),
                method.as_ptr(),
                params,
                null(),
                0,
                -1,
                null_mut(),
                null_mut(),
            )
        };
        assert!(!reply.is_null(), "{method:?} failed");
        reply
    }

    /// Reads the item's properties and drives it, from a thread of its own: the UI thread
    /// serves the item, so it must keep pumping while this waits.
    fn client(service: String, activate: bool, menu_row: c_int) -> mpsc::Receiver<Snapshot> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || unsafe {
            let connection = g_bus_get_sync(SESSION_BUS, null_mut(), null_mut());
            let service = CString::new(service).expect("a bus name");
            let path = c"/StatusNotifierItem";
            let sni = c"org.kde.StatusNotifierItem";
            let properties = c"org.freedesktop.DBus.Properties";

            let get = |name: &CStr| {
                let params = g_variant_new(c"(ss)".as_ptr(), sni.as_ptr(), name.as_ptr());
                let reply = call(connection, &service, path, properties, c"Get", params);
                let mut value: *mut c_void = null_mut();
                g_variant_get(reply, c"(v)".as_ptr(), &mut value as *mut *mut c_void);
                g_variant_unref(reply);
                value
            };

            let mut title: *const c_char = null();
            g_variant_get(
                get(c"Title"),
                c"&s".as_ptr(),
                &mut title as *mut *const c_char,
            );
            let title = CStr::from_ptr(title).to_string_lossy().into_owned();

            // The first of the item's pixmaps, an (iiay).
            let icon = g_variant_get_child_value(get(c"IconPixmap"), 0);
            let mut icon_width: c_int = 0;
            let mut icon_height: c_int = 0;
            let mut bytes: *mut c_void = null_mut();
            g_variant_get(
                icon,
                c"(ii@ay)".as_ptr(),
                &mut icon_width as *mut c_int,
                &mut icon_height as *mut c_int,
                &mut bytes as *mut *mut c_void,
            );
            let icon_bytes = g_variant_get_size(bytes);
            let mut first_pixel = [0u8; 4];
            first_pixel.copy_from_slice(slice::from_raw_parts(g_variant_get_data(bytes), 4));
            let snapshot = Snapshot {
                title,
                icon_width,
                icon_height,
                icon_bytes,
                first_pixel,
            };

            if activate {
                let params = g_variant_new(c"(ii)".as_ptr(), 0 as c_int, 0 as c_int);
                let reply = call(connection, &service, path, sni, c"Activate", params);
                g_variant_unref(reply);
            }
            if menu_row > 0 {
                let kind = g_variant_new(c"i".as_ptr(), 0 as c_int);
                let params = g_variant_new(
                    c"(isvu)".as_ptr(),
                    menu_row,
                    c"clicked".as_ptr(),
                    kind,
                    0 as c_uint,
                );
                let reply = call(
                    connection,
                    &service,
                    c"/MenuBar",
                    c"com.canonical.dbusmenu",
                    c"Event",
                    params,
                );
                g_variant_unref(reply);
            }
            let _ = tx.send(snapshot);
        });
        rx
    }

    /// Runs the UI thread's pending work until `rx` has a value; None if the client died.
    fn pump_until<T>(rx: &mpsc::Receiver<T>) -> Option<T> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while unsafe { g_main_context_iteration(null_mut(), 0) } != 0 {}
            match rx.try_recv() {
                Ok(value) => return Some(value),
                Err(TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline, "the item never answered");
                    thread::sleep(Duration::from_millis(1));
                }
                Err(TryRecvError::Disconnected) => return None,
            }
        }
    }

    #[test]
    #[ignore = "needs a session bus of its own; run with --ignored"]
    fn the_item_registers_with_the_watcher_and_answers_its_clients() -> AureaResult<()> {
        if on_private_bus(
            "tray::linux_tests::the_item_registers_with_the_watcher_and_answers_its_clients",
        ) {
            return Ok(());
        }
        let _window = Window::new("tray", 100, 100)?;
        let registered = start_watcher();

        let clicks = Rc::new(Cell::new(0));
        let rows = Rc::new(Cell::new(0));
        let on_click = Rc::clone(&clicks);
        let mut tray = TrayIcon::new("Aurea tray", move || on_click.set(on_click.get() + 1))?;
        let on_row = Rc::clone(&rows);
        tray.add_item("Quit", move || on_row.set(on_row.get() + 1))?;
        tray.set_icon_rgba(&[10, 20, 30, 40].repeat(16 * 16), 16, 16)?;

        let service = registered
            .try_recv()
            .expect("the item registered before TrayIcon::new returned");
        assert!(
            service.starts_with("org.kde.StatusNotifierItem-"),
            "{service}"
        );

        let snapshot = pump_until(&client(service, true, 1)).expect("the client answered");

        assert_eq!(snapshot.title, "Aurea tray");
        assert_eq!((snapshot.icon_width, snapshot.icon_height), (16, 16));
        assert_eq!(snapshot.icon_bytes, 16 * 16 * 4);
        // ARGB32: alpha first, from RGBA (10, 20, 30, 40)
        assert_eq!(snapshot.first_pixel, [40, 10, 20, 30]);
        assert_eq!(clicks.get(), 1, "Activate did not reach the click callback");
        assert_eq!(rows.get(), 1, "the menu row did not reach its callback");
        Ok(())
    }

    #[test]
    #[ignore = "needs a session bus of its own; run with --ignored"]
    fn without_a_watcher_the_icon_is_unsupported() -> AureaResult<()> {
        if on_private_bus("tray::linux_tests::without_a_watcher_the_icon_is_unsupported") {
            return Ok(());
        }
        let _window = Window::new("tray", 100, 100)?;

        let result = TrayIcon::new("no host", || {});
        assert!(
            matches!(result, Err(AureaError::Unsupported { .. })),
            "{:?}",
            result.err()
        );
        Ok(())
    }
}
