use std::os::raw::{c_char, c_int, c_uint, c_void};

/// Clipboard entry: a format name and its bytes.
#[repr(C)]
pub struct RawClipboardEntry {
    /// NUL-terminated format name.
    pub format: *const c_char,
    /// Entry bytes.
    pub data: *const u8,
    /// Byte length of `data`.
    pub len: usize,
}

/// The app side of the iOS host; see `native/platform/ios/host.h`.
#[cfg(target_os = "ios")]
#[repr(C)]
pub struct NGIosHost {
    pub frame: unsafe extern "C" fn(
        width: u32,
        height: u32,
        scale: f32,
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        words: *mut *const u32,
        out_width: *mut u32,
        out_height: *mut u32,
        damage: *mut u32,
    ) -> i32,
    pub touch: unsafe extern "C" fn(action: i32, x: f32, y: f32, id: usize, pencil: i32),
    pub pen: unsafe extern "C" fn(
        phase: i32,
        x: f64,
        y: f64,
        pressure: f32,
        tilt_x: f32,
        tilt_y: f32,
        twist: f32,
        age: f64,
    ),
    pub light_theme: unsafe extern "C" fn() -> i32,
    pub wake: unsafe extern "C" fn() -> i32,
    pub background: unsafe extern "C" fn() -> u32,
    pub lifecycle: unsafe extern "C" fn(state: i32),
    pub key: unsafe extern "C" fn(code: u32, pressed: i32, modifiers: u32, text: *const c_char),
    pub preedit: unsafe extern "C" fn(text: *const c_char, cursor: c_int),
    pub pen_tap: unsafe extern "C" fn(squeeze: c_int, action: c_int),
}

#[cfg(target_os = "ios")]
#[allow(clippy::missing_safety_doc, dead_code)]
unsafe extern "C" {
    pub fn ng_ios_run_host(host: NGIosHost) -> i32;
    pub fn ng_ios_wake();
    pub fn ng_ios_gpu_view() -> *mut c_void;
    pub fn ng_ios_drop_gpu_view();
    pub fn ng_ios_show_keyboard(show: c_int);
    pub fn ng_ios_keyboard_height() -> f32;
    pub fn ng_ios_set_text_area(x: f32, y: f32, width: f32, height: f32);
    pub fn ng_ios_pick_files(
        extensions: *const c_char,
        multiple: c_int,
        done: unsafe extern "C" fn(ctx: *mut c_void, paths: *const c_char),
        ctx: *mut c_void,
    ) -> c_int;
    pub fn ng_ios_preview_path(path: *const c_char) -> c_int;
    pub fn ng_ios_reveal_path(path: *const c_char) -> c_int;
    pub fn ng_ios_open_url(url: *const c_char) -> c_int;
    pub fn ng_ios_alert(
        title: *const c_char,
        message: *const c_char,
        buttons: c_int,
        done: unsafe extern "C" fn(ctx: *mut c_void, answer: c_int),
        ctx: *mut c_void,
    ) -> c_int;
    #[allow(clippy::too_many_arguments)]
    pub fn ng_ios_share_files(
        paths: *const c_char,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        done: unsafe extern "C" fn(ctx: *mut c_void, answer: c_int),
        ctx: *mut c_void,
    ) -> c_int;
    pub fn ng_ios_export_files(
        paths: *const c_char,
        done: unsafe extern "C" fn(ctx: *mut c_void, paths: *const c_char),
        ctx: *mut c_void,
    ) -> c_int;
}

#[cfg(target_os = "android")]
#[allow(clippy::missing_safety_doc, dead_code)]
unsafe extern "C" {
    pub fn ng_android_set_activity(jvm: *mut c_void, activity: *mut c_void);
    pub fn ng_android_on_pause();
    pub fn ng_android_on_resume();
    pub fn ng_android_on_destroy();
    pub fn ng_android_on_surface_lost();
    pub fn ng_android_on_surface_recreated();
}

#[allow(clippy::missing_safety_doc, dead_code)]
unsafe extern "C" {
    /// Returns the native layer's ABI version.
    pub fn ng_platform_get_abi_version() -> c_int;

    /// Initializes the native platform layer.
    pub fn ng_platform_init() -> c_int;
    /// Tears down the native platform layer.
    pub fn ng_platform_cleanup();

    /// Runs the event loop until the app exits.
    pub fn ng_platform_run() -> c_int;
    /// Processes pending events without blocking.
    pub fn ng_platform_poll_events() -> c_int;
    /// Waits up to `timeout_ms` for events.
    pub fn ng_platform_wait_events(timeout_ms: c_int) -> c_int;
    /// Requests a frame on the next loop iteration.
    pub fn ng_platform_request_frame();
    /// Marks the current frame as finished.
    pub fn ng_platform_frame_idle();

    /// Creates a window with the default type.
    pub fn ng_platform_create_window(
        title: *const c_char,
        width: c_int,
        height: c_int,
    ) -> *mut c_void;

    /// Creates a window of the given type.
    pub fn ng_platform_create_window_with_type(
        title: *const c_char,
        width: c_int,
        height: c_int,
        window_type: c_int,
    ) -> *mut c_void;
    /// Destroys a window.
    pub fn ng_platform_destroy_window(handle: *mut c_void);

    /// Destroys an element and anything the platform frees with it.
    pub fn ng_platform_destroy_element(element: *mut c_void);

    /// Removes an element from its parent, leaving it alive and unparented.
    pub fn ng_platform_detach_element(element: *mut c_void) -> i32;

    /// Tells the platform a container has taken ownership of this element.
    pub fn ng_platform_element_released_to_parent(element: *mut c_void);
    /// Retains an element.
    pub fn ng_platform_element_retain(element: *mut c_void);
    /// Releases an element.
    pub fn ng_platform_element_release(element: *mut c_void);
    /// Sets an element's change id.
    pub fn ng_platform_element_set_change_id(element: *mut c_void, id: u32) -> c_int;
    /// Shows or hides an element.
    pub fn ng_platform_element_set_visible(element: *mut c_void, visible: c_int) -> c_int;
    /// Sets an element's minimum size.
    pub fn ng_platform_element_set_min_size(
        element: *mut c_void,
        width: c_int,
        height: c_int,
    ) -> c_int;
    /// Sets the name screen readers give an element.
    pub fn ng_platform_element_set_accessible_name(
        element: *mut c_void,
        name: *const c_char,
    ) -> c_int;
    /// Sets a box's spacing.
    pub fn ng_platform_box_set_spacing(box_handle: *mut c_void, spacing: c_int) -> c_int;
    /// Sets a box's padding.
    pub fn ng_platform_box_set_padding(box_handle: *mut c_void, padding: c_int) -> c_int;
    /// Returns the parent that new elements are created under.
    pub fn ng_platform_creation_parent() -> *mut c_void;
    /// Enables or disables an element.
    pub fn ng_platform_element_set_enabled(element: *mut c_void, enabled: c_int) -> c_int;
    /// Watches an element and reports its destruction.
    pub fn ng_platform_element_watch_destroy(element: *mut c_void, id: u32) -> c_int;
    /// Returns the text unit used for offsets.
    pub fn ng_platform_text_unit() -> c_int;
    /// Gets a text element's selection range.
    pub fn ng_platform_text_get_selection(
        text: *mut c_void,
        start: *mut c_int,
        end: *mut c_int,
    ) -> c_int;
    /// Sets a text element's selection range.
    pub fn ng_platform_text_set_selection(text: *mut c_void, start: c_int, end: c_int) -> c_int;
    /// Replaces a range of text.
    pub fn ng_platform_text_replace(
        text: *mut c_void,
        start: c_int,
        end: c_int,
        replacement: *const c_char,
    ) -> c_int;
    /// Undoes or redoes the last text change.
    pub fn ng_platform_text_undo(text: *mut c_void, redo: c_int) -> c_int;
    /// Tells whether undo or redo is available.
    pub fn ng_platform_text_can_undo(text: *mut c_void, redo: c_int) -> c_int;
    /// Turns a text feature on or off.
    pub fn ng_platform_text_set_feature(text: *mut c_void, feature: c_int, on: c_int) -> c_int;
    /// Sets a window's title.
    pub fn ng_platform_window_set_title(window: *mut c_void, title: *const c_char);
    /// Sets a window's icon from RGBA pixels.
    pub fn ng_platform_window_set_icon_rgba(
        window: *mut c_void,
        rgba: *const u8,
        width: u32,
        height: u32,
    ) -> c_int;
    /// Sets a window's size.
    pub fn ng_platform_window_set_size(window: *mut c_void, width: c_int, height: c_int);
    /// Sets the smallest content size a window can be resized to; 0 leaves an axis free.
    pub fn ng_platform_window_set_min_size(
        window: *mut c_void,
        width: c_int,
        height: c_int,
    ) -> c_int;
    /// Gets a window's size.
    pub fn ng_platform_window_get_size(window: *mut c_void, width: *mut c_int, height: *mut c_int);
    /// Asks a window to close.
    pub fn ng_platform_window_request_close(window: *mut c_void);
    /// Tells whether a window has focus.
    pub fn ng_platform_window_is_focused(window: *mut c_void) -> c_int;
    /// Shows or hides the cursor over a window.
    pub fn ng_platform_window_set_cursor_visible(window: *mut c_void, visible: c_int) -> c_int;
    /// Sets the cursor grab mode for a window.
    pub fn ng_platform_window_set_cursor_grab(window: *mut c_void, mode: c_int) -> c_int;
    /// Sets the cursor icon for a window.
    pub fn ng_platform_window_set_cursor_icon(window: *mut c_void, icon: c_int) -> c_int;
    /// Sets a custom cursor from RGBA pixels.
    pub fn ng_platform_window_set_cursor_rgba(
        window: *mut c_void,
        rgba: *const u8,
        width: c_uint,
        height: c_uint,
        hot_x: c_uint,
        hot_y: c_uint,
    ) -> c_int;
    /// Sets the IME candidate area for a window.
    pub fn ng_platform_window_set_ime_cursor_area(
        window: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> c_int;
    /// Tells whether reduced motion is requested.
    pub fn ng_platform_reduced_motion() -> c_int;
    /// Tells whether the system prefers dark: 1 dark, 0 light, -1 unknown.
    pub fn ng_platform_prefers_dark() -> c_int;
    /// Lines a wheel notch scrolls, -1 for a page, 3 where the system has no setting.
    pub fn ng_platform_wheel_lines() -> c_int;
    /// Writes the system locales into `buf`.
    pub fn ng_platform_locales(buf: *mut c_char, cap: c_int) -> c_int;
    /// Sets a window's keyboard shortcuts.
    pub fn ng_platform_window_set_shortcuts(
        window: *mut c_void,
        keycodes: *const c_uint,
        modifiers: *const c_uint,
        count: c_int,
    ) -> c_int;
    /// Returns a window's content view.
    pub fn ng_platform_window_get_content_view(window: *mut c_void) -> *mut c_void;
    #[cfg(target_os = "macos")]
    pub fn ng_macos_window_use_metal_layer(window: *mut c_void) -> c_int;
    /// Shows a window.
    pub fn ng_platform_window_show(window: *mut c_void);
    /// Hides a window.
    pub fn ng_platform_window_hide(window: *mut c_void);
    /// Sets whether closing a window hides it.
    pub fn ng_platform_window_set_hide_on_close(window: *mut c_void, hide: c_int) -> c_int;
    /// Sets whether a window's close is guarded.
    pub fn ng_platform_window_set_close_guard(window: *mut c_void, guarded: c_int);
    /// Creates a tray icon.
    pub fn ng_platform_create_tray(tooltip: *const c_char, click_id: c_uint) -> *mut c_void;
    /// Destroys a tray icon.
    pub fn ng_platform_destroy_tray(tray: *mut c_void);
    /// Sets a tray icon from RGBA pixels.
    pub fn ng_platform_tray_set_icon_rgba(
        tray: *mut c_void,
        rgba: *const u8,
        width: u32,
        height: u32,
    ) -> c_int;
    /// Sets a tray icon's tooltip.
    pub fn ng_platform_tray_set_tooltip(tray: *mut c_void, tooltip: *const c_char) -> c_int;
    /// Adds an item to a tray menu.
    pub fn ng_platform_tray_add_item(tray: *mut c_void, title: *const c_char, id: c_uint) -> c_int;
    /// Shows a tray notification.
    pub fn ng_platform_tray_notify(
        tray: *mut c_void,
        title: *const c_char,
        body: *const c_char,
    ) -> c_int;
    /// Tells whether a window is visible.
    pub fn ng_platform_window_is_visible(window: *mut c_void) -> c_int;
    /// Moves a window.
    pub fn ng_platform_window_set_position(window: *mut c_void, x: c_int, y: c_int);
    /// Gets a window's position.
    pub fn ng_platform_window_get_position(window: *mut c_void, x: *mut c_int, y: *mut c_int);

    /// Gets a window's XCB window and connection.
    #[cfg(target_os = "linux")]
    pub fn ng_platform_window_get_xcb_handle(
        window: *mut c_void,
        xcb_window: *mut u32,
        xcb_connection: *mut *mut c_void,
    ) -> c_int;
    /// Gets a window's Wayland surface and display.
    #[cfg(target_os = "linux")]
    pub fn ng_platform_window_get_wayland_handle(
        window: *mut c_void,
        surface: *mut *mut c_void,
        display: *mut *mut c_void,
    ) -> c_int;
    /// Gets a canvas's XCB window and connection.
    #[cfg(target_os = "linux")]
    pub fn ng_platform_canvas_get_xcb_handle(
        canvas: *mut c_void,
        xcb_window: *mut u32,
        xcb_connection: *mut *mut c_void,
    ) -> c_int;
    /// Gets a canvas's Wayland surface and display.
    #[cfg(target_os = "linux")]
    pub fn ng_platform_canvas_get_wayland_handle(
        canvas: *mut c_void,
        surface: *mut *mut c_void,
        display: *mut *mut c_void,
    ) -> c_int;

    /// Creates a menu.
    pub fn ng_platform_create_menu() -> *mut c_void;
    /// Destroys a menu.
    pub fn ng_platform_destroy_menu(handle: *mut c_void);
    /// Attaches a menu to a window.
    pub fn ng_platform_attach_menu(window: *mut c_void, menu: *mut c_void) -> c_int;
    /// Adds an item with callback `id` to a menu.
    pub fn ng_platform_add_menu_item(
        menu_handle: *mut c_void,
        title: *const c_char,
        id: u32,
    ) -> c_int;
    /// Adds a separator to a menu.
    pub fn ng_platform_add_menu_separator(menu_handle: *mut c_void) -> c_int;
    /// Creates a submenu under a parent.
    pub fn ng_platform_create_submenu(parent: *mut c_void, title: *const c_char) -> *mut c_void;
    /// Adds a check or radio item to a menu.
    pub fn ng_platform_add_menu_check_item(
        menu_handle: *mut c_void,
        title: *const c_char,
        id: u32,
        checked: c_int,
        radio: c_int,
    ) -> c_int;
    /// Enables or disables a menu item.
    pub fn ng_platform_menu_item_set_enabled(
        menu_handle: *mut c_void,
        id: u32,
        enabled: c_int,
    ) -> c_int;
    /// Sets a menu item's checked state.
    pub fn ng_platform_menu_item_set_checked(
        menu_handle: *mut c_void,
        id: u32,
        checked: c_int,
    ) -> c_int;
    /// Sets a menu item's title.
    pub fn ng_platform_menu_item_set_title(
        menu_handle: *mut c_void,
        id: u32,
        title: *const c_char,
    ) -> c_int;
    /// Removes a menu item.
    pub fn ng_platform_menu_item_remove(menu_handle: *mut c_void, id: u32) -> c_int;
    /// Removes all items from a menu.
    pub fn ng_platform_menu_clear(menu_handle: *mut c_void) -> c_int;

    /// Opens a file dialog; returns the chosen paths, each ending in a NUL, with an empty one
    /// after the last.
    pub fn ng_platform_file_dialog(
        window: *mut c_void,
        kind: c_int,
        title: *const c_char,
        filters: *const c_char,
        default_name: *const c_char,
        directory: *const c_char,
        multiple: c_int,
    ) -> *mut c_char;
    /// Frees paths returned by `ng_platform_file_dialog`.
    pub fn ng_platform_free_dialog_paths(paths: *mut c_char);
    /// Shows a message box; returns the pressed button.
    pub fn ng_platform_message_box(
        window: *mut c_void,
        kind: c_int,
        title: *const c_char,
        text: *const c_char,
        buttons: c_int,
    ) -> c_int;
    /// Shows a color dialog; returns the chosen color.
    pub fn ng_platform_color_dialog(
        window: *mut c_void,
        title: *const c_char,
        argb_in: c_uint,
        alpha: c_int,
        argb_out: *mut c_uint,
    ) -> c_int;

    /// Gets the clipboard text.
    pub fn ng_platform_get_clipboard_text() -> *mut c_char;
    /// Frees text returned by `ng_platform_get_clipboard_text`.
    pub fn ng_platform_free_clipboard_text(text: *mut c_char);
    /// Sets the clipboard text.
    pub fn ng_platform_set_clipboard_text(text: *const c_char) -> c_int;
    /// Lists the clipboard format names.
    pub fn ng_platform_clipboard_formats(
        max_bytes: usize,
        data: *mut *mut u8,
        len: *mut usize,
    ) -> c_int;
    /// Reads clipboard data for a format.
    pub fn ng_platform_clipboard_read(
        format: *const c_char,
        max_bytes: usize,
        data: *mut *mut u8,
        len: *mut usize,
    ) -> c_int;
    /// Writes entries to the clipboard.
    pub fn ng_platform_clipboard_write(entries: *const RawClipboardEntry, count: usize) -> c_int;
    /// Frees data returned by `ng_platform_clipboard_read`.
    pub fn ng_platform_clipboard_free(data: *mut u8);

    /// Creates a native button.
    pub fn ng_platform_create_button(title: *const c_char, id: u32) -> *mut c_void;
    /// Marks a button for redraw.
    pub fn ng_platform_button_invalidate(button: *mut c_void);
    /// Creates a native label.
    pub fn ng_platform_create_label(text: *const c_char) -> *mut c_void;
    /// Marks a label for redraw.
    pub fn ng_platform_label_invalidate(label: *mut c_void);
    /// Sets a label's text.
    pub fn ng_platform_label_set_text(label: *mut c_void, text: *const c_char) -> c_int;
    /// Sets a label's style.
    pub fn ng_platform_label_set_style(label: *mut c_void, style: c_int) -> c_int;
    /// Creates a box container.
    pub fn ng_platform_create_box(is_vertical: c_int) -> *mut c_void;
    /// Marks a box for layout.
    pub fn ng_platform_box_invalidate(box_handle: *mut c_void);
    /// Adds an element to a box with a weight.
    pub fn ng_platform_box_add(box_handle: *mut c_void, element: *mut c_void, weight: f32)
    -> c_int;
    /// Sets a window's content element.
    pub fn ng_platform_set_window_content(window: *mut c_void, content: *mut c_void) -> c_int;

    /// Creates a split view.
    pub fn ng_platform_create_split_view(is_vertical: c_int) -> *mut c_void;
    /// Adds an element to a split view.
    pub fn ng_platform_split_view_add(split_handle: *mut c_void, element: *mut c_void) -> c_int;
    /// Sets a split view divider's position.
    pub fn ng_platform_split_view_set_divider_position(
        split_handle: *mut c_void,
        index: c_int,
        position: f32,
    ) -> c_int;
    /// Creates a scroll view.
    pub fn ng_platform_create_scroll_view() -> *mut c_void;
    /// Sets a scroll view's content.
    pub fn ng_platform_scroll_view_set_content(scroll: *mut c_void, element: *mut c_void) -> c_int;
    /// Sets when a scroll view shows its scroll bar.
    pub fn ng_platform_scroll_view_set_scrollbar(scroll: *mut c_void, policy: c_int) -> c_int;

    /// Creates a text editor.
    pub fn ng_platform_create_text_editor() -> *mut c_void;
    /// Marks a text editor for redraw.
    pub fn ng_platform_text_editor_invalidate(text_editor: *mut c_void);
    /// Creates a text view.
    pub fn ng_platform_create_text_view(is_editable: c_int) -> *mut c_void;
    /// Marks a text view for redraw.
    pub fn ng_platform_text_view_invalidate(text_view: *mut c_void);
    /// Creates a text field.
    pub fn ng_platform_create_text_field() -> *mut c_void;
    /// Sets a text element's content.
    pub fn ng_platform_set_text_content(text_handle: *mut c_void, content: *const c_char) -> c_int;
    /// Gets a text element's content.
    pub fn ng_platform_get_text_content(text_handle: *mut c_void) -> *mut c_char;
    /// Frees content returned by `ng_platform_get_text_content`.
    pub fn ng_platform_free_text_content(content: *mut c_char);

    /// Creates a canvas.
    pub fn ng_platform_create_canvas(width: c_int, height: c_int) -> *mut c_void;
    /// Marks a canvas for redraw.
    pub fn ng_platform_canvas_invalidate(canvas: *mut c_void);
    /// Marks a rectangle of a canvas for redraw.
    pub fn ng_platform_canvas_invalidate_rect(
        canvas: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    );
    /// Copies a pixel buffer into a canvas.
    pub fn ng_platform_canvas_update_buffer(
        canvas: *mut c_void,
        buffer: *const u8,
        size: u32,
        width: u32,
        height: u32,
    );
    /// Returns NULL if the platform doesn't support external-target presentation; callers should
    /// fall back to `ng_platform_canvas_update_buffer`.
    pub fn ng_platform_canvas_acquire_buffer(
        canvas: *mut c_void,
        width: u32,
        height: u32,
        stride_px: *mut u32,
        buffer_index: *mut u32,
    ) -> *mut u8;
    /// Presents the canvas's acquired buffer.
    pub fn ng_platform_canvas_present(canvas: *mut c_void);
    /// Sets whether the GPU owns the canvas surface.
    pub fn ng_platform_canvas_set_gpu_owned(canvas: *mut c_void, gpu_owned: c_int);
    /// Gets a canvas's logical size.
    pub fn ng_platform_canvas_get_size(canvas: *mut c_void, width: *mut u32, height: *mut u32);
    /// The canvas in device pixels; 0 x 0 when that is its logical size times the scale.
    pub fn ng_platform_canvas_get_pixel_size(
        canvas: *mut c_void,
        width: *mut u32,
        height: *mut u32,
    );
    /// Gets the window that hosts a canvas.
    pub fn ng_platform_canvas_get_window(canvas: *mut c_void) -> *mut c_void;
    /// Gets the canvas's native handle.
    pub fn ng_platform_canvas_get_native_handle(canvas: *mut c_void) -> *mut c_void;
    /// Gets a window's scale factor.
    pub fn ng_platform_get_scale_factor(window: *mut c_void) -> f32;
    /// Sets the callback for scale factor changes.
    pub fn ng_platform_window_set_scale_factor_callback(
        window: *mut c_void,
        callback: extern "C" fn(*mut c_void, f32),
    );

    /// Sets the callback for window lifecycle events.
    pub fn ng_platform_window_set_lifecycle_callback(window: *mut c_void);

    /// Renders pending frames.
    pub fn ng_process_frames();

    /// Creates an image view.
    pub fn ng_platform_create_image_view() -> *mut c_void;
    /// Loads an image view from a file path.
    pub fn ng_platform_image_view_load_from_path(
        image_view: *mut c_void,
        path: *const c_char,
    ) -> c_int;
    /// Loads an image view from encoded bytes.
    pub fn ng_platform_image_view_load_from_data(
        image_view: *mut c_void,
        data: *const u8,
        size: u32,
    ) -> c_int;
    /// Sets how an image view scales its image.
    pub fn ng_platform_image_view_set_scaling(image_view: *mut c_void, scaling_mode: c_int);
    /// Reads the shown image's size in pixels, after any EXIF rotation.
    pub fn ng_platform_image_view_get_size(
        image_view: *mut c_void,
        width: *mut u32,
        height: *mut u32,
    ) -> c_int;
    /// Marks an image view for redraw.
    pub fn ng_platform_image_view_invalidate(image_view: *mut c_void);

    /// Creates a slider with a range.
    pub fn ng_platform_create_slider(min: f64, max: f64) -> *mut c_void;
    /// Sets a slider's value.
    pub fn ng_platform_slider_set_value(slider: *mut c_void, value: f64) -> c_int;
    /// Gets a slider's value.
    pub fn ng_platform_slider_get_value(slider: *mut c_void) -> f64;
    /// Enables or disables a slider.
    pub fn ng_platform_slider_set_enabled(slider: *mut c_void, enabled: c_int) -> c_int;
    /// Marks a slider for redraw.
    pub fn ng_platform_slider_invalidate(slider: *mut c_void);

    /// Creates a checkbox.
    pub fn ng_platform_create_checkbox(label: *const c_char) -> *mut c_void;
    /// Sets a checkbox's checked state.
    pub fn ng_platform_checkbox_set_checked(checkbox: *mut c_void, checked: c_int) -> c_int;
    /// Gets a checkbox's checked state.
    pub fn ng_platform_checkbox_get_checked(checkbox: *mut c_void) -> c_int;
    /// Enables or disables a checkbox.
    pub fn ng_platform_checkbox_set_enabled(checkbox: *mut c_void, enabled: c_int) -> c_int;
    /// Marks a checkbox for redraw.
    pub fn ng_platform_checkbox_invalidate(checkbox: *mut c_void);

    /// Creates a progress bar.
    pub fn ng_platform_create_progress_bar() -> *mut c_void;
    /// Sets a progress bar's value.
    pub fn ng_platform_progress_bar_set_value(progress_bar: *mut c_void, value: f64) -> c_int;
    /// Sets whether a progress bar is indeterminate.
    pub fn ng_platform_progress_bar_set_indeterminate(
        progress_bar: *mut c_void,
        indeterminate: c_int,
    ) -> c_int;
    /// Enables or disables a progress bar.
    pub fn ng_platform_progress_bar_set_enabled(progress_bar: *mut c_void, enabled: c_int)
    -> c_int;
    /// Marks a progress bar for redraw.
    pub fn ng_platform_progress_bar_invalidate(progress_bar: *mut c_void);

    /// Creates a combo box.
    pub fn ng_platform_create_combo_box() -> *mut c_void;
    /// Adds an item to a combo box.
    pub fn ng_platform_combo_box_add_item(combo_box: *mut c_void, item: *const c_char) -> c_int;
    /// Selects a combo box item by index.
    pub fn ng_platform_combo_box_set_selected(combo_box: *mut c_void, index: c_int) -> c_int;
    /// Gets the selected combo box index.
    pub fn ng_platform_combo_box_get_selected(combo_box: *mut c_void) -> c_int;
    /// Removes all items from a combo box.
    pub fn ng_platform_combo_box_clear(combo_box: *mut c_void) -> c_int;
    /// Enables or disables a combo box.
    pub fn ng_platform_combo_box_set_enabled(combo_box: *mut c_void, enabled: c_int) -> c_int;
    /// Marks a combo box for redraw.
    pub fn ng_platform_combo_box_invalidate(combo_box: *mut c_void);

    /// Creates a tab bar.
    pub fn ng_platform_create_tab_bar(id: u32) -> *mut c_void;
    /// Adds a tab to a tab bar.
    pub fn ng_platform_tab_bar_add_tab(tab_bar: *mut c_void, title: *const c_char) -> c_int;
    /// Removes a tab from a tab bar.
    pub fn ng_platform_tab_bar_remove_tab(tab_bar: *mut c_void, index: c_int) -> c_int;
    /// Selects a tab by index.
    pub fn ng_platform_tab_bar_set_selected(tab_bar: *mut c_void, index: c_int) -> c_int;
    /// Gets the selected tab index.
    pub fn ng_platform_tab_bar_get_selected(tab_bar: *mut c_void) -> c_int;
    /// Marks a tab bar for redraw.
    pub fn ng_platform_tab_bar_invalidate(tab_bar: *mut c_void);

    /// Creates a sidebar list.
    pub fn ng_platform_create_sidebar_list(id: u32) -> *mut c_void;

    #[cfg(target_os = "macos")]
    pub fn ng_platform_create_swiftui_host(width: c_int, height: c_int) -> *mut c_void;
    /// Adds a section header to a sidebar list.
    pub fn ng_platform_sidebar_list_add_section(
        sidebar: *mut c_void,
        title: *const c_char,
    ) -> c_int;
    /// Adds an item to a sidebar list.
    pub fn ng_platform_sidebar_list_add_item(
        sidebar: *mut c_void,
        title: *const c_char,
        indent: c_int,
    ) -> c_int;
    /// Selects a sidebar list item by index.
    pub fn ng_platform_sidebar_list_set_selected(sidebar: *mut c_void, index: c_int) -> c_int;
    /// Gets the selected sidebar list index.
    pub fn ng_platform_sidebar_list_get_selected(sidebar: *mut c_void) -> c_int;
    /// Removes all items from a sidebar list.
    pub fn ng_platform_sidebar_list_clear(sidebar: *mut c_void) -> c_int;
    /// Marks a sidebar list for redraw.
    pub fn ng_platform_sidebar_list_invalidate(sidebar: *mut c_void);

    /// Creates a table whose user selections and activations report `id`.
    pub fn ng_platform_create_table(id: c_uint) -> *mut c_void;
    /// Adds a column; columns come before rows.
    pub fn ng_platform_table_add_column(table: *mut c_void, title: *const c_char) -> c_int;
    /// Inserts a row of `count` cells at `index`.
    pub fn ng_platform_table_insert_row(
        table: *mut c_void,
        index: c_int,
        cells: *const *const c_char,
        count: c_int,
    ) -> c_int;
    /// Replaces the cells of row `index`.
    pub fn ng_platform_table_set_row(
        table: *mut c_void,
        index: c_int,
        cells: *const *const c_char,
        count: c_int,
    ) -> c_int;
    /// Removes row `index`.
    pub fn ng_platform_table_remove_row(table: *mut c_void, index: c_int) -> c_int;
    /// Removes every row.
    pub fn ng_platform_table_clear(table: *mut c_void) -> c_int;
    /// Selects row `index`, or nothing for -1, without reporting it.
    pub fn ng_platform_table_set_selected(table: *mut c_void, index: c_int) -> c_int;
    /// The selected row, or -1.
    pub fn ng_platform_table_get_selected(table: *mut c_void) -> c_int;

    /// Creates a row list whose user selections and activations report `id`.
    pub fn ng_platform_create_row_list(id: c_uint) -> *mut c_void;
    /// Inserts element `row` as the row at `index`; the list owns it from then on.
    pub fn ng_platform_row_list_insert(list: *mut c_void, index: c_int, row: *mut c_void) -> c_int;
    /// Removes and destroys row `index`.
    pub fn ng_platform_row_list_remove(list: *mut c_void, index: c_int) -> c_int;
    /// Removes and destroys every row.
    pub fn ng_platform_row_list_clear(list: *mut c_void) -> c_int;
    /// Selects row `index`, or nothing for -1, without reporting it.
    pub fn ng_platform_row_list_set_selected(list: *mut c_void, index: c_int) -> c_int;
    /// The selected row, or -1.
    pub fn ng_platform_row_list_get_selected(list: *mut c_void) -> c_int;
}
