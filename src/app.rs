use std::{path::PathBuf, sync::Arc};

use crokey::crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::Paragraph,
};

use crate::{
    providers::{
        match_reader::MatchReader, match_writer::MatchWriter, set_writer::SetWriter,
        settings_reader::SettingsReader, settings_writer::SettingsWriter, team_reader::TeamReader,
        team_writer::TeamWriter,
    },
    screens::{
        screen::{AppAction, ScreenAsync},
        team_list_screen::TeamListScreen,
    },
    shapes::{settings::Settings, team::TeamEntry},
};

pub struct App {
    screens: Vec<Box<dyn ScreenAsync>>,
}

impl App {
    pub fn new<
        TR: TeamReader + Send + Sync + 'static,
        TW: TeamWriter + Send + Sync + 'static,
        SW: SettingsWriter + Send + Sync + 'static,
        MR: MatchReader + Send + Sync + 'static,
        MW: MatchWriter + Send + Sync + 'static,
        SSW: SetWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
    >(
        settings: Settings,
        teams: Vec<TeamEntry>,
        base_path: PathBuf,
        team_reader: Arc<TR>,
        team_writer: Arc<TW>,
        settings_writer: Arc<SW>,
        set_writer: Arc<SSW>,
        match_reader: Arc<MR>,
        match_writer: Arc<MW>,
        settings_reader: Arc<SR>,
    ) -> Self {
        Self {
            screens: vec![Box::new(TeamListScreen::new(
                settings,
                teams,
                base_path,
                team_reader,
                team_writer,
                settings_writer,
                match_reader,
                match_writer,
                set_writer,
                settings_reader,
            ))],
        }
    }

    pub fn current_screen(&mut self) -> Option<&mut Box<dyn ScreenAsync>> {
        self.screens.last_mut()
    }

    pub fn push_screen(&mut self, screen: Box<dyn ScreenAsync>) {
        self.screens.push(screen);
    }

    pub async fn pop_screen(&mut self, refresh: bool, count: Option<u8>) {
        let count = count.unwrap_or(0) as usize;
        if count > 0 {
            let to_pop = count.min(self.screens.len().saturating_sub(1));
            for _ in 0..to_pop {
                self.screens.pop();
            }
            if let Some(prev) = self.screens.last_mut()
                && refresh
            {
                prev.refresh_data().await;
            }
        }
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
    pub fn draw(&mut self, f: &mut Frame) {
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
        if let Some(screen) = self.current_screen() {
            screen.render(f, container[1], footer[0], footer[1]);
        }
    }

    /// Dispatches a key event to the current screen; returns the exit result
    /// when the app has to quit.
    pub async fn handle_key(&mut self, key: KeyEvent) -> Option<std::io::Result<()>> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let screen = self.current_screen()?;
        match screen.handle_key(key).await {
            AppAction::None => {}
            AppAction::SwitchScreen(new_screen) => self.push_screen(new_screen),
            AppAction::Back(refresh, count) => {
                self.pop_screen(refresh, count).await;
                if refresh && let Some(screen) = self.current_screen() {
                    screen.refresh_data().await;
                }
            }
            AppAction::Quit(result) => return Some(result),
        }
        None
    }
}
