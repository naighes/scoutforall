use crate::{
    ops::create_set,
    screens::{
        scouting_screen::ScoutingScreen,
        screen::{AppAction, Screen},
    },
    shapes::{enums::TeamSideEnum, player::PlayerEntry, r#match::MatchEntry},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Padding, Paragraph, Row, Table, TableState},
    Frame,
};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug)]
pub struct StartSetScreen {
    current_match: MatchEntry,
    set_number: u8,
    lineup: Vec<PlayerEntry>,
    initial_setter: Option<PlayerEntry>,
    error: Option<String>,
    state: StartSetScreenState,
    serving_team: Option<TeamSideEnum>,
    list_state: TableState,
    back_stack_count: Option<u8>,
}

#[derive(Debug)]
pub enum StartSetScreenState {
    SelectServingTeam,
    SelectLineupPlayers(usize, Option<Uuid>),
}

impl Screen for StartSetScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        self.error = None;
        match self.state {
            StartSetScreenState::SelectServingTeam => self.handle_serving_team_selection(key),
            StartSetScreenState::SelectLineupPlayers(player_position, setter) => {
                self.handle_select_lineup_players_key(key, player_position, setter)
            }
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(body);
        self.render_header(f, rows[0]);
        self.render_error(f, footer_right);
        match self.state {
            StartSetScreenState::SelectServingTeam => {
                self.render_serving_team(f, rows[1], footer_left);
            }
            StartSetScreenState::SelectLineupPlayers(position_index, setter) => {
                self.render_lineup_selection_screen(
                    f,
                    rows[1],
                    position_index,
                    setter,
                    footer_left,
                );
            }
        }
    }
}

impl StartSetScreen {
    fn handle_serving_team_selection(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, self.serving_team) {
            (KeyCode::Left | KeyCode::Right, Some(TeamSideEnum::Them)) => {
                self.serving_team = Some(TeamSideEnum::Us);
            }
            (KeyCode::Left, None) => {
                self.serving_team = Some(TeamSideEnum::Us);
            }
            (KeyCode::Right | KeyCode::Left, Some(TeamSideEnum::Us)) => {
                self.serving_team = Some(TeamSideEnum::Them);
            }
            (KeyCode::Right, None) => {
                self.serving_team = Some(TeamSideEnum::Them);
            }
            (KeyCode::Enter, Some(_)) => {
                self.state = StartSetScreenState::SelectLineupPlayers(0, None);
            }
            (KeyCode::Esc, _) => return AppAction::Back(true, self.back_stack_count),
            _ => {}
        };
        AppAction::None
    }

    fn handle_lineup_selection_back(
        &mut self,
        player_position: usize,
        setter: Option<Uuid>,
    ) -> AppAction {
        match (player_position, self.set_number, setter) {
            // set number (1 or 5) => back to serving team selection
            (0, 1 | 5, _) => {
                self.state = StartSetScreenState::SelectServingTeam;
            }
            // back from the entry point
            (0, _, _) => return AppAction::Back(true, self.back_stack_count),
            // back from setter selection
            (6, _, None) => {
                self.state = StartSetScreenState::SelectLineupPlayers(5, None);
                self.lineup.pop();
                self.list_state.select(None);
            }
            // back from libero selection
            (6, _, Some(_)) => {
                self.state = StartSetScreenState::SelectLineupPlayers(6, None);
                self.initial_setter = None;
                self.list_state.select(None);
            }
            // back to previous position
            (i, _, None) => {
                self.state = StartSetScreenState::SelectLineupPlayers(i - 1, None);
                self.lineup.pop();
                self.list_state.select(None);
            }
            _ => {}
        };
        AppAction::None
    }

    fn handle_libero_selection(
        &mut self,
        player_index: usize,
        available_players: Vec<PlayerEntry>,
    ) -> AppAction {
        if let (Some(libero), Some(serving_team), Some(initial_setter)) = (
            available_players.get(player_index),
            &self.serving_team,
            &self.initial_setter,
        ) {
            self.list_state.select(None);
            if let Ok(lineup) = self
                .lineup
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>()
                .try_into()
            {
                match create_set(
                    &self.current_match,
                    self.set_number,
                    *serving_team,
                    lineup,
                    libero.id,
                    initial_setter.id,
                ) {
                    Ok(set_entry) => match set_entry.compute_snapshot() {
                        Ok((snapshot, available_options)) => {
                            return AppAction::SwitchScreen(Box::new(ScoutingScreen::new(
                                self.current_match.clone(),
                                set_entry,
                                snapshot,
                                available_options,
                                self.back_stack_count.map(|x| x + 1),
                            )))
                        }
                        Err(_) => {
                            self.error = Some(format!(
                                "could not compute snapshot for set {}",
                                self.set_number
                            ));
                        }
                    },
                    Err(_) => {
                        self.error = Some(format!("could not create set {}", self.set_number));
                    }
                }
            } else {
                self.error = Some("lineup must have exactly 6 players".to_string());
            }
        };
        AppAction::None
    }

    fn handle_player_selection(
        &mut self,
        player_index: usize,
        available_players: Vec<PlayerEntry>,
        player_position: usize,
    ) -> AppAction {
        if let Some(player) = available_players.get(player_index) {
            if player_position >= self.lineup.len() {
                self.lineup.push(player.clone());
            } else {
                self.lineup[player_position] = player.clone();
            }
            self.list_state.select(None);
            self.state = StartSetScreenState::SelectLineupPlayers(player_position + 1, None);
        };
        AppAction::None
    }

    fn handle_setter_selection(
        &mut self,
        player_index: usize,
        player_position: usize,
    ) -> AppAction {
        if let Some(player) = self.lineup.get(player_index) {
            self.initial_setter = Some(player.clone());
            self.list_state.select(None);
            self.state = StartSetScreenState::SelectLineupPlayers(player_position, Some(player.id));
        }
        AppAction::None
    }

    fn handle_select_lineup_players_key(
        &mut self,
        key: KeyEvent,
        player_position: usize,
        setter: Option<Uuid>,
    ) -> AppAction {
        let available_players: Vec<PlayerEntry> = self.get_available_players();
        if available_players.is_empty() {
            self.error = Some("no available players to choose from".into());
            return AppAction::None;
        }
        if let Some(selected) = self.list_state.selected() {
            if selected >= available_players.len() {
                self.list_state
                    .select(Some(available_players.len().saturating_sub(1)));
            }
        }
        match (key.code, self.list_state.selected()) {
            // no player selection: select the first one
            (KeyCode::Down | KeyCode::Up, None) => {
                self.list_state.select(Some(0));
                AppAction::None
            }
            // move up
            (KeyCode::Up, Some(i)) => {
                self.list_state.select(Some(if i == 0 {
                    available_players.len() - 1
                } else {
                    i - 1
                }));
                AppAction::None
            }
            // move down
            (KeyCode::Down, Some(i)) => {
                self.list_state
                    .select(Some(if i + 1 >= available_players.len() {
                        0
                    } else {
                        i + 1
                    }));
                AppAction::None
            }
            (KeyCode::Esc, _) => self.handle_lineup_selection_back(player_position, setter),
            (KeyCode::Enter, Some(player_index)) => match (player_position, setter) {
                (0..6, None) => {
                    self.handle_player_selection(player_index, available_players, player_position)
                }
                (6, None) => self.handle_setter_selection(player_index, player_position),
                (6, Some(_)) => self.handle_libero_selection(player_index, available_players),
                _ => AppAction::None,
            },
            _ => AppAction::None,
        }
    }

    pub fn new(
        current_match: MatchEntry,
        set_number: u8,
        serving_team: Option<TeamSideEnum>,
        back_stack_count: Option<u8>,
    ) -> Self {
        StartSetScreen {
            current_match,
            set_number,
            serving_team,
            lineup: vec![],
            initial_setter: None,
            error: None,
            list_state: TableState::default(),
            state: if set_number == 1 || set_number == 5 {
                StartSetScreenState::SelectServingTeam
            } else {
                StartSetScreenState::SelectLineupPlayers(0, None)
            },
            back_stack_count,
        }
    }

    fn get_available_players(&self) -> Vec<PlayerEntry> {
        let selected_ids: HashSet<uuid::Uuid> = self.lineup.iter().map(|p| p.id).collect();
        self.current_match
            .team
            .players
            .iter()
            .filter(|p| !selected_ids.contains(&p.id))
            .cloned()
            .collect()
    }

    fn render_lineup_selection_available_players(
        &mut self,
        f: &mut Frame,
        position_index: usize,
        setter: Option<Uuid>,
        area: Rect,
    ) {
        let header = Row::new(vec!["#", "name", "role"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let available_players: Vec<PlayerEntry> = match (position_index, setter) {
            (6, None) => self.lineup.clone(),
            _ => self.get_available_players(),
        };
        let rows: Vec<Row> = available_players
            .iter()
            .map(|p| {
                Row::new(vec![
                    p.number.to_string(),
                    p.name.clone(),
                    p.role.to_string(),
                ])
            })
            .collect();
        let table = Table::new(
            rows,
            vec![
                Constraint::Percentage(10),
                Constraint::Percentage(50),
                Constraint::Percentage(40),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(match (position_index, setter) {
                    (6, None) => "lineup selection - setter".into(),
                    (_, Some(_)) => "lineup selection - libero".into(),
                    (_, None) => format!("pos {:?}", position_index + 1),
                }),
        )
        .row_highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED),
        )
        .highlight_symbol(">> ");
        f.render_stateful_widget(table, area, &mut self.list_state);
    }

    fn render_lineup_selection_screen(
        &mut self,
        f: &mut Frame,
        area: Rect,
        position_index: usize,
        setter: Option<Uuid>,
        footer_area: Rect,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        self.render_lineup_selection_available_players(f, position_index, setter, chunks[0]);
        self.render_lineup_selection_court(f, chunks[1], position_index, setter);
        self.render_lineup_selection_footer(f, footer_area);
    }

    fn render_lineup_selection_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph =
            Paragraph::new("↑↓ = move | Enter = select | Esc = back | Q = quit").block(block);
        f.render_widget(paragraph, area);
    }

    fn render_lineup_selection_court(
        &self,
        f: &mut Frame,
        area: Rect,
        position_index: usize,
        setter: Option<Uuid>,
    ) {
        let court_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        let position_map = [3, 2, 1, 4, 5, 0];
        for (row_index, row_area) in court_rows.iter().enumerate() {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                    Constraint::Percentage(33),
                ])
                .split(*row_area);

            for (column_index, cell_area) in columns.iter().enumerate() {
                self.render_lineup_selection_court_cell(
                    f,
                    position_index,
                    setter,
                    row_index,
                    column_index,
                    &position_map,
                    cell_area,
                );
            }
        }
    }

    fn render_lineup_selection_court_cell(
        &self,
        f: &mut Frame,
        position_index: usize,
        setter: Option<Uuid>,
        row_index: usize,
        column_index: usize,
        position_map: &[usize; 6],
        area: &Rect,
    ) {
        let cell_index = row_index * 3 + column_index;
        let pos_index = position_map[cell_index];
        let content = if let Some(player) = self.lineup.get(pos_index) {
            if let Some(true) = self.initial_setter.as_ref().map(|s| s.id == player.id) {
                format!("{}\n{}\n(S)", player.number, player.name)
            } else {
                format!("{}\n{}", player.number, player.name)
            }
        } else {
            format!("pos {}", pos_index + 1)
        };
        let cell = Paragraph::new(content)
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL))
            .style(match (position_index, setter) {
                (i, None) => {
                    if pos_index == i as usize {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    }
                }
                (_, Some(_)) => Style::default(),
            });
        f.render_widget(cell, *area);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let title = if self.current_match.home {
            format!(
                "{} vs {}",
                self.current_match.team.name, self.current_match.opponent
            )
        } else {
            format!(
                "{} vs {}",
                self.current_match.opponent, self.current_match.team.name
            )
        };
        let content = Paragraph::new(title)
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(ratatui::layout::Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("set {:?}", self.set_number)),
            );
        f.render_widget(content, area);
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

    fn render_serving_team(&mut self, f: &mut Frame, area: Rect, footer_area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("serving team");
        f.render_widget(block, area);
        fn centered_line_rect(area: Rect, line_height: u16) -> Rect {
            let h = line_height.min(area.height);
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            Rect {
                y,
                height: h,
                ..area
            }
        }
        let mut render_button = |label: &str, area: Rect, selected: bool| {
            let block = Block::default().borders(Borders::ALL).style(if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            });
            f.render_widget(block, area);
            let text = Paragraph::new(label).alignment(Alignment::Center);
            f.render_widget(text, centered_line_rect(area, 1));
        };
        let v = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(40),
            ])
            .split(area);

        let row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
            .split(v[1]);
        let us_area = row[1];
        let them_area = row[3];
        render_button("us", us_area, self.serving_team == Some(TeamSideEnum::Us));
        render_button(
            "them",
            them_area,
            self.serving_team == Some(TeamSideEnum::Them),
        );
        self.render_serving_team_footer(f, footer_area);
    }

    fn render_serving_team_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph =
            Paragraph::new("← → = choose | Enter = confirm | Esc = back | Q = quit").block(block);
        f.render_widget(paragraph, area);
    }
}
