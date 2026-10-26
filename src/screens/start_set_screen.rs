use crate::localization::current_labels;
use crate::shapes::keybinding::navigation_keybindings;
use crate::{
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel},
        scouting_screen::ScoutingScreen,
        screen::{
            AppAction, Renderable, Sba, ScreenAsync, change_value_entry, get_keybinding_actions,
        },
    },
    shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings, settings::Settings},
};
use async_trait::async_trait;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Row, Table, TableState},
};
use scout4all_core::{
    providers::set_writer::SetWriter,
    set_start::{self, SetStartForm},
    shapes::{
        enums::{RoleEnum, TeamSideEnum},
        r#match::MatchEntry,
        player::PlayerEntry,
    },
};
use std::sync::Arc;

/// Starting a set: the keys choose who serves, then the players in
/// positions 1 to 6, the setter and the liberos, as the core's
/// `SetStartForm` allows.
#[derive(Debug)]
pub struct StartSetScreen<
    SSW: SetWriter + Send + Sync,
    SW: SettingsWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
> {
    settings_reader: Arc<SR>,
    settings_writer: Arc<SW>,
    settings: Settings,
    current_match: MatchEntry,
    form: SetStartForm,
    notify_message: NotifyBanner,
    step: StartSetStep,
    list_state: TableState,
    back_stack_count: Option<u8>,
    set_writer: Arc<SSW>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

/// What the scout chooses now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartSetStep {
    ServingTeam,
    /// the player in a position, 0 for position 1
    Position(usize),
    Setter,
    Libero,
    FallbackLibero,
}

/// The keys choosing the serving team: left and right change the team, as
/// the values of a form.
const SERVING_TEAM_ACTIONS: [Sba<ScreenActionEnum>; 4] = [
    Sba::Hidden(ScreenActionEnum::Next),
    Sba::Hidden(ScreenActionEnum::Previous),
    Sba::Simple(ScreenActionEnum::Confirm),
    Sba::Simple(ScreenActionEnum::Back),
];

/// The keys choosing the players of the lineup.
const LINEUP_SELECTION_ACTIONS: [Sba<ScreenActionEnum>; 4] = [
    Sba::Simple(ScreenActionEnum::Up),
    Sba::Simple(ScreenActionEnum::Down),
    Sba::Simple(ScreenActionEnum::Select),
    Sba::Simple(ScreenActionEnum::Back),
];

impl<
    SSW: SetWriter + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
> Renderable for StartSetScreen<SSW, SW, SR>
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        match self.step {
            StartSetStep::ServingTeam => [change_value_entry()]
                .into_iter()
                .chain(get_keybinding_actions(
                    navigation_keybindings(),
                    &SERVING_TEAM_ACTIONS,
                ))
                .collect(),
            _ => get_keybinding_actions(navigation_keybindings(), &LINEUP_SELECTION_ACTIONS),
        }
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(body);
        self.render_header(f, rows[0]);
        self.notify_message.render(f, footer_right);
        match self.step {
            StartSetStep::ServingTeam => {
                self.render_serving_team(f, rows[1], footer_left);
            }
            _ => {
                if self.list_state.selected().is_none() {
                    self.list_state.select(self.default_select());
                }
                self.render_lineup_selection_screen(f, rows[1], footer_left);
            }
        }
    }
}

#[async_trait]
impl<
    SSW: SetWriter + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
> ScreenAsync for StartSetScreen<SSW, SW, SR>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
            let action = self.screen_key_bindings.get(key_combination).copied();
            match (self.step, self.notify_message.has_value()) {
                (_, true) => {
                    self.notify_message.reset();
                    AppAction::None
                }
                (StartSetStep::ServingTeam, _) => self.handle_serving_team_selection(action),
                (_, _) => self.handle_select_lineup_players_key(action, key).await,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<
    SSW: SetWriter + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
> StartSetScreen<SSW, SW, SR>
{
    fn handle_serving_team_selection(&mut self, action: Option<ScreenActionEnum>) -> AppAction {
        use TeamSideEnum::*;
        match (action, self.form.serving_team) {
            (Some(ScreenActionEnum::Next | ScreenActionEnum::Previous), Some(Them))
            | (Some(ScreenActionEnum::Previous), None) => {
                self.form.serving_team = Some(Us);
                AppAction::None
            }
            (Some(ScreenActionEnum::Next | ScreenActionEnum::Previous), Some(Us))
            | (Some(ScreenActionEnum::Next), None) => {
                self.form.serving_team = Some(Them);
                AppAction::None
            }
            (Some(ScreenActionEnum::Confirm), Some(_)) => {
                self.go_to(StartSetStep::Position(0));
                AppAction::None
            }
            (Some(ScreenActionEnum::Back), _) => AppAction::Back(true, self.back_stack_count),
            _ => AppAction::None,
        }
    }

    /// The players to choose from in the current step.
    fn choices(&self) -> Vec<PlayerEntry> {
        let team = &self.current_match.team;
        match self.step {
            StartSetStep::ServingTeam => vec![],
            StartSetStep::Position(i) => self.form.position_choices(team, i),
            StartSetStep::Setter => self.form.setter_choices(team),
            StartSetStep::Libero => self.form.libero_choices(team),
            StartSetStep::FallbackLibero => self.form.fallback_libero_choices(team),
        }
        .into_iter()
        .cloned()
        .collect()
    }

    /// The player highlighted when a step begins: the one proposed for it
    /// (the lineup of the last set), else the first setter for the setter,
    /// the first libero for the liberos, else the first player.
    fn default_select(&self) -> Option<usize> {
        let choices = self.choices();
        let proposed = match self.step {
            StartSetStep::ServingTeam => None,
            StartSetStep::Position(i) => self.form.positions.get(i).copied().flatten(),
            StartSetStep::Setter => self.form.setter,
            StartSetStep::Libero => self.form.libero,
            StartSetStep::FallbackLibero => self.form.fallback_libero,
        };
        let preferred = match self.step {
            StartSetStep::Setter => Some(RoleEnum::Setter),
            StartSetStep::Libero | StartSetStep::FallbackLibero => Some(RoleEnum::Libero),
            _ => None,
        };
        let proposed = proposed.and_then(|id| choices.iter().position(|p| p.id == id));
        match (choices.is_empty(), proposed, preferred) {
            (true, _, _) => None,
            (false, Some(i), _) => Some(i),
            (false, None, Some(role)) => choices
                .iter()
                .position(|p| p.role == Some(role))
                .or(Some(0)),
            (false, None, None) => Some(0),
        }
    }

    fn go_to(&mut self, step: StartSetStep) {
        self.step = step;
        self.list_state.select(self.default_select());
    }

    fn handle_lineup_selection_back(&mut self) -> AppAction {
        match (self.step, self.form.chooses_serving_team()) {
            (StartSetStep::Position(0), true) => self.go_to(StartSetStep::ServingTeam),
            (StartSetStep::Position(0), false) => {
                return AppAction::Back(true, self.back_stack_count);
            }
            (StartSetStep::Position(i), _) => {
                self.form.place(i - 1, None);
                self.go_to(StartSetStep::Position(i - 1));
            }
            (StartSetStep::Setter, _) => {
                self.form.place(5, None);
                self.go_to(StartSetStep::Position(5));
            }
            (StartSetStep::Libero, _) => {
                self.form.setter = None;
                self.go_to(StartSetStep::Setter);
            }
            (StartSetStep::FallbackLibero, _) => {
                self.form.libero = None;
                self.go_to(StartSetStep::Libero);
            }
            (StartSetStep::ServingTeam, _) => {}
        };
        AppAction::None
    }

    /// Chooses `player` in the current step, and goes to the next one.
    async fn choose(&mut self, player: &PlayerEntry) -> AppAction {
        match self.step {
            StartSetStep::Position(i) => {
                self.form.place(i, Some(player.id));
                self.go_to(match i {
                    5 => StartSetStep::Setter,
                    _ => StartSetStep::Position(i + 1),
                });
                AppAction::None
            }
            StartSetStep::Setter => {
                self.form.setter = Some(player.id);
                self.go_to(StartSetStep::Libero);
                AppAction::None
            }
            StartSetStep::Libero => {
                self.form.libero = Some(player.id);
                match self
                    .form
                    .fallback_libero_choices(&self.current_match.team)
                    .is_empty()
                {
                    // no fallback libero to choose
                    true => self.start().await,
                    false => {
                        self.go_to(StartSetStep::FallbackLibero);
                        AppAction::None
                    }
                }
            }
            StartSetStep::FallbackLibero => {
                self.form.fallback_libero = Some(player.id);
                self.start().await
            }
            StartSetStep::ServingTeam => AppAction::None,
        }
    }

    /// Saves the set and scouts it.
    async fn start(&mut self) -> AppAction {
        let started = match self.form.clone().validate(&self.current_match.team) {
            Ok(input) => {
                set_start::start(self.current_match.clone(), input, self.set_writer.clone()).await
            }
            Err(e) => Err(e),
        };
        match started {
            Ok(session) => AppAction::SwitchScreen(Box::new(ScoutingScreen::new(
                self.settings_reader.clone(),
                self.settings_writer.clone(),
                self.settings.clone(),
                session,
                self.back_stack_count.map(|x| x + 1),
            ))),
            Err(e) => {
                self.notify_message
                    .set_error(e.message(current_labels().domain));
                AppAction::None
            }
        }
    }

    async fn handle_select_lineup_players_key(
        &mut self,
        action: Option<ScreenActionEnum>,
        key: KeyEvent,
    ) -> AppAction {
        let choices = self.choices();
        if choices.is_empty() {
            self.notify_message
                .set_error(current_labels().no_available_players.to_string());
            return AppAction::None;
        }
        // ensure selected index is not out of bounds
        if self
            .list_state
            .selected()
            .filter(|&i| i >= choices.len())
            .is_some()
        {
            self.list_state
                .select(Some(choices.len().saturating_sub(1)));
        }
        match (action, key.code, self.list_state.selected()) {
            // no player selection: select the first one
            (Some(ScreenActionEnum::Up | ScreenActionEnum::Down), _, None) => {
                self.list_state.select(Some(0));
                AppAction::None
            }
            // move up
            (Some(ScreenActionEnum::Up), _, Some(i)) => {
                self.list_state.select(Some(match i {
                    0 => choices.len() - 1,
                    _ => i - 1,
                }));
                AppAction::None
            }
            // move down
            (Some(ScreenActionEnum::Down), _, Some(i)) => {
                self.list_state
                    .select(Some(if i + 1 >= choices.len() { 0 } else { i + 1 }));
                AppAction::None
            }
            (Some(ScreenActionEnum::Back), _, _) => self.handle_lineup_selection_back(),
            (Some(ScreenActionEnum::Select), _, Some(i)) => match choices.get(i) {
                Some(player) => self.choose(player).await,
                None => AppAction::None,
            },
            // no fallback libero
            (None, KeyCode::Tab, _) if self.step == StartSetStep::FallbackLibero => {
                self.form.fallback_libero = None;
                self.start().await
            }
            _ => AppAction::None,
        }
    }

    pub fn new(
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
        settings: Settings,
        current_match: MatchEntry,
        form: SetStartForm,
        back_stack_count: Option<u8>,
        set_writer: Arc<SSW>,
    ) -> Self {
        StartSetScreen {
            settings_reader,
            settings_writer,
            settings,
            current_match,
            step: match form.chooses_serving_team() {
                true => StartSetStep::ServingTeam,
                false => StartSetStep::Position(0),
            },
            form,
            notify_message: NotifyBanner::new(),
            list_state: TableState::default(),
            back_stack_count,
            set_writer,
            screen_key_bindings: ActionsKeyBindings::empty(),
        }
    }

    fn lineup_selection_title(&self) -> Block<'static> {
        let title = match self.step {
            StartSetStep::Setter => current_labels().lineup_selection_setter.to_string(),
            StartSetStep::Libero => current_labels().lineup_selection_libero.to_string(),
            StartSetStep::FallbackLibero => current_labels()
                .lineup_selection_fallback_libero
                .to_string(),
            StartSetStep::Position(i) => format!("pos {}", i + 1),
            StartSetStep::ServingTeam => String::new(),
        };
        panel::section(title)
    }

    fn render_lineup_selection_row(p: &PlayerEntry) -> Row<'static> {
        Row::new(vec![
            p.number.to_string(),
            p.name.clone(),
            p.role.map_or_else(|| "-".to_string(), |r| r.to_string()),
        ])
    }

    fn render_lineup_selection_available_players(&mut self, f: &mut Frame, area: Rect) {
        let area = match self.step {
            StartSetStep::FallbackLibero => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(1)])
                    .split(area);
                let hint = Paragraph::new(current_labels().skip_fallback_libero_hint)
                    .style(Style::default().fg(Color::Cyan))
                    .block(
                        panel::section(current_labels().hint)
                            .border_style(Style::default().fg(Color::Cyan)),
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
        let rows: Vec<Row> = self
            .choices()
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
        .block(self.lineup_selection_title())
        .row_highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::REVERSED),
        )
        .highlight_symbol(">> ");
        f.render_stateful_widget(table, area, &mut self.list_state);
    }

    fn render_lineup_selection_screen(&mut self, f: &mut Frame, area: Rect, footer_area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        self.render_lineup_selection_available_players(f, chunks[0]);
        self.render_lineup_selection_court(f, chunks[1]);
        self.render_lineup_selection_footer(f, footer_area);
    }

    fn render_lineup_selection_footer(&mut self, f: &mut Frame, area: Rect) {
        self.screen_key_bindings =
            navigation_keybindings().slice(Sba::keys(&LINEUP_SELECTION_ACTIONS));
        NavigationFooter::new().render(f, area, self.footer_entries());
    }

    fn render_lineup_selection_court(&self, f: &mut Frame, area: Rect) {
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
                    row_index,
                    column_index,
                    &position_map,
                    cell_area,
                );
            }
        }
    }

    fn lineup_selection_court_cell_content(&self, index: usize) -> String {
        let player = self
            .form
            .positions
            .get(index)
            .copied()
            .flatten()
            .and_then(|id| self.current_match.team.find_player(id));
        match (player, player.map(|p| Some(p.id) == self.form.setter)) {
            (Some(player), Some(true)) => format!(
                "{}\n{}\n({})",
                player.number,
                player.name,
                current_labels().domain.setter_prefix
            ),
            (Some(player), _) => format!("{}\n{}", player.number, player.name),
            (None, _) => format!("pos {}", index + 1),
        }
    }

    fn render_lineup_selection_court_cell(
        &self,
        f: &mut Frame,
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
            .style(match self.step {
                StartSetStep::Position(i) if i == index => {
                    Style::default().add_modifier(Modifier::REVERSED)
                }
                _ => Style::default(),
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
            .block(panel::screen(format!("set {:?}", self.form.set_number)));
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
        let block = panel::section(current_labels().serving_team);
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
            self.form.serving_team == Some(TeamSideEnum::Us),
        );
        Self::render_serving_team_button(
            f,
            current_labels().them,
            them_area,
            self.form.serving_team == Some(TeamSideEnum::Them),
        );
        self.render_serving_team_footer(f, footer_area);
    }

    fn render_serving_team_footer(&mut self, f: &mut Frame, area: Rect) {
        self.screen_key_bindings = navigation_keybindings().slice(Sba::keys(&SERVING_TEAM_ACTIONS));
        NavigationFooter::new().render(f, area, self.footer_entries());
    }
}
