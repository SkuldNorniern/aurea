//! `log` records written to logcat under the `aurea` tag.

use std::ffi::CString;
use std::os::raw::{c_char, c_int};

use log::{Level, LevelFilter, Log, Metadata, Record, max_level, set_logger, set_max_level};

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

struct Logcat;

static LOGCAT: Logcat = Logcat;

impl Log for Logcat {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // android/log.h priorities.
        let priority = match record.level() {
            Level::Error => 6,
            Level::Warn => 5,
            Level::Info => 4,
            Level::Debug => 3,
            Level::Trace => 2,
        };
        let text = format!("{}", record.args()).replace('\0', " ");
        let Ok(text) = CString::new(text) else {
            return;
        };
        unsafe { __android_log_write(priority, c"aurea".as_ptr(), text.as_ptr()) };
    }

    fn flush(&self) {}
}

/// Sends `log` output to logcat, unless the app set a logger already.
pub(super) fn install() {
    if set_logger(&LOGCAT).is_ok() {
        set_max_level(LevelFilter::Info);
    }
}
