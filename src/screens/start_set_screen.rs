use crate::{
    localization::current_labels,
    ops::create_set,
    screens::{
        scouting_screen::ScoutingScreen,
        screen::{AppAction, Screen},
    },
    shapes::{
        enums::{RoleEnum, TeamSideEnum},
        player::PlayerEntry,
        r#match::MatchEntry,
    },
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
        use StartSetScreenState::*;
        match (&self.state, &self.error) {
            (_, Some(_)) => {
                self.error = None;
                AppAction::None
            }
            (SelectServingTeam, _) => self.handle_serving_team_selection(key),
            (SelectLineupPlayers(player_position, setter), _) => {
                self.handle_select_lineup_players_key(key, *player_position, *setter)
            }
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        use StartSetScreenState::*;
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(body);
        self.render_header(f, rows[0]);
        self.render_error(f, footer_right);
        match self.state {
            SelectServingTeam => {
                self.render_serving_team(f, rows[1], footer_left);
            }
            SelectLineupPlayers(position_index, setter) => {
                if self.list_state.selected().is_none() {
                    self.list_state
                        .select(self.default_select(position_index, setter));
                }
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
        use KeyCode::*;
        use TeamSideEnum::*;
        match (key.code, self.serving_team) {
            (Left | Right, Some(Them)) => {
                self.serving_team = Some(Us);
                AppAction::None
            }
            (Left, None) => {
                self.serving_team = Some(Us);
                AppAction::None
            }
            (Right | Left, Some(Us)) => {
                self.serving_team = Some(Them);
                AppAction::None
            }
            (Right, None) => {
                self.serving_team = Some(Them);
                AppAction::None
            }
            (Enter, Some(_)) => {
                self.state = StartSetScreenState::SelectLineupPlayers(0, None);
                AppAction::None
            }
            (Esc, _) => AppAction::Back(true, self.back_stack_count),
            _ => AppAction::None,
        }
    }

    fn default_select(&self, position_index: usize, setter: Option<Uuid>) -> Option<usize> {
        // se la schermata e' quella del palleggiatore, allora prendi il primo palleggiatore dalla lista di available players e selezionalo di default
        let available_players = self.get_available_players(position_index, setter);
        match (available_players.is_empty(), position_index, setter) {
            (true, _, _) => None,
            (false, 6, None) => available_players
                .iter()
                .position(|p| p.role == RoleEnum::Setter)
                .or(Some(0)),
            (false, 6, Some(_)) => available_players
                .iter()
                .position(|p| p.role == RoleEnum::Libero)
                .or(Some(0)),
            _ => Some(0),
        }
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
                self.list_state.select(self.default_select(5, setter));
            }
            // back from libero selection
            (6, _, Some(_)) => {
                self.state = StartSetScreenState::SelectLineupPlayers(6, None);
                self.initial_setter = None;
                self.list_state.select(self.default_select(6, setter));
            }
            // back to previous position
            (i, _, None) => {
                self.state = StartSetScreenState::SelectLineupPlayers(i - 1, None);
                self.lineup.pop();
                self.list_state.select(self.default_select(i - 1, setter));
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
        let lineup: Result<[Uuid; 6], _> = self
            .lineup
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
            .try_into();
        match (
            available_players.get(player_index),
            &self.serving_team,
            &self.initial_setter,
            lineup,
        ) {
            (Some(libero), Some(serving_team), Some(initial_setter), Ok(lineup)) => {
                // this is due to the start of a new set, so selection is cleaned up here
                self.list_state.select(self.default_select(0, None));
                match create_set(
                    &self.current_match,
                    self.set_number,
                    *serving_team,
                    lineup,
                    libero.id,
                    initial_setter.id,
                )
                .and_then(|set_entry| {
                    set_entry
                        .compute_snapshot()
                        .map(|(snapshot, options)| (set_entry, snapshot, options))
                }) {
                    Ok((set_entry, snapshot, available_options)) => {
                        AppAction::SwitchScreen(Box::new(ScoutingScreen::new(
                            self.current_match.clone(),
                            set_entry,
                            snapshot,
                            available_options,
                            self.back_stack_count.map(|x| x + 1),
                        )))
                    }
                    Err(_) => {
                        self.error = Some(current_labels().could_not_compute_snapshot.to_string());
                        AppAction::None
                    }
                }
            }
            _ => AppAction::None,
        }
    }

    fn handle_player_selection(
        &mut self,
        player_index: usize,
        available_players: Vec<PlayerEntry>,
        player_position: usize,
    ) -> AppAction {
        if let Some(player) = available_players.get(player_index).cloned() {
            match self.lineup.get_mut(player_position) {
                Some(slot) => *slot = player,
                None => self.lineup.push(player),
            }
            self.list_state
                .select(self.default_select(player_position + 1, None));
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
            self.list_state
                .select(self.default_select(player_position, Some(player.id)));
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
        let available_players: Vec<PlayerEntry> =
            self.get_available_players(player_position, setter);
        if available_players.is_empty() {
            self.error = Some(current_labels().no_available_players.to_string());
            return AppAction::None;
        }
        // ensure selected index is not out of bounds
        if let Some(_) = self
            .list_state
            .selected()
            .filter(|&i| i >= available_players.len())
        {
            self.list_state
                .select(Some(available_players.len().saturating_sub(1)));
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

    fn get_available_players(
        &self,
        position_index: usize,
        setter: Option<Uuid>,
    ) -> Vec<PlayerEntry> {
        match (position_index, setter) {
            (6, None) => self.lineup.clone(),
            _ => {
                let selected_ids: HashSet<uuid::Uuid> = self.lineup.iter().map(|p| p.id).collect();
                self.current_match
                    .team
                    .players
                    .iter()
                    .filter(|p| !selected_ids.contains(&p.id))
                    .cloned()
                    .collect()
            }
        }
    }

    fn lineup_selection_title(position_index: usize, setter: Option<Uuid>) -> Block<'static> {
        let title = match (position_index, setter) {
            (6, None) => current_labels().lineup_selection_setter,
            (_, Some(_)) => current_labels().lineup_selection_libero,
            (_, None) => &format!("pos {}", position_index + 1),
        };
        Block::default()
            .borders(Borders::ALL)
            .title(title.to_string())
    }

    fn render_lineup_selection_row(p: &PlayerEntry) -> Row<'static> {
        Row::new(vec![
            p.number.to_string(),
            p.name.clone(),
            p.role.to_string(),
        ])
    }

    fn render_lineup_selection_available_players(
        &mut self,
        f: &mut Frame,
        position_index: usize,
        setter: Option<Uuid>,
        area: Rect,
    ) {
        let header = Row::new(vec!["#", current_labels().name, current_labels().role]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let available_players: Vec<PlayerEntry> =
            self.get_available_players(position_index, setter);
        let rows: Vec<Row> = available_players
            .iter()
            .map(StartSetScreen::render_lineup_selection_row)
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
        .block(StartSetScreen::lineup_selection_title(
            position_index,
            setter,
        ))
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
        let paragraph = Paragraph::new(format!(
            "↑↓ = {} | Enter = {} | Esc = {} | Q = {}",
            current_labels().navigate,
            current_labels().select,
            current_labels().back,
            current_labels().quit
        ))
        .block(block);
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

    fn lineup_selection_court_cell_content(&self, index: usize) -> String {
        if let Some(player) = self.lineup.get(index) {
            if let Some(true) = self.initial_setter.as_ref().map(|s| s.id == player.id) {
                format!(
                    "{}\n{}\n({})",
                    player.number,
                    player.name,
                    current_labels().setter_prefix
                )
            } else {
                format!("{}\n{}", player.number, player.name)
            }
        } else {
            format!("pos {}", index + 1)
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
        let index = position_map[cell_index];
        let cell = Paragraph::new(self.lineup_selection_court_cell_content(index))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL))
            .style(match (position_index, setter) {
                (i, None) => {
                    if index == i as usize {
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
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(current_labels().error),
                );
            f.render_widget(error_widget, area);
        }
    }

    fn render_serving_team_button(f: &mut Frame, label: &str, area: Rect, selected: bool) {
        fn centered_line_rect(area: Rect, line_height: u16) -> Rect {
            let h = line_height.min(area.height);
            let y = area.y + (area.height.saturating_sub(h)) / 2;
            Rect {
                y,
                height: h,
                ..area
            }
        }
        let block: Block<'_> = Block::default().borders(Borders::ALL).style(if selected {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        });
        f.render_widget(block, area);
        let text = Paragraph::new(label).alignment(Alignment::Center);
        f.render_widget(text, centered_line_rect(area, 1));
    }

    fn render_serving_team(&mut self, f: &mut Frame, area: Rect, footer_area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(current_labels().serving_team);
        f.render_widget(block, area);
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
        StartSetScreen::render_serving_team_button(
            f,
            current_labels().us,
            us_area,
            self.serving_team == Some(TeamSideEnum::Us),
        );
        StartSetScreen::render_serving_team_button(
            f,
            current_labels().them,
            them_area,
            self.serving_team == Some(TeamSideEnum::Them),
        );
        self.render_serving_team_footer(f, footer_area);
    }

    fn render_serving_team_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph = Paragraph::new(format!(
            "← → = {} | Enter = {} | Esc = {} | Q = {}",
            current_labels().choose,
            current_labels().confirm,
            current_labels().back,
            current_labels().quit,
        ))
        .block(block);
        f.render_widget(paragraph, area);
    }
}
