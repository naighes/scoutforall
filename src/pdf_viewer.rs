use scout4all_core::errors::{AppError, IOError};
use scout4all_core::localization::Labels;
use scout4all_core::reporting::pdf::render_match_pdf;
use scout4all_core::shapes::r#match::MatchEntry;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};
use uuid::Uuid;

/// Writes the report of the match to a temporary file and opens it with the
/// system viewer: the core only renders the bytes.
pub fn open_match_pdf(m: &MatchEntry, labels: &Labels) -> Result<(), AppError> {
    let pdf = render_match_pdf(m, labels)?;
    let mut path: PathBuf = env::temp_dir();
    let uid = Uuid::new_v4().to_string();
    path.push(format!("{}_{}.pdf", m.id, uid));
    fs::write(&path, pdf).map_err(|e| AppError::IO(IOError::from(e)))?;
    open_with_system_viewer(&path)
}

fn open_with_system_viewer(path: &Path) -> Result<(), AppError> {
    // the empty argument is the window title: without it `start` would treat
    // a quoted path (e.g. containing spaces) as the title
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(path);
        c
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };

    #[cfg(target_os = "linux")]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };

    #[allow(clippy::zombie_processes)]
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::IO(IOError::from(e)))
}
