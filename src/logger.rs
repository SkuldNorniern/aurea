use std::env::var;

use log::{
    LevelFilter, Log, Metadata, Record, SetLoggerError, log_enabled, set_logger, set_max_level,
};

struct SimpleLogger;

impl Log for SimpleLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if log_enabled!(target: record.target(), record.level()) {
            eprintln!(
                "[{}] {} -- {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

/// Initialize with explicit level.
pub fn init(level: LevelFilter) -> Result<(), SetLoggerError> {
    set_logger(&LOGGER)?;
    set_max_level(level);
    Ok(())
}

/// Initialize from `RUST_LOG` env var; falls back to `Warn`.
pub fn init_from_env() -> Result<(), SetLoggerError> {
    init(parse_rust_log())
}

/// Initialize with default level (Info).
pub fn init_default() -> Result<(), SetLoggerError> {
    init(LevelFilter::Info)
}

fn parse_rust_log() -> LevelFilter {
    let Ok(val) = var("RUST_LOG") else {
        return LevelFilter::Warn;
    };
    match val.to_ascii_lowercase().as_str() {
        "trace" => LevelFilter::Trace,
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "warn" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        "off" => LevelFilter::Off,
        _ => LevelFilter::Warn,
    }
}
