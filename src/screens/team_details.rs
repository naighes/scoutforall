use crate::{
    ops::load_teams,
    screens::{
        add_player::AddPlayerScreen,
        match_list::MatchListScreen,
        screen::{AppAction, Screen},
    },
    shapes::team::TeamEntry,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Alignment,
    style::Color,
    widgets::{Padding, Table},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, ListState, Paragraph, Row},
    Frame,
};
use uuid::Uuid;

#[derive(Debug)]
pub struct TeamDetailsScreen {
    list_state: ListState,
    teams: Vec<TeamEntry>,
    team_id: Uuid,
    refresh: bool,
    error: Option<String>,
}

impl Screen for TeamDetailsScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match key.code {
            KeyCode::Down => {
                self.next_player();
                AppAction::None
            }
            KeyCode::Up => {
                self.previous_player();
                AppAction::None
            }
            KeyCode::Char('n') => match self.teams.iter().find(|t| t.id == self.team_id) {
                Some(t) => AppAction::SwitchScreen(Box::new(AddPlayerScreen::new(t.clone()))),
                None => AppAction::None,
            },
            KeyCode::Char('m') => match self.teams.iter().find(|t| t.id == self.team_id) {
                Some(t) => AppAction::SwitchScreen(Box::new(MatchListScreen::new(t.clone()))),
                None => AppAction::None,
            },
            KeyCode::Esc => AppAction::Back(true, Some(1)),
            _ => AppAction::None,
        }
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        if self.refresh {
            self.refresh = false;
            self.teams = match load_teams() {
                Ok(teams) => teams,
                Err(_) => {
                    self.error = Some("could not load teams".to_string());
                    vec![]
                }
            }
        }
        self.render_error(f, footer_right);
        let team = self.teams.iter().find(|t| t.id == self.team_id);
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);
        self.render_header(f, container[0], team);
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
            Row::new(vec!["nr.", "full name", "role"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(Block::default().borders(Borders::ALL).title("players"))
        .widths(&[
            Constraint::Length(7),
            Constraint::Length(30),
            Constraint::Length(20),
        ]);
        if let Some(_) = team.and_then(|t| if t.players.len() > 0 { Some(t) } else { None }) {
            f.render_widget(table, container[1]);
        } else {
            self.render_no_players_yet(f, container[1]);
        }
        self.render_footer(f, footer_left);
    }

    fn on_resume(&mut self, refresh: bool) {
        if refresh {
            self.refresh = true;
        }
    }
}

impl TeamDetailsScreen {
    pub fn new(teams: Vec<TeamEntry>, team_id: Uuid) -> Self {
        TeamDetailsScreen {
            teams,
            team_id,
            list_state: ListState::default(),
            refresh: false,
            error: None,
        }
    }

    fn next_player(&mut self) {
        match (
            self.list_state.selected(),
            self.teams.iter().find(|t| t.id == self.team_id),
        ) {
            (Some(selected), Some(team)) => {
                let new_selected = (selected + 1).min(team.players.len() - 1);
                self.list_state.select(Some(new_selected));
            }
            _ => {}
        }
    }

    fn previous_player(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = if selected == 0 { 0 } else { selected - 1 };
            self.list_state.select(Some(new_selected));
        }
    }

    fn render_footer(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph = match self.teams.iter().find(|t| t.id == self.team_id).map(|t| t.players.len()) {
            Some(0) | None => Paragraph::new("N = new player | M = match list | Esc = back | Q = quit").block(block),
            _ => {
                Paragraph::new("↑↓ = move | Enter = select | N = new player | M = match list | Esc = back | Q = quit").block(block)
            }
        };
        f.render_widget(paragraph, area);
    }

    fn render_header(&self, f: &mut Frame, area: Rect, team: Option<&TeamEntry>) {
        let header_text = if let Some(team) = team {
            format!(
                "{}\nleague: {}\nyear: {}",
                team.name, team.league, team.year
            )
        } else {
            "team not found".into()
        };
        let header =
            Paragraph::new(header_text).block(Block::default().borders(Borders::ALL).title("team"));
        f.render_widget(header, area);
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
        let paragraph = Paragraph::new("no players yet")
            .block(Block::default().borders(Borders::NONE))
            .alignment(Alignment::Center);
        f.render_widget(paragraph, chunks[1]);
    }

    fn render_error(&self, f: &mut Frame, area: Rect) {
        if let Some(err) = &self.error {
            let error_widget = Paragraph::new(err.clone())
                .style(
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                )
                .block(Block::default().borders(Borders::ALL).title("error"));
            f.render_widget(error_widget, area);
        }
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
