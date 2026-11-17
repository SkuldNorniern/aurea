use log::error;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use crate::app::opened;
use crate::input::{drop, gesture, ime, pen, raw, text};
use crate::registry::custom::invoke_custom_callback;
use crate::registry::elements::{
    invoke_button_callback, invoke_row_list_activated, invoke_row_list_selected,
    invoke_sidebar_selected, invoke_tab_detach, invoke_tab_selected, invoke_table_activated,
    invoke_table_selected,
};
use crate::registry::menu::invoke_menu_callback;
use crate::registry::window::{push_event_to_all_windows, scale_factor_changed};
use crate::render::notify_canvas_destroyed;
use crate::window::{
    KeyCode, Modifiers, MouseButton, MouseButtons, PointerKind, WindowEvent, push_window_event,
};
use aurea_runtime::FrameScheduler;

fn guard(entry: &str, call: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(call)).is_err() {
        error!("aurea: application callback panicked in {entry}; the panic was contained");
    }
}

/// Tells Aurea a canvas's native view was destroyed.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_canvas_destroyed(canvas: *mut c_void) {
    guard("ng_invoke_canvas_destroyed", || {
        notify_canvas_destroyed(canvas)
    });
}

/// Runs the menu item callback `id`.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_menu_callback(id: u32) {
    guard("ng_invoke_menu_callback", || {
        invoke_menu_callback(id);
    });
}

/// Runs the button callback `id`.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_button_callback(id: u32) {
    guard("ng_invoke_button_callback", || {
        invoke_button_callback(id);
    });
}

/// Reports that tab `index` of tab bar `id` was selected.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_tab_bar_selected(id: u32, index: i32) {
    guard("ng_invoke_tab_bar_selected", || {
        invoke_tab_selected(id, index);
    });
}

/// Reports that tab `index` of tab bar `id` was detached.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_tab_bar_detach(id: u32, index: i32) {
    guard("ng_invoke_tab_bar_detach", || {
        invoke_tab_detach(id, index);
    });
}

/// Reports that row `index` of sidebar list `id` was selected.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_sidebar_list_selected(id: u32, index: i32) {
    guard("ng_invoke_sidebar_list_selected", || {
        invoke_sidebar_selected(id, index);
    });
}

/// Reports that the user selected row `index` of table `id`, or no row for -1.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_table_selected(id: u32, index: i32) {
    guard("ng_invoke_table_selected", || {
        invoke_table_selected(id, index);
    });
}

/// Reports that the user activated row `index` of table `id`.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_table_activated(id: u32, index: i32) {
    guard("ng_invoke_table_activated", || {
        invoke_table_activated(id, index);
    });
}

/// Reports that the user selected row `index` of row list `id`, or no row for -1.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_row_list_selected(id: u32, index: i32) {
    guard("ng_invoke_row_list_selected", || {
        invoke_row_list_selected(id, index);
    });
}

/// Reports that the user activated row `index` of row list `id`.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_row_list_activated(id: u32, index: i32) {
    guard("ng_invoke_row_list_activated", || {
        invoke_row_list_activated(id, index);
    });
}

/// Invoke a lifecycle callback from the platform layer.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_lifecycle_callback(window: *mut c_void, event_id: u32) {
    guard("ng_invoke_lifecycle_callback", || {
        use crate::lifecycle::{event_from_id, invoke_lifecycle_callback};
        if let Some(event) = event_from_id(event_id) {
            invoke_lifecycle_callback(window, event);
        }
    });
}

/// Queues a key press or release for `window`.
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

/// Queues a mouse button press or release for `window`.
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
        let button = u8::try_from(button.max(0)).unwrap_or(u8::MAX);
        let event = WindowEvent::MouseButton {
            button: MouseButton::from_raw(button),
            pressed: pressed != 0,
            modifiers: Modifiers::from_bits(modifiers),
            x,
            y,
            click_count: u8::try_from(click_count.clamp(1, c_int::from(u8::MAX)))
                .expect("clamped to u8 range"),
            time: event_time(0.0),
            pointer: PointerKind::from_bits(modifiers),
        };
        push_window_event(window, event);
    });
}

/// Queues a mouse move for `window`.
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn ng_invoke_mouse_move(
    window: *mut c_void,
    x: f64,
    y: f64,
    buttons: u32,
    modifiers: u32,
) {
    guard("ng_invoke_mouse_move", || {
        let event = WindowEvent::MouseMove {
            x,
            y,
            buttons: MouseButtons::from_bits(buttons),
            modifiers: Modifiers::from_bits(modifiers),
            time: event_time(0.0),
            pointer: PointerKind::from_bits(modifiers),
        };
        push_window_event(window, event);
    });
}

/// Queues a mouse wheel scroll for `window`.
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
            // queue fills these.
            x: 0.0,
            y: 0.0,
            modifiers: Modifiers::from_bits(modifiers),
        };
        push_window_event(window, event);
    });
}

/// Queues a pinch (`value` a factor) or a twist (`value` radians, clockwise) about (`x`, `y`).
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_gesture(window: *mut c_void, kind: u32, value: f64, x: f64, y: f64) {
    guard("ng_invoke_gesture", || {
        gesture::deliver(window, kind, value, x, y);
    });
}

/// Queues typed text for `window`, dropping control characters.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_text_input(window: *mut c_void, text: *const c_char) {
    guard("ng_invoke_text_input", || {
        text::deliver(window, text);
    });
}

/// Queues IME preedit `text` with the cursor at byte `cursor`, if valid.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_ime_preedit(window: *mut c_void, text: *const c_char, cursor: c_int) {
    guard("ng_invoke_ime_preedit", || {
        ime::deliver(window, text, cursor);
    });
}

/// Queues a focus gain or loss for `window`.
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

/// Tells every window the system switched between light and dark.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_theme_changed(dark: c_int) {
    guard("ng_invoke_theme_changed", || {
        push_event_to_all_windows(WindowEvent::ThemeChanged { dark: dark != 0 });
    });
}

/// The system asked the app to open the file at `path`.
///
/// # Safety
///
/// `path` is null or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ng_invoke_file_opened(path: *const c_char) {
    guard("ng_invoke_file_opened", || {
        if path.is_null() {
            return;
        }
        // SAFETY: the caller hands a NUL-terminated path.
        let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
        opened(vec![PathBuf::from(path.as_ref())]);
    });
}

/// Queues a cursor enter or exit for `window`.
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

/// Queues a raw mouse delta for `window`.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_raw_mouse_motion(window: *mut c_void, delta_x: f64, delta_y: f64) {
    guard("ng_invoke_raw_mouse_motion", || {
        raw::deliver(window, delta_x, delta_y);
    });
}

static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

pub(crate) fn event_time(age: f64) -> Duration {
    let now = Instant::now().saturating_duration_since(*EPOCH);
    if !(0.0..1.0).contains(&age) {
        return now;
    }
    now.saturating_sub(Duration::from_secs_f64(age))
}

/// Queues a pen sample for `window`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn ng_invoke_pen(
    window: *mut c_void,
    x: f64,
    y: f64,
    pressure: f32,
    tilt_x: f32,
    tilt_y: f32,
    twist: f32,
    phase: u32,
    flags: u32,
    modifiers: u32,
    age: f64,
) {
    guard("ng_invoke_pen", || {
        pen::deliver(
            window, x, y, pressure, tilt_x, tilt_y, twist, phase, flags, modifiers, age,
        );
    });
}

/// Queues a file hover or drop for `window`.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_file_drop(
    window: *mut c_void,
    kind: u32,
    x: f64,
    y: f64,
    paths: *const c_char,
) {
    guard("ng_invoke_file_drop", || {
        drop::deliver(window, kind, x, y, paths);
    });
}

/// Queues a scale factor change for `window`.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_scale_factor_changed(window: *mut c_void, scale_factor: f32) {
    guard("ng_invoke_scale_factor_changed", || {
        if !scale_factor_changed(window, scale_factor) {
            return;
        }
        let event = WindowEvent::ScaleFactorChanged { scale_factor };
        push_window_event(window, event);
        FrameScheduler::schedule();
    });
}

/// A canvas was given a new size by the platform.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_canvas_resized(canvas: *mut c_void) {
    guard("ng_invoke_canvas_resized", || {
        FrameScheduler::schedule_canvas(canvas);
    });
}

/// Invoke a custom callback by ID.
#[unsafe(no_mangle)]
pub extern "C" fn ng_invoke_custom_callback(id: u32) {
    guard("ng_invoke_custom_callback", || {
        invoke_custom_callback(id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "widgets")]
    use crate::registry::elements::{next_button_id, register_button_callback};
    use std::thread::sleep;

    #[test]
    #[cfg(feature = "widgets")]
    fn a_panicking_callback_is_contained() {
        let id = next_button_id();
        register_button_callback(id, || panic!("application callback blew up"));

        ng_invoke_button_callback(id);
    }

    #[test]
    #[cfg(feature = "widgets")]
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

    #[test]
    fn event_times_count_back_by_their_age_and_reject_strange_ones() {
        let _ = *EPOCH;
        sleep(Duration::from_millis(30));
        let now = event_time(0.0);
        let earlier = event_time(0.02);
        assert!(
            earlier < now && now - earlier >= Duration::from_millis(19),
            "{earlier:?} {now:?}"
        );
        assert!(event_time(-1.0) >= now, "a negative age is now");
        assert!(
            event_time(5.0) >= now,
            "an age past a second is a clock mismatch"
        );
        assert!(event_time(f64::NAN) >= now);
    }
}
