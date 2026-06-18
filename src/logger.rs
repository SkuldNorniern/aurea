use log::{LevelFilter, Metadata, Record};

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if log::log_enabled!(target: record.target(), record.level()) {
            eprintln!("[{}] {} -- {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

/// Initialize with explicit level.
pub fn init(level: LevelFilter) -> Result<(), log::SetLoggerError> {
    log::set_logger(&LOGGER)?;
    log::set_max_level(level);
    Ok(())
}

/// Initialize from `RUST_LOG` env var; falls back to `Warn`.
pub fn init_from_env() -> Result<(), log::SetLoggerError> {
    init(parse_rust_log())
}

/// Initialize with default level (Info).
pub fn init_default() -> Result<(), log::SetLoggerError> {
    init(LevelFilter::Info)
}

fn parse_rust_log() -> LevelFilter {
    let Ok(val) = std::env::var("RUST_LOG") else {
        return LevelFilter::Warn;
    };
    match val.to_ascii_lowercase().as_str() {
        "trace" => LevelFilter::Trace,
        "debug" => LevelFilter::Debug,
        "info"  => LevelFilter::Info,
        "warn"  => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        "off"   => LevelFilter::Off,
        _       => LevelFilter::Warn,
    }
}
