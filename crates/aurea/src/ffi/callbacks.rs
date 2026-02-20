use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};

#[inline]
fn c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }

    let c_str = unsafe { CStr::from_ptr(ptr) };
    c_str.to_str().ok().map(str::to_owned)
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_menu_callback(id: u32) {
    crate::menu::invoke_menu_callback(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_button_callback(id: u32) {
    crate::elements::invoke_button_callback(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_tab_bar_selected(id: u32, index: i32) {
    crate::elements::invoke_tab_bar_selected(id, index);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_tab_bar_detach(id: u32, index: i32) {
    crate::elements::invoke_tab_bar_detach(id, index);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_sidebar_list_selected(id: u32, index: i32) {
    crate::elements::invoke_sidebar_list_selected(id, index);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_text_callback(id: u32, content: *const c_char) {
    if let Some(content) = c_string(content) {
        crate::elements::invoke_text_callback(id, content);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_textview_callback(id: u32, content: *const c_char) {
    if let Some(content) = c_string(content) {
        crate::elements::invoke_textview_callback(id, content);
    }
}

/// Invoke a lifecycle callback from the platform layer.
///
/// This function is called by platform-specific code when a lifecycle event occurs.
/// The event_id corresponds to the LifecycleEvent enum values.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_lifecycle_callback(window: *mut c_void, event_id: u32) {
    use crate::lifecycle::{event_from_id, invoke_lifecycle_callback};
    if let Some(event) = event_from_id(event_id) {
        invoke_lifecycle_callback(window, event);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_key_event(
    window: *mut c_void,
    keycode: u32,
    pressed: c_int,
    modifiers: u32,
) {
    let event = crate::window::WindowEvent::KeyInput {
        key: crate::window::KeyCode::from_raw(keycode),
        pressed: pressed != 0,
        modifiers: crate::window::Modifiers::from_bits(modifiers),
    };
    crate::window::push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_mouse_button(
    window: *mut c_void,
    button: c_int,
    pressed: c_int,
    modifiers: u32,
) {
    let button = if button < 0 { 0 } else { button as u8 };
    let event = crate::window::WindowEvent::MouseButton {
        button: crate::window::MouseButton::from_raw(button),
        pressed: pressed != 0,
        modifiers: crate::window::Modifiers::from_bits(modifiers),
    };
    crate::window::push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_mouse_move(window: *mut c_void, x: f64, y: f64) {
    let event = crate::window::WindowEvent::MouseMove { x, y };
    crate::window::push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_mouse_wheel(
    window: *mut c_void,
    delta_x: f64,
    delta_y: f64,
    modifiers: u32,
) {
    let event = crate::window::WindowEvent::MouseWheel {
        delta_x,
        delta_y,
        modifiers: crate::window::Modifiers::from_bits(modifiers),
    };
    crate::window::push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_text_input(window: *mut c_void, text: *const c_char) {
    if let Some(text) = c_string(text) {
        let event = crate::window::WindowEvent::TextInput { text };
        crate::window::push_window_event(window, event);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_focus_changed(window: *mut c_void, focused: c_int) {
    let event = if focused != 0 {
        crate::window::WindowEvent::Focused
    } else {
        crate::window::WindowEvent::Unfocused
    };
    crate::window::push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_cursor_entered(window: *mut c_void, entered: c_int) {
    let event = if entered != 0 {
        crate::window::WindowEvent::MouseEntered
    } else {
        crate::window::WindowEvent::MouseExited
    };
    crate::window::push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_raw_mouse_motion(window: *mut c_void, delta_x: f64, delta_y: f64) {
    let event = crate::window::WindowEvent::RawMouseMotion { delta_x, delta_y };
    crate::window::push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_scale_factor_changed(window: *mut c_void, scale_factor: f32) {
    let event = crate::window::WindowEvent::ScaleFactorChanged { scale_factor };
    crate::window::push_window_event(window, event);
    crate::view::FrameScheduler::schedule();
}
