use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOGGER: once_cell::sync::Lazy<Mutex<Logger>> =
    once_cell::sync::Lazy::new(|| Mutex::new(Logger::new()));

struct Logger {
    path: Option<PathBuf>,
}

impl Logger {
    fn new() -> Self {
        Self { path: None }
    }

    fn initialize(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    fn write(&self, level: &str, message: &str) {
        if let Some(path) = &self.path {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
            let line = format!("[{}] {} - {}\n", timestamp, level, message);
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = file.write_all(line.as_bytes());
            }
        }
    }
}

pub fn init_logger(log_path: PathBuf) {
    if let Ok(mut logger) = LOGGER.lock() {
        logger.initialize(log_path);
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
