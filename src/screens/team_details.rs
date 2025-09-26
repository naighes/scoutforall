use crate::{
    localization::current_labels,
    ops::load_teams,
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner,
            team_header::TeamHeader,
        },
        edit_player::EditPlayerScreen,
        edit_team::EditTeamScreen,
        export_team::ExportTeamAction,
        file_system_screen::FileSystemScreen,
        match_list::MatchListScreen,
        screen::{AppAction, Screen},
    },
    shapes::team::TeamEntry,
};
use crossterm::event::{KeyCode, KeyEvent};
use dirs::home_dir;
use ratatui::widgets::*;
use ratatui::{layout::Alignment, widgets::Table};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    Frame,
};
use uuid::Uuid;

#[derive(Debug)]
pub struct TeamDetailsScreen {
    list_state: ListState,
    teams: Vec<TeamEntry>,
    team_id: Uuid,
    refresh: bool,
    notify_message: NotifyBanner,
    header: TeamHeader,
    footer: NavigationFooter,
}

impl Screen for TeamDetailsScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, &self.notify_message.has_value()) {
            (_, true) => {
                self.notify_message.reset();
                AppAction::None
            }
            (KeyCode::Down, _) => {
                self.next_player();
                AppAction::None
            }
            (KeyCode::Up, _) => {
                self.previous_player();
                AppAction::None
            }
            (KeyCode::Char('n'), _) => match self.teams.iter().find(|t| t.id == self.team_id) {
                Some(t) => AppAction::SwitchScreen(Box::new(EditPlayerScreen::new(t.clone()))),
                None => AppAction::None,
            },
            (KeyCode::Char('m'), _) => match self.teams.iter().find(|t| t.id == self.team_id) {
                Some(t) => AppAction::SwitchScreen(Box::new(MatchListScreen::new(t.clone()))),
                None => AppAction::None,
            },
            (KeyCode::Char('e'), _) => match self.teams.iter().find(|t| t.id == self.team_id) {
                Some(t) => AppAction::SwitchScreen(Box::new(EditTeamScreen::edit(t))),
                None => AppAction::None,
            },
            (KeyCode::Char('s'), _) => match self.teams.iter().find(|t| t.id == self.team_id) {
                Some(t) => match home_dir() {
                    Some(path) => AppAction::SwitchScreen(Box::new(FileSystemScreen::new(
                        path,
                        current_labels().export,
                        ExportTeamAction::new(t.id),
                    ))),
                    None => {
                        self.notify_message.set_error(
                            current_labels()
                                .could_not_recognize_home_directory
                                .to_string(),
                        );
                        AppAction::None
                    }
                },
                None => AppAction::None,
            },
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _) => {
                self.teams
                    .iter()
                    .find(|t| t.id == self.team_id)
                    .and_then(|team| {
                        self.list_state.selected().map(|selected| {
                            let player = team.players.get(selected).cloned();
                            match player {
                                Some(p) => AppAction::SwitchScreen(Box::new(
                                    EditPlayerScreen::edit(team.clone(), p),
                                )),
                                None => AppAction::None,
                            }
                        })
                    })
                    .unwrap_or(AppAction::None)
            }
            _ => AppAction::None,
        }
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        if self.refresh {
            self.refresh = false;
            self.teams = match load_teams() {
                Ok(teams) => teams,
                Err(_) => {
                    self.notify_message
                        .set_error(current_labels().could_not_load_teams.to_string());
                    vec![]
                }
            }
        }
        self.notify_message.render(f, footer_right);
        let team = self.teams.iter().find(|t| t.id == self.team_id);
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);
        self.header.render(f, container[0], team);
        let selected_player = match self.list_state.selected() {
            None => {
                self.list_state.select(Some(0));
                0
            }
            Some(p) => p,
        };
        let table = Table::new(
            self.get_rows(team, selected_player),
            vec![
                Constraint::Length(7),
                Constraint::Length(30),
                Constraint::Length(20),
            ],
        )
        .header(
            Row::new(vec!["#", current_labels().name, current_labels().role])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(current_labels().players),
        )
        .widths([
            Constraint::Length(7),
            Constraint::Length(30),
            Constraint::Length(20),
        ]);
        if team
            .and_then(|t| if t.players.is_empty() { None } else { Some(t) })
            .is_some()
        {
            f.render_widget(table, container[1]);
        } else {
            self.render_no_players_yet(f, container[1]);
        }
        self.footer
            .render(f, footer_left, self.get_footer_entries());
    }

    fn on_resume(&mut self, refresh: bool) {
        if refresh {
            self.refresh = true;
        }
    }
}

impl TeamDetailsScreen {
    pub fn new(teams: Vec<TeamEntry>, team_id: Uuid) -> Self {
        let header = TeamHeader::default();
        TeamDetailsScreen {
            teams,
            team_id,
            list_state: ListState::default(),
            refresh: false,
            notify_message: NotifyBanner::new(),
            header,
            footer: NavigationFooter::new(),
        }
    }

    fn next_player(&mut self) {
        if let (Some(selected), Some(team)) = (
            self.list_state.selected(),
            self.teams.iter().find(|t| t.id == self.team_id),
        ) {
            let new_selected = (selected + 1).min(team.players.len() - 1);
            self.list_state.select(Some(new_selected));
        }
    }

    fn previous_player(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = if selected == 0 { 0 } else { selected - 1 };
            self.list_state.select(Some(new_selected));
        }
    }

    fn get_footer_entries(&self) -> Vec<(String, String)> {
        match self
            .teams
            .iter()
            .find(|t| t.id == self.team_id)
            .map(|t| t.players.len())
        {
            Some(0) | None => vec![
                ("E".to_string(), current_labels().edit_team.to_string()),
                ("N".to_string(), current_labels().new_player.to_string()),
                ("M".to_string(), current_labels().match_list.to_string()),
                ("Esc".to_string(), current_labels().back.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ],
            _ => vec![
                ("↑↓".to_string(), current_labels().navigate.to_string()),
                (
                    current_labels().enter.to_string(),
                    current_labels().edit_player.to_string(),
                ),
                ("E".to_string(), current_labels().edit_team.to_string()),
                ("N".to_string(), current_labels().new_player.to_string()),
                ("M".to_string(), current_labels().match_list.to_string()),
                ("S".to_string(), current_labels().export.to_string()),
                ("Esc".to_string(), current_labels().back.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ],
        }
    }

    fn render_no_players_yet(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(3),
                Constraint::Percentage(40),
            ])
            .split(area);
        let paragraph = Paragraph::new(current_labels().no_players_yet)
            .block(Block::default().borders(Borders::NONE))
            .alignment(Alignment::Center);
        f.render_widget(paragraph, chunks[1]);
    }

    fn get_rows(&self, team: Option<&TeamEntry>, selected_player: usize) -> Vec<Row<'_>> {
        match team {
            Some(team) => team
                .players
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let mut row = Row::new(vec![
                        p.number.to_string(),
                        p.name.clone(),
                        p.role.to_string().clone(),
                    ]);
                    if i == selected_player {
                        row = row.style(
                            Style::default()
                                .add_modifier(Modifier::REVERSED)
                                .add_modifier(Modifier::BOLD),
                        );
                    }
                    row
                })
                .collect(),
            _ => vec![],
        }
    }
}
