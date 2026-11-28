use crate::clock::WallNow;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOGGER: std::sync::LazyLock<Mutex<Logger>> =
    std::sync::LazyLock::new(|| Mutex::new(Logger::new()));

struct Logger {
    /// where, and what time it is, once started
    started: Option<(PathBuf, WallNow)>,
}

impl Logger {
    fn new() -> Self {
        Self { started: None }
    }

    fn initialize(&mut self, path: PathBuf, now: WallNow) {
        self.started = Some((path, now));
    }

    fn write(&self, level: &str, message: &str) {
        if let Some((path, now)) = &self.started {
            let timestamp = now().format("%Y-%m-%d %H:%M:%S");
            let line = format!("[{}] {} - {}\n", timestamp, level, message);
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = file.write_all(line.as_bytes());
            }
        }
    }
}

/// The log written in `log_path`, each line at the time `now` tells.
pub fn init_logger(log_path: PathBuf, now: WallNow) {
    if let Ok(mut logger) = LOGGER.lock() {
        logger.initialize(log_path, now);
    }
}

pub fn log_info(message: &str) {
    if let Ok(logger) = LOGGER.lock() {
        logger.write("INFO", message);
    }
}

pub fn log_error(message: &str) {
    if let Ok(logger) = LOGGER.lock() {
        logger.write("ERROR", message);
    }
}
