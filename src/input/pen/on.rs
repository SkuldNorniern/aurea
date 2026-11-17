//! Pen events: native readings and their barrel taps for the desktop, the mobile hosts' samples,
//! and the batching of consecutive samples.

use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use aurea_runtime::EventQueue;

use super::{MODE, PenMode, SOURCE};
use crate::Point;
use crate::ffi::callbacks::event_time;
use crate::window::{
    Modifiers, MouseButtons, PenPhase, PenSample, PenTapAction, PenTool, PointerKind, WindowEvent,
    push_window_event,
};

const PEN_ERASER: u32 = 1;
const PEN_BARREL: u32 = 2;
const PEN_WINTAB: u32 = 4;

static LAST_BARREL: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
unsafe extern "C" {
    fn ng_windows_pen_set_mode(mode: i32);
}

/// Sets the source the native layer reads pens from.
pub(crate) fn set_mode(mode: PenMode) {
    let code = match mode {
        PenMode::Auto => 0,
        PenMode::System => 1,
        PenMode::Wintab => 2,
    };
    MODE.store(code, Ordering::Relaxed);
    SOURCE.store(0, Ordering::Relaxed);
    #[cfg(windows)]
    // SAFETY: takes any value, and the caller is on the UI thread.
    unsafe {
        ng_windows_pen_set_mode(i32::from(code));
    }
}

fn note_source(wintab: bool) {
    SOURCE.store(if wintab { 2 } else { 1 }, Ordering::Relaxed);
}

/// Queues a pen sample for `window`, with a barrel tap when the barrel went down in the air.
#[allow(clippy::too_many_arguments)]
pub(crate) fn deliver(
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
    note_source(flags & PEN_WINTAB != 0);
    let sample = PenSample {
        time: event_time(age),
        x,
        y,
        pressure: pressure.clamp(0.0, 1.0),
        tilt_x: tilt_x.clamp(-90.0, 90.0),
        tilt_y: tilt_y.clamp(-90.0, 90.0),
        twist: twist.rem_euclid(360.0),
        phase: PenPhase::from_raw(phase),
        tool: pen_tool(flags),
        barrel: flags & PEN_BARREL != 0,
        modifiers: Modifiers::from_bits(modifiers),
    };
    let tapped = barrel_tapped(&LAST_BARREL, sample.barrel, sample.phase);
    push_window_event(window, WindowEvent::Pen(sample));
    if tapped {
        push_window_event(
            window,
            WindowEvent::PenTap {
                squeeze: false,
                action: PenTapAction::default(),
            },
        );
    }
}

/// Whether the barrel button just went down with the pen in the air: a tap on it, as a phone's
/// stylus reports one. Pressed against the surface it only changes what the stroke does.
fn barrel_tapped(last: &AtomicBool, barrel: bool, phase: PenPhase) -> bool {
    let was = last.swap(barrel, Ordering::Relaxed);
    barrel && !was && phase == PenPhase::Hover
}

fn pen_tool(flags: u32) -> PenTool {
    if flags & PEN_ERASER != 0 {
        PenTool::Eraser
    } else {
        PenTool::Tip
    }
}

/// A pen sample from a mobile host, as [`WindowEvent::Pen`].
#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(crate) fn sample(sample: PenSample) -> Option<WindowEvent> {
    Some(WindowEvent::Pen(sample))
}

/// A predicted sample from iOS, ahead of the pen.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub(crate) fn predicted(sample: PenSample) -> Option<WindowEvent> {
    Some(WindowEvent::PenPredicted(sample))
}

/// A pen's side button or squeeze, from a mobile host.
#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(crate) fn tap(squeeze: bool, action: PenTapAction) -> Option<WindowEvent> {
    Some(WindowEvent::PenTap { squeeze, action })
}

/// A pen above the screen on Android: the pointer moves without pressing.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn hover(at: Point, age: f64) -> Option<WindowEvent> {
    Some(WindowEvent::MouseMove {
        x: f64::from(at.x),
        y: f64::from(at.y),
        buttons: MouseButtons::default(),
        modifiers: Modifiers::default(),
        time: event_time(age),
        pointer: PointerKind::Pen,
    })
}

pub(crate) fn set_batching(queue: &EventQueue, enabled: bool) {
    queue.set_pen_batching(enabled);
}

pub(crate) fn set_batch_size(queue: &EventQueue, size: usize) {
    queue.set_pen_batch_size(size);
}

pub(crate) fn batch_size(queue: &EventQueue) -> usize {
    queue.pen_batch_size()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::pen::{PenMode, PenSource, pen_mode, pen_source, set_pen_mode};

    #[test]
    fn a_barrel_press_in_the_air_is_a_tap() {
        let last = AtomicBool::new(false);
        assert!(
            barrel_tapped(&last, true, PenPhase::Hover),
            "going down in the air"
        );
        assert!(
            !barrel_tapped(&last, true, PenPhase::Hover),
            "held is no new tap"
        );
        assert!(!barrel_tapped(&last, false, PenPhase::Hover));
        assert!(
            !barrel_tapped(&last, true, PenPhase::Move),
            "on the surface"
        );
    }

    #[test]
    fn pen_flags_and_phases_read_as_sent() {
        assert_eq!(pen_tool(0), PenTool::Tip);
        assert_eq!(pen_tool(PEN_ERASER | PEN_BARREL), PenTool::Eraser);
        assert_eq!(PenPhase::from_raw(5), PenPhase::Cancel);
        let phases = [0, 1, 2, 3, 9].map(PenPhase::from_raw);
        assert_eq!(
            phases,
            [
                PenPhase::Hover,
                PenPhase::Down,
                PenPhase::Move,
                PenPhase::Up,
                PenPhase::Hover
            ]
        );
        assert!(PenPhase::Down.touching() && !PenPhase::Up.touching());
    }

    #[test]
    fn the_source_starts_over_with_the_mode_and_follows_samples() {
        note_source(true);
        assert_eq!(pen_source(), PenSource::Wintab);
        note_source(false);
        assert_eq!(pen_source(), PenSource::System);
        set_pen_mode(PenMode::Wintab);
        assert_eq!(pen_mode(), PenMode::Wintab);
        assert_eq!(pen_source(), PenSource::None);
        set_pen_mode(PenMode::default());
        assert_eq!(pen_mode(), PenMode::Auto);
    }
}
