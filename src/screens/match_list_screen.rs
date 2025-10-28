use std::{path::PathBuf, sync::Arc};

use crate::{
    errors::AppError,
    localization::current_labels,
    providers::{
        match_reader::MatchReader, match_writer::MatchWriter, set_writer::SetWriter,
        settings_reader::SettingsReader, settings_writer::SettingsWriter,
    },
    reporting::pdf::open_match_pdf,
    screens::{
        add_match_screen::AddMatchScreen,
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner,
            team_header::TeamHeader,
        },
        export_match_screen::ExportMatchAction,
        file_system_screen::FileSystemScreen,
        import_match_screen::ImportMatchAction,
        match_stats_screen::MatchStatsScreen,
        scouting_screen::ScoutingScreen,
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
        start_set_screen::StartSetScreen,
    },
    shapes::{
        enums::{ScreenActionEnum, TeamSideEnum},
        keybinding::KeyBindings,
        r#match::{MatchEntry, MatchStatus},
        set::SetEntry,
        settings::Settings,
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use crokey::{crossterm::event::KeyEvent, Combiner};
use dirs::home_dir;
use ratatui::{
    layout::Alignment,
    widgets::{Row, Table},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, ListState, Paragraph},
    Frame,
};

#[derive(Debug)]
pub struct MatchListScreen<
    MR: MatchReader + Send + Sync,
    MW: MatchWriter + Send + Sync,
    SSW: SetWriter + Send + Sync,
    SR: SettingsReader + Send + Sync,
    SW: SettingsWriter + Send + Sync,
> {
    settings: Settings,
    list_state: ListState,
    team: TeamEntry,
    matches: Vec<(MatchEntry, MatchStatus)>,
    notify_message: NotifyBanner,
    header: TeamHeader,
    footer: NavigationFooter,
    base_path: PathBuf,
    match_reader: Arc<MR>,
    match_writer: Arc<MW>,
    set_writer: Arc<SSW>,
    settings_reader: Arc<SR>,
    settings_writer: Arc<SW>,
    combiner: Combiner,
    screen_key_bindings: KeyBindings,
}

impl<
        MR: MatchReader + Send + Sync + 'static,
        MW: MatchWriter + Send + Sync + 'static,
        SSW: SetWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
        SW: SettingsWriter + Send + Sync + 'static,
    > Renderable for MatchListScreen<MR, MW, SSW, SR, SW>
{
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);
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
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(current_labels().match_list),
            )
            .widths([
                Constraint::Length(14),
                Constraint::Length(30),
                Constraint::Length(30),
                Constraint::Length(17),
                Constraint::Length(20),
            ]);
            if self.matches.is_empty() {
                self.render_no_matches_yet(f, container[1]);
            } else {
                f.render_widget(table, container[1]);
            }
        } else {
            self.notify_message
                .set_error(current_labels().could_not_render_match_list.to_string());
        }
        self.header.render(f, container[0], Some(&self.team));
        self.notify_message.render(f, footer_right);
        let kb: &KeyBindings = &self.settings.keybindings;
        let screen_actions = &self.screen_actions();
        let sc = screen_actions.iter().map(|f| f.0).collect();
        let screen_key_bindings = &kb.slice(sc);
        let footer_entries =
            get_keybinding_actions(kb, Sba::MappedAction(screen_actions));
        self.footer.render(f, footer_left, footer_entries);
        self.screen_key_bindings = screen_key_bindings.to_owned()
    }
}

#[async_trait]
impl<
        MR: MatchReader + Send + Sync + 'static,
        MW: MatchWriter + Send + Sync + 'static,
        SSW: SetWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
        SW: SettingsWriter + Send + Sync + 'static,
    > ScreenAsync for MatchListScreen<MR, MW, SSW, SR, SW>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.combiner.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                &self.notify_message.has_value(),
            ) {
                (_, _, true) => {
                    self.notify_message.reset();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Next), _, _) => self.next_match(),
                (Some(ScreenActionEnum::Previous), _, _) => self.previous_match(),
                (Some(ScreenActionEnum::Select), _, _) => self.handle_enter_key(),
                (Some(ScreenActionEnum::PrintReport), _, _) => self.handle_print(),
                (Some(ScreenActionEnum::MatchStats), _, _) => self.handle_space_key(),
                (Some(ScreenActionEnum::Back), _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::New), _, _) => {
                    if self.team.players.len() >= 6 {
                        AppAction::SwitchScreen(Box::new(AddMatchScreen::new(
                            self.settings.clone(),
                            self.team.clone(),
                            self.match_writer.clone(),
                            self.set_writer.clone(),
                        )))
                    } else {
                        AppAction::None
                    }
                }
                (Some(ScreenActionEnum::Import), _, _) => match home_dir() {
                    Some(path) => AppAction::SwitchScreen(Box::new(FileSystemScreen::new(
                        self.settings.clone(),
                        path,
                        current_labels().import_match,
                        ImportMatchAction::new(
                            self.team.clone(),
                            self.match_reader.clone(),
                            self.match_writer.clone(),
                            self.set_writer.clone(),
                        ),
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
                },
                (Some(ScreenActionEnum::Export), _, _) => {
                    let selected_match = self.get_selected_match();
                    match (home_dir(), selected_match) {
                        (Some(path), Some((match_entry, _))) => {
                            AppAction::SwitchScreen(Box::new(FileSystemScreen::new(
                                self.settings.clone(),
                                path,
                                current_labels().export,
                                ExportMatchAction::new(
                                    self.team.clone(),
                                    match_entry.id.clone(),
                                    self.base_path.clone(),
                                ),
                                self.settings_reader.clone(),
                                self.settings_writer.clone(),
                            )))
                        }
                        (None, _) => {
                            self.notify_message.set_error(
                                current_labels()
                                    .could_not_recognize_home_directory
                                    .to_string(),
                            );
                            AppAction::None
                        }
                        (_, None) => {
                            self.notify_message
                                .set_error(current_labels().no_match_selected.to_string());
                            AppAction::None
                        }
                    }
                }
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }
    async fn refresh_data(&mut self) {
        match self.match_reader.read_all(&self.team).await {
            Ok(matches) => {
                let matches = matches
                    .into_iter()
                    .filter_map(|m| m.get_status().ok().map(|s| (m, s)))
                    .collect::<Vec<_>>();
                self.matches = matches;
                if self.matches.is_empty() {
                    self.list_state.select(None);
                } else if let Some(selected) = self.list_state.selected() {
                    if selected >= self.matches.len() {
                        self.list_state.select(Some(self.matches.len() - 1));
                    }
                } else {
                    self.list_state.select(Some(0));
                }
            }
            Err(_) => {
                self.notify_message
                    .set_error(current_labels().could_not_load_matches.to_string());
            }
        }
    }
}

impl<
        MR: MatchReader + Send + Sync + 'static,
        MW: MatchWriter + Send + Sync + 'static,
        SSW: SetWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
        SW: SettingsWriter + Send + Sync + 'static,
    > MatchListScreen<MR, MW, SSW, SR, SW>
{
    pub fn new(
        settings: Settings,
        team: TeamEntry,
        matches: Vec<MatchEntry>,
        base_path: PathBuf,
        match_reader: Arc<MR>,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
    ) -> Self {
        let matches = matches
            .into_iter()
            .filter_map(|m| m.get_status().ok().map(|s| (m, s)))
            .collect::<Vec<_>>();
        MatchListScreen {
            settings,
            combiner: Combiner::default(),
            matches,
            team,
            list_state: ListState::default(),
            base_path,
            notify_message: NotifyBanner::new(),
            header: TeamHeader::default(),
            footer: NavigationFooter::new(),
            match_reader,
            match_writer,
            set_writer,
            settings_reader,
            settings_writer,
            screen_key_bindings: KeyBindings::empty(),
        }
    }
    fn get_selected_match(&self) -> Option<(&MatchEntry, &MatchStatus)> {
        self.list_state
            .selected()
            .and_then(|i| self.matches.get(i))
            .map(|(m, s)| (m, s))
    }

    fn get_match_row(
        &self,
        m: &(MatchEntry, MatchStatus),
        row_index: usize,
        match_index: usize,
    ) -> Result<Row<'_>, AppError> {
        let (m, status) = m;
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
                current_labels().in_progress
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
            self.settings.clone(),
            m.clone(),
            next_set_number,
            match (next_set_number, last_serving_team) {
                (_, None) => None,
                (1 | 5, _) => None,
                (_, Some(TeamSideEnum::Them)) => Some(TeamSideEnum::Us),
                (_, Some(TeamSideEnum::Us)) => Some(TeamSideEnum::Them),
            },
            Some(1),
            self.set_writer.clone(),
        )))
    }

    fn continue_set(&mut self, m: &MatchEntry, last_incomplete_set: SetEntry) -> AppAction {
        match last_incomplete_set.compute_snapshot() {
            Ok((snapshot, available_options)) => {
                AppAction::SwitchScreen(Box::new(ScoutingScreen::new(
                    self.settings.clone(),
                    m.clone(),
                    last_incomplete_set,
                    snapshot,
                    available_options,
                    Some(1),
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

    fn screen_actions(&self) -> Vec<(&ScreenActionEnum, Option<fn(String) -> String>)> {
        let mut actions: Vec<(&ScreenActionEnum, Option<fn(String) -> String>)> = Vec::new();
        actions.push((
            &ScreenActionEnum::Import,
            Some(|lbl| -> String { lbl.replace("{}", current_labels().match_word) }),
        ));

        if !self.matches.is_empty() {
            actions.push((&ScreenActionEnum::Next, None));
            actions.push((&ScreenActionEnum::Previous, None));
        }
        if let Some((_, status)) = self.get_selected_match() {
            if !status.match_finished {
                actions.push((&ScreenActionEnum::Select, None));
            }
            actions.push((&ScreenActionEnum::Export, None));
            actions.push((&ScreenActionEnum::MatchStats, None));
            actions.push((&ScreenActionEnum::PrintReport, None));
        }
        if self.team.players.len() >= 6 {
            actions.push((&ScreenActionEnum::New, None));
        };
        actions.push((&ScreenActionEnum::Back, None));
        actions.push((&ScreenActionEnum::Quit, None));
        actions
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
        let paragraph = Paragraph::new(current_labels().no_matches_yet)
            .block(Block::default().borders(Borders::NONE))
            .alignment(Alignment::Center);
        f.render_widget(paragraph, chunks[1]);
    }

    fn handle_space_key(&mut self) -> AppAction {
        match self.get_selected_match() {
            Some((m, _)) => match MatchStatsScreen::new(self.settings.clone(), m.clone()) {
                Ok(screen) => AppAction::SwitchScreen(Box::new(screen)),
                Err(_) => {
                    self.notify_message
                        .set_error(current_labels().could_not_open_match_stats.to_string());
                    AppAction::None
                }
            },
            None => {
                self.notify_message
                    .set_error(current_labels().no_match_selected.to_string());
                AppAction::None
            }
        }
    }

    fn handle_enter_key(&mut self) -> AppAction {
        let selected = self.get_selected_match().map(|(m, s)| (m.clone(), s));
        if let Some((match_entry, status)) = selected {
            match (
                status.match_finished,
                status.last_incomplete_set.clone(),
                status.next_set_number,
            ) {
                (false, None, Some(next_set_number)) => {
                    self.new_set(&match_entry, next_set_number, status.last_serving_team)
                }
                (false, Some(last_set), _) => self.continue_set(&match_entry, last_set),
                _ => AppAction::None,
            }
        } else {
            self.notify_message
                .set_error(current_labels().no_match_selected.to_string());
            AppAction::None
        }
    }

    fn handle_print(&mut self) -> AppAction {
        let selected = self.get_selected_match().map(|(m, s)| (m.clone(), s));
        if let Some((match_entry, _)) = selected {
            match open_match_pdf(&match_entry) {
                Ok(_) => AppAction::None,
                Err(_) => {
                    self.notify_message
                        .set_error(current_labels().could_not_open_pdf.to_string());
                    AppAction::None
                }
            }
        } else {
            self.notify_message
                .set_error(current_labels().no_match_selected.to_string());
            AppAction::None
        }
    }
}
