use crate::{
    localization::current_labels,
    providers::set_writer::SetWriter,
    screens::{
        components::notify_banner::NotifyBanner,
        scouting_screen::ScoutingScreen,
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
    },
    shapes::{
        enums::{RoleEnum, ScreenActionEnum, TeamSideEnum},
        keybinding::KeyBindings,
        player::PlayerEntry,
        r#match::MatchEntry,
        settings::Settings,
    },
};
use async_trait::async_trait;
use crokey::{
    crossterm::event::{KeyCode, KeyEvent},
    Combiner,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Padding, Paragraph, Row, Table, TableState},
    Frame,
};
use std::{collections::HashSet, sync::Arc};
use uuid::Uuid;

#[derive(Debug)]
pub struct StartSetScreen<SSW: SetWriter + Send + Sync> {
    settings: Settings,
    current_match: MatchEntry,
    set_number: u8,
    lineup: Vec<PlayerEntry>,
    initial_setter: Option<PlayerEntry>,
    initial_libero: Option<PlayerEntry>,
    notify_message: NotifyBanner,
    state: StartSetScreenState,
    serving_team: Option<TeamSideEnum>,
    list_state: TableState,
    back_stack_count: Option<u8>,
    set_writer: Arc<SSW>,
    combiner: crokey::Combiner,
    screen_key_bindings: crate::shapes::keybinding::KeyBindings,
}

#[derive(Debug)]
pub enum StartSetScreenState {
    SelectServingTeam,
    // (player position index, setter, libero)
    SelectLineupPlayers(usize, Option<Uuid>, Option<Uuid>),
}

impl<SSW: SetWriter + Send + Sync + 'static> Renderable for StartSetScreen<SSW> {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        use StartSetScreenState::*;
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(body);
        self.render_header(f, rows[0]);
        self.notify_message.render(f, footer_right);
        match self.state {
            SelectServingTeam => {
                self.render_serving_team(f, rows[1], footer_left);
            }
            SelectLineupPlayers(position_index, setter, libero) => {
                if self.list_state.selected().is_none() {
                    self.list_state
                        .select(self.default_select(position_index, setter, libero));
                }
                self.render_lineup_selection_screen(
                    f,
                    rows[1],
                    position_index,
                    setter,
                    libero,
                    footer_left,
                );
            }
        }
    }
}

#[async_trait]
impl<SSW: SetWriter + Send + Sync + 'static> ScreenAsync for StartSetScreen<SSW> {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        use StartSetScreenState::*;
        if let Some(key_combination) = self.combiner.transform(key) {
            let action = self.screen_key_bindings.get(key_combination).copied();
            match (&self.state, &self.notify_message.has_value()) {
                (_, true) => {
                    self.notify_message.reset();
                    AppAction::None
                }
                (SelectServingTeam, _) => self.handle_serving_team_selection(action),
                (SelectLineupPlayers(current_player_position, setter, libero), _) => {
                    self.handle_select_lineup_players_key(
                        action,
                        key,
                        *current_player_position,
                        *setter,
                        *libero,
                    )
                    .await
                }
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<SSW: SetWriter + Send + Sync + 'static> StartSetScreen<SSW> {
    fn handle_serving_team_selection(&mut self, action: Option<ScreenActionEnum>) -> AppAction {
        use TeamSideEnum::*;
        match (action, self.serving_team) {
            (Some(ScreenActionEnum::Next | ScreenActionEnum::Previous), Some(Them)) => {
                self.serving_team = Some(Us);
                AppAction::None
            }
            (Some(ScreenActionEnum::Previous), None) => {
                self.serving_team = Some(Us);
                AppAction::None
            }
            (Some(ScreenActionEnum::Next | ScreenActionEnum::Previous), Some(Us)) => {
                self.serving_team = Some(Them);
                AppAction::None
            }
            (Some(ScreenActionEnum::Next), None) => {
                self.serving_team = Some(Them);
                AppAction::None
            }
            (Some(ScreenActionEnum::Confirm), Some(_)) => {
                self.state = StartSetScreenState::SelectLineupPlayers(0, None, None);
                AppAction::None
            }
            (Some(ScreenActionEnum::Back), _) => AppAction::Back(true, self.back_stack_count),
            _ => AppAction::None,
        }
    }

    fn default_select(
        &self,
        position_index: usize,
        setter: Option<Uuid>,
        libero: Option<Uuid>,
    ) -> Option<usize> {
        let available_players = self.get_available_players(position_index, setter, libero);
        match (available_players.is_empty(), position_index, setter, libero) {
            (true, _, _, _) => None,
            // on setter selection, grab the first setter from available players as default
            (false, 6, None, _) => available_players
                .iter()
                .position(|p| p.role == Some(RoleEnum::Setter))
                .or(Some(0)),
            // on libero/fallbacklibero selection, grab the first libero from available players as default
            (false, 6, Some(_), None | Some(_)) => available_players
                .iter()
                .position(|p| p.role == Some(RoleEnum::Libero))
                .or(Some(0)),
            _ => Some(0),
        }
    }

    fn handle_lineup_selection_back(
        &mut self,
        current_player_position: usize,
        setter: Option<Uuid>,
        libero: Option<Uuid>,
    ) -> AppAction {
        match (current_player_position, self.set_number, setter, libero) {
            // set number (1 or 5) => back to serving team selection
            (0, 1 | 5, _, _) => {
                self.state = StartSetScreenState::SelectServingTeam;
            }
            // back from the entry point
            (0, _, _, _) => return AppAction::Back(true, self.back_stack_count),
            // back from setter selection
            (6, _, None, _) => {
                self.state = StartSetScreenState::SelectLineupPlayers(5, None, None);
                self.lineup.pop();
                self.list_state.select(self.default_select(5, None, None));
            }
            // back from libero selection
            (6, _, Some(_), None) => {
                self.state = StartSetScreenState::SelectLineupPlayers(6, None, None);
                self.initial_setter = None;
                self.list_state.select(self.default_select(6, None, None));
            }
            // back from fallback libero selection
            (6, _, Some(_), Some(_)) => {
                self.state = StartSetScreenState::SelectLineupPlayers(6, setter, None);
                self.initial_libero = None;
                self.list_state.select(self.default_select(6, setter, None));
            }
            // back to previous position
            (i, _, None, None) => {
                self.state = StartSetScreenState::SelectLineupPlayers(i - 1, None, None);
                self.lineup.pop();
                self.list_state
                    .select(self.default_select(i - 1, None, None));
            }
            _ => {}
        };
        AppAction::None
    }

    async fn handle_fallback_libero_selection(
        &mut self,
        selection_index: Option<usize>,
        available_players: Vec<PlayerEntry>,
    ) -> AppAction {
        let lineup: Result<[Uuid; 6], _> = self
            .lineup
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
            .try_into();
        match (
            selection_index.and_then(|i| available_players.get(i).cloned()),
            &self.serving_team,
            &self.initial_setter,
            &self.initial_libero,
            lineup,
        ) {
            (
                fallback_libero,
                Some(serving_team),
                Some(initial_setter),
                Some(initial_libero),
                Ok(lineup),
            ) => {
                // this is due to the start of a new set, so selection is cleaned up here
                self.list_state.select(self.default_select(0, None, None));
                match self
                    .set_writer
                    .create(
                        &self.current_match,
                        self.set_number,
                        *serving_team,
                        lineup,
                        initial_libero.id,
                        fallback_libero.map(|f| f.id),
                        initial_setter.id,
                        vec![], // no events at the start of the set
                    )
                    .await
                    .and_then(|set_entry| {
                        set_entry
                            .compute_snapshot()
                            .map(|(snapshot, options)| (set_entry, snapshot, options))
                    }) {
                    Ok((set_entry, snapshot, available_options)) => {
                        AppAction::SwitchScreen(Box::new(ScoutingScreen::new(
                            self.settings.clone(),
                            self.current_match.clone(),
                            set_entry,
                            snapshot,
                            available_options,
                            self.back_stack_count.map(|x| x + 1),
                            self.set_writer.clone(),
                        )))
                    }
                    Err(_) => {
                        self.notify_message
                            .set_error(current_labels().could_not_compute_snapshot.to_string());
                        AppAction::None
                    }
                }
            }
            _ => AppAction::None,
        }
    }

    fn handle_player_selection(
        &mut self,
        selection_index: usize,
        available_players: Vec<PlayerEntry>,
        current_player_position: usize,
    ) -> AppAction {
        if let Some(player) = available_players.get(selection_index).cloned() {
            match self.lineup.get_mut(current_player_position) {
                Some(slot) => *slot = player,
                None => self.lineup.push(player),
            }
            self.list_state
                .select(self.default_select(current_player_position + 1, None, None));
            self.state =
                StartSetScreenState::SelectLineupPlayers(current_player_position + 1, None, None);
        };
        AppAction::None
    }

    fn handle_setter_selection(&mut self, selection_index: usize) -> AppAction {
        if let Some(setter) = self.lineup.get(selection_index) {
            self.initial_setter = Some(setter.clone());
            self.list_state
                .select(self.default_select(6, Some(setter.id), None));
            self.state = StartSetScreenState::SelectLineupPlayers(6, Some(setter.id), None);
        }
        AppAction::None
    }

    async fn handle_libero_selection(
        &mut self,
        selection_index: usize,
        available_players: Vec<PlayerEntry>,
    ) -> AppAction {
        if let (Some(libero), Some(setter_id)) = (
            available_players.get(selection_index).cloned(),
            self.initial_setter.clone().map(|s| s.id),
        ) {
            self.initial_libero = Some(libero.clone());
            self.list_state
                .select(self.default_select(6, Some(setter_id), Some(libero.id)));
            self.state =
                StartSetScreenState::SelectLineupPlayers(6, Some(setter_id), Some(libero.id));
            if self.current_match.team.players.len() < 8 {
                // cannot have a second libero with less than 8 players
                self.handle_fallback_libero_selection(None, available_players)
                    .await
            } else {
                AppAction::None
            }
        } else {
            AppAction::None
        }
    }

    async fn handle_select_lineup_players_key(
        &mut self,
        action: Option<ScreenActionEnum>,
        key: KeyEvent,
        current_player_position: usize,
        setter: Option<Uuid>,
        libero: Option<Uuid>,
    ) -> AppAction {
        let available_players: Vec<PlayerEntry> =
            self.get_available_players(current_player_position, setter, libero);
        if available_players.is_empty() {
            self.notify_message
                .set_error(current_labels().no_available_players.to_string());
            return AppAction::None;
        }
        // ensure selected index is not out of bounds
        if self
            .list_state
            .selected()
            .filter(|&i| i >= available_players.len())
            .is_some()
        {
            self.list_state
                .select(Some(available_players.len().saturating_sub(1)));
        }
        match (action, key.code, self.list_state.selected()) {
            // no player selection: select the first one
            (Some(ScreenActionEnum::Up | ScreenActionEnum::Down), _, None) => {
                self.list_state.select(Some(0));
                AppAction::None
            }
            // move up
            (Some(ScreenActionEnum::Up), _, Some(i)) => {
                self.list_state.select(Some(if i == 0 {
                    available_players.len() - 1
                } else {
                    i - 1
                }));
                AppAction::None
            }
            // move down
            (Some(ScreenActionEnum::Down), _, Some(i)) => {
                self.list_state
                    .select(Some(if i + 1 >= available_players.len() {
                        0
                    } else {
                        i + 1
                    }));
                AppAction::None
            }
            (Some(ScreenActionEnum::Back), _, _) => {
                self.handle_lineup_selection_back(current_player_position, setter, libero)
            }
            (Some(ScreenActionEnum::Select), _, Some(selection_index)) => {
                match (current_player_position, setter, libero) {
                    // players selection
                    (0..6, None, _) => self.handle_player_selection(
                        selection_index,
                        available_players,
                        current_player_position,
                    ),
                    // setter selection
                    (6, None, _) => self.handle_setter_selection(selection_index),
                    // libero selection
                    (6, Some(_), None) => {
                        self.handle_libero_selection(selection_index, available_players)
                            .await
                    }
                    (6, Some(_), Some(_)) => {
                        self.handle_fallback_libero_selection(
                            Some(selection_index),
                            available_players,
                        )
                        .await
                    }
                    _ => AppAction::None,
                }
            }
            (None, KeyCode::Tab, _) => {
                if matches!(
                    (current_player_position, setter, libero),
                    (6, Some(_), Some(_))
                ) {
                    self.handle_fallback_libero_selection(None, available_players)
                        .await
                } else {
                    AppAction::None
                }
            }
            _ => AppAction::None,
        }
    }

    pub fn new(
        settings: Settings,
        current_match: MatchEntry,
        set_number: u8,
        serving_team: Option<TeamSideEnum>,
        back_stack_count: Option<u8>,
        set_writer: Arc<SSW>,
    ) -> Self {
        StartSetScreen {
            settings,
            current_match,
            set_number,
            serving_team,
            lineup: vec![],
            initial_setter: None,
            initial_libero: None,
            notify_message: NotifyBanner::new(),
            list_state: TableState::default(),
            state: if set_number == 1 || set_number == 5 {
                StartSetScreenState::SelectServingTeam
            } else {
                StartSetScreenState::SelectLineupPlayers(0, None, None)
            },
            back_stack_count,
            set_writer,
            combiner: Combiner::default(),
            screen_key_bindings: KeyBindings::default(),
        }
    }

    fn get_available_players(
        &self,
        position_index: usize,
        setter: Option<Uuid>,
        libero: Option<Uuid>,
    ) -> Vec<PlayerEntry> {
        match (position_index, setter, libero) {
            // setter selection: choose only from lineup
            (6, None, None) => self.lineup.clone(),
            _ => {
                let mut selected_ids: HashSet<Uuid> = self.lineup.iter().map(|p| p.id).collect();
                if let Some(initial_libero) = &self.initial_libero {
                    selected_ids.insert(initial_libero.id);
                }
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

    fn lineup_selection_title(
        position_index: usize,
        setter: Option<Uuid>,
        libero: Option<Uuid>,
    ) -> Block<'static> {
        let title = match (position_index, setter, libero) {
            (6, None, _) => current_labels().lineup_selection_setter,
            (_, Some(_), None) => current_labels().lineup_selection_libero,
            (_, Some(_), Some(_)) => current_labels().lineup_selection_fallback_libero,
            _ => &format!("pos {}", position_index + 1),
        };
        Block::default()
            .borders(Borders::ALL)
            .title(title.to_string())
    }

    fn render_lineup_selection_row(p: &PlayerEntry) -> Row<'static> {
        Row::new(vec![
            p.number.to_string(),
            p.name.clone(),
            p.role.map_or_else(|| "-".to_string(), |r| r.to_string()),
        ])
    }

    fn render_lineup_selection_available_players(
        &mut self,
        f: &mut Frame,
        position_index: usize,
        setter: Option<Uuid>,
        libero: Option<Uuid>,
        area: Rect,
    ) {
        let area = match (setter, libero) {
            (Some(_), Some(_)) => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(1)])
                    .split(area);
                let hint = Paragraph::new(current_labels().skip_fallback_libero_hint)
                    .style(Style::default().fg(Color::Cyan))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Cyan))
                            .title(current_labels().hint),
                    );
                f.render_widget(hint, chunks[0]);
                chunks[1]
            }
            _ => area,
        };
        let header = Row::new(vec!["#", current_labels().name, current_labels().role]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let available_players: Vec<PlayerEntry> =
            self.get_available_players(position_index, setter, libero);
        let rows: Vec<Row> = available_players
            .iter()
            .map(Self::render_lineup_selection_row)
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
        .block(Self::lineup_selection_title(position_index, setter, libero))
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
        libero: Option<Uuid>,
        footer_area: Rect,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        self.render_lineup_selection_available_players(
            f,
            position_index,
            setter,
            libero,
            chunks[0],
        );
        self.render_lineup_selection_court(f, chunks[1], position_index, setter);
        self.render_lineup_selection_footer(f, footer_area);
    }

    fn render_lineup_selection_footer(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));

        let lineup_selection_actions = &[
            Sba::Simple(ScreenActionEnum::Up),
            Sba::Simple(ScreenActionEnum::Down),
            Sba::Simple(ScreenActionEnum::Select),
            Sba::Simple(ScreenActionEnum::Back),
            Sba::Simple(ScreenActionEnum::Quit),
        ];
        self.screen_key_bindings = self
            .settings
            .keybindings
            .slice(Sba::keys(lineup_selection_actions));
        self.screen_key_bindings
            .slice(Sba::keys(lineup_selection_actions));
        let footer_entries =
            get_keybinding_actions(&self.settings.keybindings, lineup_selection_actions);
        let paragraph = Paragraph::new(
            footer_entries
                .iter()
                .map(|(key, desc)| format!("{} = {}", key, desc))
                .collect::<Vec<_>>()
                .join(" | "),
        )
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
                    if index == i {
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
        Self::render_serving_team_button(
            f,
            current_labels().us,
            us_area,
            self.serving_team == Some(TeamSideEnum::Us),
        );
        Self::render_serving_team_button(
            f,
            current_labels().them,
            them_area,
            self.serving_team == Some(TeamSideEnum::Them),
        );
        self.render_serving_team_footer(f, footer_area);
    }

    fn render_serving_team_footer(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));

        let serving_team_actions = &[
            Sba::Simple(ScreenActionEnum::Next),
            Sba::Simple(ScreenActionEnum::Previous),
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
            Sba::Simple(ScreenActionEnum::Quit),
        ];
        self.screen_key_bindings = self
            .settings
            .keybindings
            .slice(Sba::keys(serving_team_actions));
        self.screen_key_bindings
            .slice(Sba::keys(serving_team_actions));
        let footer_entries =
            get_keybinding_actions(&self.settings.keybindings, serving_team_actions);
        let paragraph = Paragraph::new(
            footer_entries
                .iter()
                .map(|(key, desc)| format!("{} = {}", key, desc))
                .collect::<Vec<_>>()
                .join(" | "),
        )
        .block(block);
        f.render_widget(paragraph, area);
    }
}
