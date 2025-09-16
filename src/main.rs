mod constants;
mod ops;
mod pdf;
mod util;

mod errors;
mod io;
mod localization;
mod screens;
mod shapes;

#[cfg(test)]
mod tests;

use crate::{
    constants::DEFAULT_LANGUAGE,
    localization::init_language,
    ops::load_settings,
    screens::{
        screen::{AppAction, Screen},
        team_list::TeamListScreen,
    },
    shapes::enums::LanguageEnum,
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
use std::{error::Error, str::FromStr};

struct App {
    screens: Vec<Box<dyn Screen>>,
}

impl App {
    fn new() -> Self {
        App {
            screens: vec![Box::new(TeamListScreen::new())],
        }
    }

    fn current_screen(&mut self) -> Option<&mut Box<dyn Screen>> {
        self.screens.last_mut()
    }

    fn push_screen(&mut self, screen: Box<dyn Screen>) {
        self.screens.push(screen);
    }

    fn pop_screen(&mut self, refresh: bool, count: Option<u8>) {
        let count = count.unwrap_or(0) as usize;
        if count > 0 {
            let to_pop = count.min(self.screens.len().saturating_sub(1));
            for _ in 0..to_pop {
                self.screens.pop();
            }
            if let Some(prev) = self.screens.last_mut() {
                prev.on_resume(refresh);
            }
        }
    }
}

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

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let language = load_settings()
        .ok()
        .map(|s| s.language)
        .and_then(|l| LanguageEnum::from_str(&l).ok())
        .unwrap_or(
            LanguageEnum::from_str(DEFAULT_LANGUAGE)
                .ok()
                .unwrap_or(LanguageEnum::En),
        );
    init_language(language);
    maybe_check_update();
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, App::new());

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
fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> std::io::Result<()> {
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

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                if key.code == KeyCode::Char('q') {
                    return Ok(());
                }
                if let Some(screen) = app.current_screen() {
                    match screen.handle_key(key) {
                        AppAction::None => {}
                        AppAction::SwitchScreen(new_screen) => app.push_screen(new_screen),
                        AppAction::Back(refresh, count) => app.pop_screen(refresh, count),
                    }
                }
            }
        }
    }
}
