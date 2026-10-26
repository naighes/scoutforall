mod app;
mod localization;
mod pdf_viewer;
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
        settings_reader::SettingsReader,
        store::{settings_reader::StoreSettingsReader, settings_writer::StoreSettingsWriter},
    },
    shapes::settings::{Settings, init_settings},
};
use crokey::crossterm::event::{self, Event};
use ratatui::{Terminal, backend::Backend};
use scout4all_core::{
    analytics::{global::init_global_queue_manager, upload::AnalyticsUploadWorker},
    example::add_example_team_if_new,
    logging::logger::{init_logger, log_error},
    providers::{
        store::{
            match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
            queue_reader::StoreQueueReader, queue_writer::StoreQueueWriter,
            set_reader::StoreSetReader, set_writer::StoreSetWriter, team_reader::StoreTeamReader,
            team_writer::StoreTeamWriter,
        },
        team_reader::TeamReader,
    },
    storage::{Store, fs::FsStore},
};
use std::{
    env, fs::create_dir_all, panic, path::PathBuf, process::ExitCode, sync::Arc, time::Duration,
};

/// The variable naming the folder of the data, `~/.scout4all` if unset.
const DATA_DIR_VAR: &str = "SCOUT4ALL_DATA_DIR";

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

/// The folder of the data, made if missing: the one in `SCOUT4ALL_DATA_DIR`,
/// or `.scout4all` in the home folder.
fn data_dir() -> Result<PathBuf, String> {
    let labels = current_labels();
    let dir = env::var_os(DATA_DIR_VAR)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".scout4all")))
        .ok_or_else(|| labels.could_not_recognize_home_directory.to_string())?;
    create_dir_all(&dir).map_err(|e| {
        format!(
            "{}: {} ({e})",
            labels.could_not_create_app_directory,
            dir.display()
        )
    })?;
    Ok(dir)
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let data_dir = data_dir()?;

    // init logger
    let log_path = data_dir.join("scout4all.log");
    init_logger(log_path);

    let store: Arc<dyn Store> = Arc::new(FsStore::new(&data_dir));
    let settings_reader = StoreSettingsReader::new(Arc::clone(&store));
    let team_reader = StoreTeamReader::new(Arc::clone(&store), language());
    let team_writer = StoreTeamWriter::new(Arc::clone(&store));
    let settings_writer = StoreSettingsWriter::new(Arc::clone(&store));
    let set_reader = StoreSetReader::new(Arc::clone(&store), language());
    let match_reader = StoreMatchReader::new(Arc::clone(&store), Arc::new(set_reader), language());
    let match_writer = StoreMatchWriter::new(Arc::clone(&store));
    let set_writer = StoreSetWriter::new(Arc::clone(&store), language());
    let queue_reader = StoreQueueReader::new(Arc::clone(&store));
    let queue_writer = StoreQueueWriter::new(Arc::clone(&store));
    let settings = settings_reader
        .read()
        .await
        .ok()
        .unwrap_or_else(Settings::default);
    init_settings(settings.clone());
    // a new installation starts with a team, to try the app at once
    if let Err(e) = add_example_team_if_new(store.as_ref(), &team_writer, language().labels()).await
    {
        log_error(&format!("could not add the example team: {e}"));
    }
    let teams = team_reader.read_all().await.unwrap_or_else(|_| vec![]);
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
        Arc::clone(&store),
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
            store,
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
