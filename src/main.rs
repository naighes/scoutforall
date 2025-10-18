mod analytics;
mod app;
mod constants;
mod errors;
mod localization;
mod logging;
mod providers;
mod reporting;
mod screens;
mod shapes;
mod util;

#[cfg(test)]
mod tests;

use crate::{
    analytics::{global::init_global_queue_manager, upload::AnalyticsUploadWorker},
    app::App,
    logging::logger::init_logger,
    providers::{
        fs::{
            match_reader::FileSystemMatchReader, match_writer::FileSystemMatchWriter,
            path::get_base_path, queue_reader::FileSystemQueueReader,
            queue_writer::FileSystemQueueWriter, set_reader::FileSystemSetReader,
            set_writer::FileSystemSetWriter, settings_reader::FileSystemSettingsReader,
            settings_writer::FileSystemSettingsWriter, team_reader::FileSystemTeamReader,
            team_writer::FileSystemTeamWriter,
        },
        settings_reader::SettingsReader,
        team_reader::TeamReader,
    },
    screens::screen::AppAction,
    shapes::settings::{init_settings, Settings},
};
use crokey::crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::Paragraph,
    Terminal,
};
use std::{error::Error, sync::Arc, time::Duration};

#[cfg(feature = "self-update")]
fn maybe_check_update() {
    use crate::localization::current_labels;
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
            if status.updated() {
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
    // start analytics upload worker only if analytics is enabled
    let worker_handle = if settings.analytics_enabled {
        let worker = AnalyticsUploadWorker::new(
            base_dir.clone(),
            Arc::clone(&team_reader_arc),
            Arc::clone(&match_reader_arc),
            Arc::clone(&queue_reader_arc),
            Arc::clone(&queue_writer_arc),
            Duration::from_secs(30), // poll every 30 seconds
        );
        init_global_queue_manager(worker.queue_manager())?;
        Some(worker.start())
    } else {
        None
    };
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
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
    // graceful shutdown: abort worker if it was started
    if let Some(handle) = worker_handle {
        handle.abort();
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    if let Err(err) = res {
        println!("{:?}", err)
    }
    Ok(())
}

/// The main structure is the following one:
///
/// |----------------------------|
/// |          header            |
/// |----------------------------|
/// |                            |
/// |                            |
/// |           body             |
/// |                            |
/// |                            |
/// |----------------------------|
/// | footer_left | footer_right |
/// |----------------------------|
async fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> std::io::Result<()> {
    loop {
        terminal.draw(|f| {
            let size = f.area();
            let container = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(3),
                ])
                .split(size);
            let footer = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(container[2]);
            let header =
                Paragraph::new("🏐 scout4all").style(Style::default().add_modifier(Modifier::BOLD));
            f.render_widget(header, container[0]);
            if let Some(screen) = app.current_screen() {
                screen.render(f, container[1], footer[0], footer[1]);
            }
        })?;
        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.kind, app.current_screen()) {
                    (_, KeyEventKind::Release, _) => continue,
                    (_, _, Some(screen)) => match screen.handle_key(key).await {
                        AppAction::None => {}
                        AppAction::SwitchScreen(new_screen) => app.push_screen(new_screen),
                        AppAction::Back(refresh, count) => {
                            app.pop_screen(refresh, count).await;
                            if refresh {
                                if let Some(screen) = app.current_screen() {
                                    screen.refresh_data().await;
                                }
                            }
                        }
                        AppAction::Quit(result) => return result,
                    },
                    _ => {}
                }
            }
        }
    }
}
