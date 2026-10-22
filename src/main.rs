mod app;
mod localization;
mod providers;
mod screens;
mod shapes;
#[cfg(feature = "self-update")]
mod updater;

// the tests may panic on what they don't expect (see clippy.toml)
#[cfg(test)]
#[allow(clippy::unreachable)]
mod tests;

use crate::{
    app::App,
    localization::{current_labels, language},
    providers::{
        fs::{
            settings_reader::FileSystemSettingsReader, settings_writer::FileSystemSettingsWriter,
        },
        settings_reader::SettingsReader,
    },
    shapes::settings::{Settings, init_settings},
};
use crokey::crossterm::event::{self, Event};
use ratatui::{Terminal, backend::Backend};
use scout4all_core::{
    analytics::{global::init_global_queue_manager, upload::AnalyticsUploadWorker},
    constants,
    logging::logger::{init_logger, log_error},
    providers::{
        fs::{
            match_reader::FileSystemMatchReader, match_writer::FileSystemMatchWriter,
            path::get_base_path, queue_reader::FileSystemQueueReader,
            queue_writer::FileSystemQueueWriter, set_reader::FileSystemSetReader,
            set_writer::FileSystemSetWriter, team_reader::FileSystemTeamReader,
            team_writer::FileSystemTeamWriter,
        },
        team_reader::TeamReader,
    },
};
use std::{panic, process::ExitCode, sync::Arc, time::Duration};

#[cfg(feature = "self-update")]
async fn maybe_check_update() {
    // the updater blocks on network and on the confirmation prompt
    if let Err(err) = tokio::task::spawn_blocking(updater::check_update).await {
        scout4all_core::logging::logger::log_error(&format!("self update panicked: {err}"));
    }
}

#[cfg(not(feature = "self-update"))]
async fn maybe_check_update() {}

/// The app never panics on the user: what can go wrong at startup ends it
/// with a message, and a panic left anyway (a bug) is logged.
#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{}: {message}", env!("CARGO_PKG_NAME"));
            ExitCode::FAILURE
        }
    }
}

/// Logs a panic with where it happened, so that the bug can be reported:
/// the terminal is restored by the hook of ratatui, installed after.
fn log_panics() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        log_error(&format!("panic: {info}"));
        previous(info);
    }));
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let base_dir = get_base_path(current_labels().domain).map_err(|e| e.to_string())?;

    // init logger
    let log_path = base_dir.join("scout4all.log");
    init_logger(log_path);

    let queue_path = base_dir.join(constants::UPLOAD_QUEUE_FILE_NAME);
    let settings_reader = FileSystemSettingsReader::new(&base_dir);
    let team_reader = FileSystemTeamReader::new(&base_dir, language());
    let team_writer = FileSystemTeamWriter::new(&base_dir, language());
    let settings_writer = FileSystemSettingsWriter::new(&base_dir);
    let set_reader = FileSystemSetReader::new(&base_dir, language());
    let match_reader = FileSystemMatchReader::new(&base_dir, Arc::new(set_reader), language());
    let match_writer = FileSystemMatchWriter::new(&base_dir, language());
    let set_writer = FileSystemSetWriter::new(&base_dir, language());
    let queue_reader = FileSystemQueueReader::new(&queue_path);
    let queue_writer = FileSystemQueueWriter::new(&queue_path);
    let teams = team_reader.read_all().await.unwrap_or_else(|_| vec![]);
    let settings = settings_reader
        .read()
        .await
        .ok()
        .unwrap_or_else(Settings::default);
    init_settings(settings.clone());
    maybe_check_update().await;
    let team_reader_arc = Arc::new(team_reader);
    let team_writer_arc = Arc::new(team_writer);
    let settings_writer_arc = Arc::new(settings_writer);
    let set_writer_arc = Arc::new(set_writer);
    let match_reader_arc = Arc::new(match_reader);
    let match_writer_arc = Arc::new(match_writer);
    let settings_reader_arc = Arc::new(settings_reader);
    let queue_reader_arc = Arc::new(queue_reader);
    let queue_writer_arc = Arc::new(queue_writer);
    // the analytics upload worker always runs: it uploads only while
    // analytics is enabled, which can change from the settings
    let worker = AnalyticsUploadWorker::new(
        base_dir.clone(),
        Arc::clone(&team_reader_arc),
        Arc::clone(&match_reader_arc),
        Arc::clone(&queue_reader_arc),
        Arc::clone(&queue_writer_arc),
        Duration::from_secs(30), // poll every 30 seconds
    );
    // without the queue the analytics aren't uploaded: the app still works
    if let Err(e) = init_global_queue_manager(worker.queue_manager()) {
        log_error(&format!("analytics queue not initialized: {e}"));
    }
    let worker_handle = worker.start();
    log_panics();
    // also installs a panic hook restoring the terminal state
    let mut terminal = ratatui::try_init().map_err(|e| {
        ratatui::restore();
        format!("cannot use the terminal: {e}")
    })?;
    let res = run_app(
        &mut terminal,
        App::new(
            settings,
            teams,
            base_dir,
            team_reader_arc,
            team_writer_arc,
            settings_writer_arc,
            set_writer_arc,
            match_reader_arc,
            match_writer_arc,
            settings_reader_arc,
        ),
    )
    .await;
    // graceful shutdown
    worker_handle.abort();
    ratatui::restore();
    res.map_err(|e| e.to_string())
}

async fn run_app<B: Backend<Error = std::io::Error>>(
    terminal: &mut Terminal<B>,
    mut app: App,
) -> std::io::Result<()> {
    loop {
        terminal.draw(|f| app.draw(f))?;
        if event::poll(std::time::Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && let Some(result) = app.handle_key(key).await
        {
            return result;
        }
    }
}
