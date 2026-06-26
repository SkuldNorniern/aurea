use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};

use crate::elements::{
    invoke_button_callback, invoke_sidebar_list_selected, invoke_tab_bar_detach,
    invoke_tab_bar_selected, invoke_text_callback, invoke_textview_callback,
};
use crate::menu::invoke_menu_callback;
use crate::registry::custom::invoke_custom_callback;
use crate::view::FrameScheduler;
use crate::window::{KeyCode, Modifiers, MouseButton, WindowEvent, push_window_event};
use aurea_ffi::ng_platform_get_scale_factor;

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
    invoke_menu_callback(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_button_callback(id: u32) {
    invoke_button_callback(id);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_tab_bar_selected(id: u32, index: i32) {
    invoke_tab_bar_selected(id, index);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_tab_bar_detach(id: u32, index: i32) {
    invoke_tab_bar_detach(id, index);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_sidebar_list_selected(id: u32, index: i32) {
    invoke_sidebar_list_selected(id, index);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_text_callback(id: u32, content: *const c_char) {
    if let Some(content) = c_string(content) {
        invoke_text_callback(id, content);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_textview_callback(id: u32, content: *const c_char) {
    if let Some(content) = c_string(content) {
        invoke_textview_callback(id, content);
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
    let event = WindowEvent::KeyInput {
        key: KeyCode::from_raw(keycode),
        pressed: pressed != 0,
        modifiers: Modifiers::from_bits(modifiers),
    };
    push_window_event(window, event);
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn ng_invoke_mouse_button(
    window: *mut c_void,
    button: c_int,
    pressed: c_int,
    modifiers: u32,
    x: f64,
    y: f64,
    click_count: c_int,
) {
    let scale = f64::from(unsafe { ng_platform_get_scale_factor(window) }).max(1.0);
    let button = u8::try_from(button.max(0)).unwrap_or(u8::MAX);
    let event = WindowEvent::MouseButton {
        button: MouseButton::from_raw(button),
        pressed: pressed != 0,
        modifiers: Modifiers::from_bits(modifiers),
        x: x / scale,
        y: y / scale,
        click_count: u8::try_from(click_count.clamp(1, c_int::from(u8::MAX)))
            .expect("clamped to u8 range"),
    };
    push_window_event(window, event);
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn ng_invoke_mouse_move(window: *mut c_void, x: f64, y: f64) {
    let scale = f64::from(unsafe { ng_platform_get_scale_factor(window) }).max(1.0);
    let event = WindowEvent::MouseMove {
        x: x / scale,
        y: y / scale,
    };
    push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_mouse_wheel(
    window: *mut c_void,
    delta_x: f64,
    delta_y: f64,
    modifiers: u32,
) {
    let event = WindowEvent::MouseWheel {
        delta_x,
        delta_y,
        modifiers: Modifiers::from_bits(modifiers),
    };
    push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_text_input(window: *mut c_void, text: *const c_char) {
    if let Some(text) = c_string(text) {
        let event = WindowEvent::TextInput { text };
        push_window_event(window, event);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_focus_changed(window: *mut c_void, focused: c_int) {
    let event = if focused != 0 {
        WindowEvent::Focused
    } else {
        WindowEvent::Unfocused
    };
    push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_cursor_entered(window: *mut c_void, entered: c_int) {
    let event = if entered != 0 {
        WindowEvent::MouseEntered
    } else {
        WindowEvent::MouseExited
    };
    push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_raw_mouse_motion(window: *mut c_void, delta_x: f64, delta_y: f64) {
    let event = WindowEvent::RawMouseMotion { delta_x, delta_y };
    push_window_event(window, event);
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_scale_factor_changed(window: *mut c_void, scale_factor: f32) {
    let event = WindowEvent::ScaleFactorChanged { scale_factor };
    push_window_event(window, event);
    FrameScheduler::schedule();
}

/// Invoke a custom callback by ID. Used by SwiftUI and other platform code.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_custom_callback(id: u32) {
    invoke_custom_callback(id);
}
