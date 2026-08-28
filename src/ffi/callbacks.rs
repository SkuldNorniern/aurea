use log::error;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::registry::custom::invoke_custom_callback;
use crate::registry::elements::{
    invoke_button_callback, invoke_sidebar_selected, invoke_tab_detach, invoke_tab_selected,
    invoke_text_editor_callback, invoke_text_view_callback,
};
use crate::registry::menu::invoke_menu_callback;
use crate::window::{KeyCode, Modifiers, MouseButton, WindowEvent, push_window_event};
use aurea_ffi::ng_platform_get_scale_factor;
use aurea_runtime::FrameScheduler;

/// Runs an application callback reached from native code.
///
/// A panic must not cross the C boundary. `extern "C"` aborts the process on
/// unwind, and killing the whole application because one button handler
/// panicked is not a reasonable thing for a UI toolkit to do. Log it and keep
/// the UI running instead.
fn guard(entry: &str, call: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(call)).is_err() {
        error!("aurea: application callback panicked in {entry}; the panic was contained");
    }
}

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
    guard("ng_invoke_menu_callback", || {
        invoke_menu_callback(id);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_button_callback(id: u32) {
    guard("ng_invoke_button_callback", || {
        invoke_button_callback(id);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_tab_bar_selected(id: u32, index: i32) {
    guard("ng_invoke_tab_bar_selected", || {
        invoke_tab_selected(id, index);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_tab_bar_detach(id: u32, index: i32) {
    guard("ng_invoke_tab_bar_detach", || {
        invoke_tab_detach(id, index);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_sidebar_list_selected(id: u32, index: i32) {
    guard("ng_invoke_sidebar_list_selected", || {
        invoke_sidebar_selected(id, index);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_text_callback(id: u32, content: *const c_char) {
    guard("ng_invoke_text_callback", || {
        if let Some(content) = c_string(content) {
            invoke_text_editor_callback(id, content);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_textview_callback(id: u32, content: *const c_char) {
    guard("ng_invoke_textview_callback", || {
        if let Some(content) = c_string(content) {
            invoke_text_view_callback(id, content);
        }
    });
}

/// Invoke a lifecycle callback from the platform layer.
///
/// This function is called by platform-specific code when a lifecycle event occurs.
/// The event_id corresponds to the LifecycleEvent enum values.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_lifecycle_callback(window: *mut c_void, event_id: u32) {
    guard("ng_invoke_lifecycle_callback", || {
        use crate::lifecycle::{event_from_id, invoke_lifecycle_callback};
        if let Some(event) = event_from_id(event_id) {
            invoke_lifecycle_callback(window, event);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_key_event(
    window: *mut c_void,
    keycode: u32,
    pressed: c_int,
    modifiers: u32,
) {
    guard("ng_invoke_key_event", || {
        let event = WindowEvent::KeyInput {
            key: KeyCode::from_raw(keycode),
            pressed: pressed != 0,
            modifiers: Modifiers::from_bits(modifiers),
        };
        push_window_event(window, event);
    });
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
    guard("ng_invoke_mouse_button", || {
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
    });
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn ng_invoke_mouse_move(window: *mut c_void, x: f64, y: f64) {
    guard("ng_invoke_mouse_move", || {
        let scale = f64::from(unsafe { ng_platform_get_scale_factor(window) }).max(1.0);
        let event = WindowEvent::MouseMove {
            x: x / scale,
            y: y / scale,
        };
        push_window_event(window, event);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_mouse_wheel(
    window: *mut c_void,
    delta_x: f64,
    delta_y: f64,
    modifiers: u32,
) {
    guard("ng_invoke_mouse_wheel", || {
        let event = WindowEvent::MouseWheel {
            delta_x,
            delta_y,
            modifiers: Modifiers::from_bits(modifiers),
        };
        push_window_event(window, event);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_text_input(window: *mut c_void, text: *const c_char) {
    guard("ng_invoke_text_input", || {
        if let Some(text) = c_string(text) {
            let event = WindowEvent::TextInput { text };
            push_window_event(window, event);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_focus_changed(window: *mut c_void, focused: c_int) {
    guard("ng_invoke_focus_changed", || {
        let event = if focused != 0 {
            WindowEvent::Focused
        } else {
            WindowEvent::Unfocused
        };
        push_window_event(window, event);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_cursor_entered(window: *mut c_void, entered: c_int) {
    guard("ng_invoke_cursor_entered", || {
        let event = if entered != 0 {
            WindowEvent::MouseEntered
        } else {
            WindowEvent::MouseExited
        };
        push_window_event(window, event);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_raw_mouse_motion(window: *mut c_void, delta_x: f64, delta_y: f64) {
    guard("ng_invoke_raw_mouse_motion", || {
        let event = WindowEvent::RawMouseMotion { delta_x, delta_y };
        push_window_event(window, event);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_scale_factor_changed(window: *mut c_void, scale_factor: f32) {
    guard("ng_invoke_scale_factor_changed", || {
        let event = WindowEvent::ScaleFactorChanged { scale_factor };
        push_window_event(window, event);
        FrameScheduler::schedule();
    });
}

/// Invoke a custom callback by ID. Used by SwiftUI and other platform code.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_custom_callback(id: u32) {
    guard("ng_invoke_custom_callback", || {
        invoke_custom_callback(id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::elements::{next_button_id, register_button_callback};

    /// A panicking application callback must not take the process with it.
    #[test]
    fn a_panicking_callback_is_contained() {
        let id = next_button_id();
        register_button_callback(id, || panic!("application callback blew up"));

        // Would abort the process without the guard.
        ng_invoke_button_callback(id);
    }

    /// Containing a panic must not stop the next callback from running.
    #[test]
    fn callbacks_still_run_after_a_panic() {
        use std::cell::Cell;
        use std::rc::Rc;

        let bad = next_button_id();
        register_button_callback(bad, || panic!("boom"));
        ng_invoke_button_callback(bad);

        let ran = Rc::new(Cell::new(false));
        let ran_clone = Rc::clone(&ran);
        let good = next_button_id();
        register_button_callback(good, move || ran_clone.set(true));

        ng_invoke_button_callback(good);
        assert!(ran.get(), "the registry should still work after a panic");
    }
}
