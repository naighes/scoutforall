use std::{path::PathBuf, sync::Arc};

use crate::{
    localization::current_labels,
    providers::{
        match_reader::MatchReader, match_writer::MatchWriter, set_writer::SetWriter,
        settings_reader::SettingsReader, settings_writer::SettingsWriter, team_reader::TeamReader,
        team_writer::TeamWriter,
    },
    screens::{
        components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner},
        edit_team_screen::EditTeamScreen,
        file_system_screen::FileSystemScreen,
        import_team_screen::ImportTeamAction,
        screen::{AppAction, Renderable, ScreenAsync},
        settings_screen::SettingsScreen,
        team_details_screen::TeamDetailsScreen,
    },
    shapes::{enums::FriendlyName, settings::Settings, team::TeamEntry},
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

#[derive(Debug)]
pub struct TeamListScreen<
    TR: TeamReader + Send + Sync,
    TW: TeamWriter + Send + Sync,
    SW: SettingsWriter + Send + Sync,
    MR: MatchReader + Send + Sync,
    MW: MatchWriter + Send + Sync,
    SSW: SetWriter + Send + Sync,
    SR: SettingsReader + Send + Sync,
> {
    list_state: ListState,
    teams: Vec<TeamEntry>,
    settings: Settings,
    notify_message: NotifyBanner,
    footer: NavigationFooter,
    base_path: PathBuf,
    team_reader: Arc<TR>,
    team_writer: Arc<TW>,
    settings_writer: Arc<SW>,
    match_reader: Arc<MR>,
    match_writer: Arc<MW>,
    set_writer: Arc<SSW>,
    settings_reader: Arc<SR>,
}

#[async_trait]
impl<
        TR: TeamReader + Send + Sync + 'static,
        TW: TeamWriter + Send + Sync + 'static,
        SW: SettingsWriter + Send + Sync + 'static,
        MR: MatchReader + Send + Sync + 'static,
        MW: MatchWriter + Send + Sync + 'static,
        SSW: SetWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
    > ScreenAsync for TeamListScreen<TR, TW, SW, MR, MW, SSW, SR>
{
    async fn refresh_data(&mut self) {
        match self.team_reader.read_all().await {
            Ok(teams) => {
                self.teams = teams;
                if self.teams.is_empty() {
                    self.list_state.select(None);
                } else if let Some(selected) = self.list_state.selected() {
                    if selected >= self.teams.len() {
                        self.list_state.select(Some(self.teams.len() - 1));
                    }
                } else {
                    self.list_state.select(Some(0));
                }
            }
            Err(_) => {
                self.notify_message
                    .set_error(current_labels().could_not_load_teams.to_string());
                self.teams = vec![];
            }
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, &self.notify_message.has_value()) {
            (_, true) => {
                self.notify_message.reset();
                AppAction::None
            }
            (KeyCode::Down, _) => {
                self.next_team();
                AppAction::None
            }
            (KeyCode::Up, _) => {
                self.previous_team();
                AppAction::None
            }
            (KeyCode::Enter, _) => match self.list_state.selected().and_then(|x| self.teams.get(x))
            {
                None => AppAction::None,
                Some(team) => AppAction::SwitchScreen(Box::new(TeamDetailsScreen::new(
                    team,
                    self.base_path.clone(),
                    self.team_reader.clone(),
                    self.team_writer.clone(),
                    self.match_reader.clone(),
                    self.match_writer.clone(),
                    self.set_writer.clone(),
                    self.settings_reader.clone(),
                    self.settings_writer.clone(),
                ))),
            },
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Char('n'), _) => {
                AppAction::SwitchScreen(Box::new(EditTeamScreen::new(self.team_writer.clone())))
            }
            (KeyCode::Char('i'), _) => {
                let default_path = self.settings.get_default_path();
                match default_path {
                    Some(path) => AppAction::SwitchScreen(Box::new(FileSystemScreen::new(
                        path,
                        current_labels().import_team,
                        ImportTeamAction::new(self.team_reader.clone(), self.team_writer.clone()),
                        self.settings_reader.clone(),
                        self.settings_writer.clone(),
                    ))),
                    None => {
                        self.notify_message.set_error(
                            current_labels()
                                .could_not_recognize_home_directory
                                .to_string(),
                        );
                        AppAction::None
                    }
                }
            }
            (KeyCode::Char('s'), _) => AppAction::SwitchScreen(Box::new(SettingsScreen::new(
                self.settings.clone(),
                self.settings_writer.clone(),
            ))),
            _ => AppAction::None,
        }
    }
}

impl<
        TR: TeamReader + Send + Sync + 'static,
        TW: TeamWriter + Send + Sync + 'static,
        SW: SettingsWriter + Send + Sync + 'static,
        MR: MatchReader + Send + Sync + 'static,
        MW: MatchWriter + Send + Sync + 'static,
        SSW: SetWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
    > Renderable for TeamListScreen<TR, TW, SW, MR, MW, SSW, SR>
{
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        self.notify_message.render(f, footer_right);
        let items: Vec<ListItem> = self
            .teams
            .iter()
            .map(|t| {
                ListItem::new(format!(
                    "{} ({}/{}, {})",
                    t.name,
                    t.classification
                        .map(|c| c.friendly_name(current_labels()))
                        .unwrap_or_default(),
                    t.gender
                        .map(|g| g.friendly_name(current_labels()))
                        .unwrap_or_default(),
                    t.year,
                ))
            })
            .collect();
        if items.is_empty() {
            self.render_no_teams_yet(f, body);
        } else {
            self.render_list(f, body, items);
        }
        self.footer
            .render(f, footer_left, self.get_footer_entries());
    }
}

impl<
        TR: TeamReader + Send + Sync,
        TW: TeamWriter + Send + Sync,
        SW: SettingsWriter + Send + Sync,
        MR: MatchReader + Send + Sync,
        MW: MatchWriter + Send + Sync,
        SSW: SetWriter + Send + Sync,
        SR: SettingsReader + Send + Sync,
    > TeamListScreen<TR, TW, SW, MR, MW, SSW, SR>
{
    pub fn new(
        settings: Settings,
        teams: Vec<TeamEntry>,
        base_path: PathBuf,
        team_reader: Arc<TR>,
        team_writer: Arc<TW>,
        settings_writer: Arc<SW>,
        match_reader: Arc<MR>,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
        settings_reader: Arc<SR>,
    ) -> Self {
        TeamListScreen {
            teams,
            list_state: ListState::default(),
            notify_message: NotifyBanner::new(),
            settings: settings.clone(),
            footer: NavigationFooter::new(),
            base_path,
            team_reader,
            team_writer,
            settings_writer,
            match_reader,
            match_writer,
            set_writer,
            settings_reader,
        }
    }

    fn next_team(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = (selected + 1).min(self.teams.len() - 1);
            self.list_state.select(Some(new_selected));
        }
    }

    fn previous_team(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = if selected == 0 { 0 } else { selected - 1 };
            self.list_state.select(Some(new_selected));
        }
    }

    fn get_footer_entries(&self) -> Vec<(String, String)> {
        if self.teams.is_empty() {
            vec![
                ("N".to_string(), current_labels().new_team.to_string()),
                ("S".to_string(), current_labels().settings.to_string()),
                ("I".to_string(), current_labels().import_team.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ]
        } else {
            vec![
                ("↑↓".to_string(), current_labels().navigate.to_string()),
                (
                    current_labels().enter.to_string(),
                    current_labels().select.to_string(),
                ),
                ("S".to_string(), current_labels().settings.to_string()),
                ("N".to_string(), current_labels().new_team.to_string()),
                ("I".to_string(), current_labels().import_team.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ]
        }
    }

    fn render_no_teams_yet(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(3),
                Constraint::Percentage(40),
            ])
            .split(area);
        let paragraph = Paragraph::new(current_labels().no_teams_yet)
            .block(Block::default().borders(Borders::NONE))
            .alignment(Alignment::Center);
        f.render_widget(paragraph, chunks[1]);
    }

    fn render_list(&mut self, f: &mut Frame, area: Rect, items: Vec<ListItem>) {
        if self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(current_labels().teams),
            )
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }
}
