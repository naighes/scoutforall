mod constants;
mod util;

mod app;
mod errors;
mod localization;
mod providers;
mod reporting;
mod screens;
mod shapes;

#[cfg(test)]
mod tests;

use crate::{
    app::App,
    localization::init_language,
    providers::{
        fs::{
            match_reader::FileSystemMatchReader, match_writer::FileSystemMatchWriter,
            path::get_base_path, set_reader::FileSystemSetReader, set_writer::FileSystemSetWriter,
            settings_reader::FileSystemSettingsReader, settings_writer::FileSystemSettingsWriter,
            team_reader::FileSystemTeamReader, team_writer::FileSystemTeamWriter,
        },
        settings_reader::SettingsReader,
        team_reader::TeamReader,
    },
    screens::screen::AppAction,
    shapes::settings::Settings,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
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
use std::{error::Error, sync::Arc};

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
    let dir = get_base_path().expect("cannot get home directory");
    let settings_reader = FileSystemSettingsReader::new(&dir);
    let team_reader = FileSystemTeamReader::new(&dir);
    let team_writer = FileSystemTeamWriter::new(&dir);
    let settings_writer = FileSystemSettingsWriter::new(&dir);
    let set_reader = FileSystemSetReader::new(&dir);
    let match_reader = FileSystemMatchReader::new(&dir, Arc::new(set_reader));
    let match_writer = FileSystemMatchWriter::new(&dir);
    let set_writer = FileSystemSetWriter::new(&dir);
    let teams = team_reader.read_all().await.unwrap_or_else(|_| vec![]);
    let settings = settings_reader
        .read()
        .await
        .ok()
        .unwrap_or_else(Settings::default);
    init_language(settings.language);
    maybe_check_update();
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
            dir,
            Arc::new(team_reader),
            Arc::new(team_writer),
            Arc::new(settings_writer),
            Arc::new(set_writer),
            Arc::new(match_reader),
            Arc::new(match_writer),
        ),
    )
    .await;

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
                    (KeyCode::Char('q'), _, _) => return Ok(()),
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
                    },
                    _ => {}
                }
            }
        }
    }
}
