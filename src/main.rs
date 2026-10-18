mod app;
mod providers;
mod screens;
mod shapes;

#[cfg(test)]
mod tests;

use crate::{
    app::App,
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
    logging::logger::init_logger,
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
use std::{error::Error, sync::Arc, time::Duration};

#[cfg(feature = "self-update")]
fn maybe_check_update() {
    use scout4all_core::localization::current_labels;
    use self_update::backends::github::Update;
    use self_update::cargo_crate_version;
    let repo_url = env!("CARGO_PKG_REPOSITORY");
    let pkg_name = env!("CARGO_PKG_NAME");
    let (repo_owner, repo_name) = repo_url
        .trim_start_matches("https://github.com/")
        .split_once('/')
        .unwrap_or(("naighes", "scoutforall"));
    let status = Update::configure()
        .repo_owner(repo_owner)
        .repo_name(repo_name)
        .bin_name(pkg_name)
        .show_download_progress(true)
        .current_version(cargo_crate_version!())
        .build()
        .and_then(|u| u.update());
    match status {
        Ok(status) => {
            if status.is_updated() {
                println!(
                    "{} {}",
                    current_labels().updated_to_version,
                    status.version(),
                );
            }
        }
        Err(_) => println!("{}", current_labels().update_check_failed),
    }
}

#[cfg(not(feature = "self-update"))]
fn maybe_check_update() {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let base_dir = get_base_path().expect("cannot get app directory");

    // init logger
    let log_path = base_dir.join("scout4all.log");
    init_logger(log_path);

    let queue_path = base_dir.join(constants::UPLOAD_QUEUE_FILE_NAME);
    let settings_reader = FileSystemSettingsReader::new(&base_dir);
    let team_reader = FileSystemTeamReader::new(&base_dir);
    let team_writer = FileSystemTeamWriter::new(&base_dir);
    let settings_writer = FileSystemSettingsWriter::new(&base_dir);
    let set_reader = FileSystemSetReader::new(&base_dir);
    let match_reader = FileSystemMatchReader::new(&base_dir, Arc::new(set_reader));
    let match_writer = FileSystemMatchWriter::new(&base_dir);
    let set_writer = FileSystemSetWriter::new(&base_dir);
    let queue_reader = FileSystemQueueReader::new(&queue_path);
    let queue_writer = FileSystemQueueWriter::new(&queue_path);
    let teams = team_reader.read_all().await.unwrap_or_else(|_| vec![]);
    let settings = settings_reader
        .read()
        .await
        .ok()
        .unwrap_or_else(Settings::default);
    init_settings(settings.clone());
    maybe_check_update();
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
    init_global_queue_manager(worker.queue_manager())?;
    let worker_handle = worker.start();
    // also installs a panic hook restoring the terminal state
    let mut terminal = ratatui::init();
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
    if let Err(err) = res {
        println!("{:?}", err)
    }
    Ok(())
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
