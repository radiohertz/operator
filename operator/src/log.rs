use log::{Level, Metadata, Record, SetLoggerError};

/// Logging for operator.
struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!(
                "[PID {}] {} - {}",
                std::process::id(),
                record.level(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: Logger = Logger;

/// Init logging for operator.
pub fn init() -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER).map(|()| log::set_max_level(log::LevelFilter::Info))
}
