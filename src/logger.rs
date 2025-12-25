use log::{Level, LevelFilter, Metadata, Record};

/// Simple logger implementation that writes to stderr
pub struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Trace
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

/// Initialize the logger with the specified log level
pub fn init(level: LevelFilter) -> Result<(), log::SetLoggerError> {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(level))
}

/// Initialize the logger with default level (Info)
pub fn init_default() -> Result<(), log::SetLoggerError> {
    init(LevelFilter::Info)
}

