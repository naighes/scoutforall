use crate::{
    errors::AppError,
    ops::get_matches,
    pdf::open_match_pdf,
    screens::{
        add_match::AddMatchScreen,
        scouting_screen::ScoutingScreen,
        screen::{AppAction, Screen},
        start_set_screen::StartSetScreen,
    },
    shapes::{enums::TeamSideEnum, r#match::MatchEntry, set::SetEntry, team::TeamEntry},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Alignment,
    widgets::{Padding, Row, Table},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, ListState, Paragraph},
    Frame,
};

#[derive(Debug)]
pub struct MatchListScreen {
    list_state: ListState,
    team: TeamEntry,
    matches: Vec<MatchEntry>,
    error: Option<String>,
    refresh: bool,
}

impl Screen for MatchListScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if self.error.is_some() {
            self.error = None;
            return AppAction::None;
        }
        match key.code {
            KeyCode::Down => self.next_match(),
            KeyCode::Up => self.previous_match(),
            KeyCode::Enter => self.handle_enter_key(),
            KeyCode::Esc => AppAction::Back(true, Some(1)),
            KeyCode::Char('n') => {
                if self.team.players.len() >= 6 {
                    AppAction::SwitchScreen(Box::new(AddMatchScreen::new(self.team.clone())))
                } else {
                    AppAction::None
                }
            }
            _ => AppAction::None,
        }
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        if self.refresh {
            self.refresh = false;
            self.matches = match get_matches(&self.team) {
                Ok(matches) => matches,
                Err(_) => {
                    self.error = Some("could not load matches".to_string());
                    vec![]
                }
            }
        }
        self.render_error(f, footer_right);
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);
        let header_text = format!(
            "{}\nleague: {}\nyear: {}",
            self.team.name, self.team.league, self.team.year
        );
        let header =
            Paragraph::new(header_text).block(Block::default().borders(Borders::ALL).title("team"));
        f.render_widget(header, container[0]);
        let match_index = match self.list_state.selected() {
            None => {
                self.list_state.select(Some(0));
                0
            }
            Some(p) => p,
        };
        let rows: Result<Vec<Row>, AppError> = self
            .matches
            .iter()
            .enumerate()
            .map(|(i, m)| self.get_match_row(m, i, match_index))
            .collect();
        if let Ok(rows) = rows {
            let table = Table::new(
                rows,
                vec![
                    Constraint::Length(14),
                    Constraint::Length(30),
                    Constraint::Length(30),
                    Constraint::Length(17),
                    Constraint::Length(20),
                ],
            )
            .block(Block::default().borders(Borders::ALL).title("match list"))
            .widths(&[
                Constraint::Length(14),
                Constraint::Length(30),
                Constraint::Length(30),
                Constraint::Length(17),
                Constraint::Length(20),
            ]);
            if self.matches.len() > 0 {
                f.render_widget(table, container[1]);
            } else {
                self.render_no_matches_yet(f, container[1]);
            }
        } else {
            self.error = Some("could not render the match list".to_string());
        }
        self.render_footer(f, footer_left);
    }

    fn on_resume(&mut self, refresh: bool) {
        if refresh {
            self.refresh = true;
        }
    }
}

impl MatchListScreen {
    pub fn new(team: TeamEntry) -> Self {
        MatchListScreen {
            matches: vec![],
            team,
            list_state: ListState::default(),
            refresh: true,
            error: None,
        }
    }

    fn get_match_row(
        &self,
        m: &MatchEntry,
        row_index: usize,
        match_index: usize,
    ) -> Result<Row<'_>, AppError> {
        let status = m.get_status()?;
        let (name_left, name_right, score_left, score_right) = if m.home {
            (
                m.team.name.clone(),
                m.opponent.clone(),
                status.us_wins,
                status.them_wins,
            )
        } else {
            (
                m.opponent.clone(),
                m.team.name.clone(),
                status.them_wins,
                status.us_wins,
            )
        };
        let mut row = Row::new(vec![
            name_left,
            name_right,
            format!("{:?}-{:?}", score_left, score_right),
            m.date.format("%a %b %d, %Y").to_string(),
            (if status.match_finished {
                ""
            } else {
                "in progress"
            })
            .into(),
        ]);
        let mut style = match (status.match_finished, status.us_wins, status.them_wins) {
            (true, 3, _) => Style::default().fg(Color::LightGreen),
            (true, _, 3) => Style::default().fg(Color::Red),
            _ => Style::default().fg(Color::White),
        };
        if row_index == match_index {
            style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        row = row.style(style);
        Ok(row)
    }

    fn new_set(
        &mut self,
        m: &MatchEntry,
        next_set_number: u8,
        last_serving_team: Option<TeamSideEnum>,
    ) -> AppAction {
        AppAction::SwitchScreen(Box::new(StartSetScreen::new(
            m.clone(),
            next_set_number,
            match (next_set_number, last_serving_team) {
                (_, None) => None,
                (1 | 5, _) => None,
                (_, Some(side)) => {
                    if side == TeamSideEnum::Them {
                        Some(TeamSideEnum::Us)
                    } else {
                        Some(TeamSideEnum::Them)
                    }
                }
            },
            Some(1),
        )))
    }

    fn continue_set(&mut self, m: &MatchEntry, last_incomplete_set: SetEntry) -> AppAction {
        match last_incomplete_set.compute_snapshot() {
            Ok((snapshot, available_options)) => {
                AppAction::SwitchScreen(Box::new(ScoutingScreen::new(
                    m.clone(),
                    last_incomplete_set,
                    snapshot,
                    available_options,
                    Some(1),
                )))
            }
            Err(_) => {
                self.error = Some("could not compute the snapshot".to_string());
                AppAction::None
            }
        }
    }

    fn next_match(&mut self) -> AppAction {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = (selected + 1).min(self.matches.len() - 1);
            self.list_state.select(Some(new_selected));
        };
        AppAction::None
    }

    fn previous_match(&mut self) -> AppAction {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = if selected == 0 { 0 } else { selected - 1 };
            self.list_state.select(Some(new_selected));
        };
        AppAction::None
    }

    fn render_footer(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let new_match_text = if self.team.players.len() >= 6 {
            "N = new match | "
        } else {
            ""
        };
        let paragraph = match self.matches.len() {
            0 => Paragraph::new(format!("{}Esc = back | Q = quit", new_match_text)).block(block),
            _ => Paragraph::new(format!(
                "↑↓ = move | Enter = select | {}Esc = back | Q = quit",
                new_match_text
            ))
            .block(block),
        };
        f.render_widget(paragraph, area);
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

    fn render_no_matches_yet(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(3),
                Constraint::Percentage(40),
            ])
            .split(area);
        let paragraph = Paragraph::new("no matches yet")
            .block(Block::default().borders(Borders::NONE))
            .alignment(Alignment::Center);
        f.render_widget(paragraph, chunks[1]);
    }

    fn handle_enter_key(&mut self) -> AppAction {
        let selected_match = match self
            .list_state
            .selected()
            .and_then(|i| self.matches.get(i).cloned())
        {
            Some(m) => m,
            None => {
                self.error = Some("no match selected".to_string());
                return AppAction::None;
            }
        };
        let status = match selected_match.get_status() {
            Ok(s) => s,
            Err(_) => {
                self.error = Some("could not get match status".to_string());
                return AppAction::None;
            }
        };
        match (
            status.match_finished,
            status.last_incomplete_set.clone(),
            status.next_set_number,
        ) {
            (true, _, _) => {
                open_match_pdf(&selected_match).expect("OH MY!");
                AppAction::None
            }
            (false, None, Some(next_set_number)) => {
                // play a new set
                self.new_set(&selected_match, next_set_number, status.last_serving_team)
            }
            (false, Some(last_set), _) => {
                // continue incomplete set
                self.continue_set(&selected_match, last_set)
            }
            _ => AppAction::None,
        }
    }
}
