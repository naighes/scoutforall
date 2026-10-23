use crate::shapes::keybinding::navigation_keybindings;
use std::sync::Arc;

use crate::localization::current_labels;
use crate::pdf_viewer::open_match_pdf;
use crate::{
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        add_match_screen::AddMatchScreen,
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel,
            team_header::TeamHeader,
        },
        export_match_screen::ExportMatchAction,
        file_system_screen::FileSystemScreen,
        import_match_screen::ImportMatchAction,
        match_stats_screen::MatchStatsScreen,
        scouting_screen::ScoutingScreen,
        screen::{AppAction, LIST_NAVIGATION, Renderable, Sba, ScreenAsync, list_entries, step},
        set_list_screen::SetListScreen,
        start_set_screen::StartSetScreen,
    },
    shapes::{
        actions::ScreenActionEnum,
        keybinding::{ActionsKeyBindings, KeyBindings},
        settings::Settings,
    },
};
use async_trait::async_trait;
use crokey::crossterm::event::KeyEvent;
use dirs::home_dir;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::ListState,
};
use ratatui::{
    text::Span,
    widgets::{Row, Table},
};
use scout4all_core::{
    errors::AppError,
    providers::{match_reader::MatchReader, match_writer::MatchWriter, set_writer::SetWriter},
    set_start::{self, NextSet, SetStartForm, next_set},
    shapes::{
        listed::{Listed, Unreadable},
        r#match::{MatchEntry, MatchStatus},
        set::SetEntry,
        team::TeamEntry,
    },
    storage::Store,
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
    matches: Vec<Listed<(MatchEntry, MatchStatus)>>,
    notify_message: NotifyBanner,
    header: TeamHeader,
    footer: NavigationFooter,
    store: Arc<dyn Store>,
    match_reader: Arc<MR>,
    match_writer: Arc<MW>,
    set_writer: Arc<SSW>,
    settings_reader: Arc<SR>,
    settings_writer: Arc<SW>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

/// A team in the match list, on a wide screen.
const MATCH_TEAM_MAX_WIDTH: u16 = 30;
/// The sets won, as `3-1`.
const MATCH_SCORE_WIDTH: u16 = 3;
/// The date of the match, as `Mon Sep 01, 2025`.
const MATCH_DATE_FORMAT: &str = "%a %b %d, %Y";
const MATCH_DATE_WIDTH: u16 = 16;

impl<
    MR: MatchReader + Send + Sync + 'static,
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> Renderable for MatchListScreen<MR, MW, SSW, SR, SW>
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        list_entries(navigation_keybindings(), &self.screen_actions())
    }

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
            let table = Table::new(rows, Self::column_widths())
                .block(panel::screen(current_labels().match_list));
            if self.matches.is_empty() {
                panel::render_empty(f, container[1], current_labels().no_matches_yet);
            } else {
                f.render_widget(table, container[1]);
            }
        } else {
            self.notify_message
                .set_error(current_labels().could_not_render_match_list.to_string());
        }
        self.header.render(f, container[0], Some(&self.team));
        self.notify_message.render(f, footer_right);
        let kb: &KeyBindings<ScreenActionEnum> = navigation_keybindings();
        let screen_actions = &self.screen_actions();
        let screen_key_bindings = &kb.slice(Sba::keys(screen_actions));
        self.footer.render(f, footer_left, self.footer_entries());
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
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                &self.notify_message.has_value(),
            ) {
                (_, _, true) => {
                    self.notify_message.reset();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Next | ScreenActionEnum::Down), _, _) => self.next_match(),
                (Some(ScreenActionEnum::Previous | ScreenActionEnum::Up), _, _) => {
                    self.previous_match()
                }
                (Some(ScreenActionEnum::Select), _, _) => self.handle_enter_key(),
                (Some(ScreenActionEnum::PrintReport), _, _) => self.handle_print(),
                (Some(ScreenActionEnum::MatchStats), _, _) => self.handle_space_key(),
                (Some(ScreenActionEnum::Back), _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::Quit), _, _) => AppAction::Quit(Ok(())),
                (Some(ScreenActionEnum::Edit), _, _) => self.handle_edit_key(),
                (Some(ScreenActionEnum::EditEvents), _, _) => self.handle_edit_events_key(),
                (Some(ScreenActionEnum::New), _, _) => {
                    if self.team.players.len() >= 6 {
                        AppAction::SwitchScreen(Box::new(AddMatchScreen::new(
                            self.settings_reader.clone(),
                            self.settings_writer.clone(),
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
                    // unreadable matches too: their files are what can be
                    // sent to get them fixed
                    let selected_match = self.get_selected_match_id().map(str::to_owned);
                    match (home_dir(), selected_match) {
                        (Some(path), Some(match_id)) => {
                            AppAction::SwitchScreen(Box::new(FileSystemScreen::new(
                                path,
                                current_labels().export,
                                ExportMatchAction::new(
                                    self.team.clone(),
                                    match_id,
                                    Arc::clone(&self.store),
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
                self.matches = with_status(matches);
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
        matches: Vec<Listed<MatchEntry>>,
        store: Arc<dyn Store>,
        match_reader: Arc<MR>,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
    ) -> Self {
        MatchListScreen {
            settings,
            matches: with_status(matches),
            team,
            list_state: ListState::default(),
            store,
            notify_message: NotifyBanner::new(),
            header: TeamHeader::default(),
            footer: NavigationFooter::new(),
            match_reader,
            match_writer,
            set_writer,
            settings_reader,
            settings_writer,
            screen_key_bindings: ActionsKeyBindings::empty(),
        }
    }
    fn get_selected(&self) -> Option<&Listed<(MatchEntry, MatchStatus)>> {
        self.list_state.selected().and_then(|i| self.matches.get(i))
    }

    fn get_selected_match(&self) -> Option<(&MatchEntry, &MatchStatus)> {
        match self.get_selected() {
            Some(Ok((m, s))) => Some((m, s)),
            _ => None,
        }
    }

    /// The folder of the selected match, even when it's unreadable.
    fn get_selected_match_id(&self) -> Option<&str> {
        self.get_selected().map(|m| match m {
            Ok((m, _)) => m.id.as_str(),
            Err(unreadable) => unreadable.id.as_str(),
        })
    }

    /// The teams share the width the score, the date and the status leave,
    /// up to `MATCH_TEAM_MAX_WIDTH` each: these take what their text takes.
    fn column_widths() -> [Constraint; 5] {
        let labels = current_labels();
        let status = [labels.in_progress, labels.unreadable]
            .iter()
            .map(|label| Span::raw(*label).width())
            .max()
            .unwrap_or(0) as u16;
        [
            Constraint::Max(MATCH_TEAM_MAX_WIDTH),
            Constraint::Max(MATCH_TEAM_MAX_WIDTH),
            Constraint::Length(MATCH_SCORE_WIDTH),
            Constraint::Length(MATCH_DATE_WIDTH),
            Constraint::Length(status),
        ]
    }

    fn get_match_row(
        &self,
        m: &Listed<(MatchEntry, MatchStatus)>,
        row_index: usize,
        match_index: usize,
    ) -> Result<Row<'_>, AppError> {
        let selected_style = |style: Style| {
            if row_index == match_index {
                style.add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                style
            }
        };
        let (m, status) = match m {
            Ok(m) => m,
            Err(unreadable) => {
                // the folder in the wider column
                return Ok(Row::new(vec![
                    String::new(),
                    unreadable.id.clone(),
                    String::new(),
                    String::new(),
                    current_labels().unreadable.to_string(),
                ])
                .style(selected_style(Style::default().fg(Color::Yellow))));
            }
        };
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
            m.date.format(MATCH_DATE_FORMAT).to_string(),
            (if status.match_finished {
                ""
            } else {
                current_labels().in_progress
            })
            .into(),
        ]);
        let style = match (status.match_finished, status.us_wins, status.them_wins) {
            (true, 3, _) => Style::default().fg(Color::LightGreen),
            (true, _, 3) => Style::default().fg(Color::Red),
            _ => Style::default().fg(Color::White),
        };
        row = row.style(selected_style(style));
        Ok(row)
    }

    fn new_set(&mut self, m: &MatchEntry, form: SetStartForm) -> AppAction {
        AppAction::SwitchScreen(Box::new(StartSetScreen::new(
            self.settings_reader.clone(),
            self.settings_writer.clone(),
            self.settings.clone(),
            m.clone(),
            form,
            Some(1),
            self.set_writer.clone(),
        )))
    }

    fn continue_set(&mut self, m: &MatchEntry, last_incomplete_set: SetEntry) -> AppAction {
        match set_start::resume(m.clone(), last_incomplete_set, self.set_writer.clone()) {
            Ok(session) => AppAction::SwitchScreen(Box::new(ScoutingScreen::new(
                self.settings_reader.clone(),
                self.settings_writer.clone(),
                self.settings.clone(),
                session,
                Some(1),
            ))),
            Err(e) => {
                self.notify_message
                    .set_error(e.message(current_labels().domain));
                AppAction::None
            }
        }
    }

    fn next_match(&mut self) -> AppAction {
        self.move_selection(true)
    }

    fn previous_match(&mut self) -> AppAction {
        self.move_selection(false)
    }

    fn move_selection(&mut self, forward: bool) -> AppAction {
        if let Some(selected) = self.list_state.selected() {
            self.list_state
                .select(step(selected, self.matches.len(), forward));
        }
        AppAction::None
    }

    fn screen_actions(&self) -> Vec<Sba<ScreenActionEnum>> {
        let mut actions = Vec::new();
        actions.push(Sba::Redacted(ScreenActionEnum::Import, |lbl| -> String {
            lbl.replace("{}", current_labels().match_word)
        }));

        if !self.matches.is_empty() {
            actions.extend(LIST_NAVIGATION);
        }
        match self.get_selected() {
            Some(Ok((m, status))) => {
                if !status.match_finished {
                    actions.push(Sba::Simple(ScreenActionEnum::Select));
                }
                actions.push(Sba::Simple(ScreenActionEnum::Edit));
                if !m.sets.is_empty() {
                    actions.push(Sba::Simple(ScreenActionEnum::EditEvents));
                }
                actions.push(Sba::Simple(ScreenActionEnum::Export));
                actions.push(Sba::Simple(ScreenActionEnum::MatchStats));
                actions.push(Sba::Simple(ScreenActionEnum::PrintReport));
            }
            Some(Err(_)) => actions.push(Sba::Simple(ScreenActionEnum::Export)),
            None => {}
        }
        if self.team.players.len() >= 6 {
            actions.push(Sba::Simple(ScreenActionEnum::New));
        };
        actions.push(Sba::Simple(ScreenActionEnum::Back));
        actions.push(Sba::Simple(ScreenActionEnum::Quit));
        actions
    }

    fn handle_space_key(&mut self) -> AppAction {
        match self.get_selected_match() {
            Some((m, _)) => match MatchStatsScreen::new(m.clone()) {
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
            match next_set(status) {
                NextSet::Start(form) => self.new_set(&match_entry, form),
                NextSet::Resume(set) => self.continue_set(&match_entry, set),
                NextSet::Over => AppAction::None,
            }
        } else {
            self.notify_message
                .set_error(current_labels().no_match_selected.to_string());
            AppAction::None
        }
    }

    fn handle_edit_key(&mut self) -> AppAction {
        match self.get_selected_match().map(|(m, _)| m.clone()) {
            Some(m) => AppAction::SwitchScreen(Box::new(AddMatchScreen::new_edit(
                self.settings_reader.clone(),
                self.settings_writer.clone(),
                self.settings.clone(),
                self.match_writer.clone(),
                self.set_writer.clone(),
                m,
            ))),
            None => {
                self.notify_message
                    .set_error(current_labels().no_match_selected.to_string());
                AppAction::None
            }
        }
    }

    fn handle_edit_events_key(&mut self) -> AppAction {
        match self.get_selected_match().map(|(m, _)| m.clone()) {
            Some(m) => AppAction::SwitchScreen(Box::new(SetListScreen::new(
                m,
                self.match_reader.clone(),
                self.set_writer.clone(),
            ))),
            None => {
                self.notify_message
                    .set_error(current_labels().no_match_selected.to_string());
                AppAction::None
            }
        }
    }

    fn handle_print(&mut self) -> AppAction {
        let selected = self.get_selected_match().map(|(m, s)| (m.clone(), s));
        if let Some((match_entry, _)) = selected {
            match open_match_pdf(&match_entry, current_labels().domain) {
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

/// The matches with their status: a match whose sets don't replay is
/// unreadable.
fn with_status(matches: Vec<Listed<MatchEntry>>) -> Vec<Listed<(MatchEntry, MatchStatus)>> {
    matches
        .into_iter()
        .map(|m| {
            m.and_then(|m| match m.get_status() {
                Ok(status) => Ok((m, status)),
                Err(_) => Err(Unreadable { id: m.id }),
            })
        })
        .collect()
}
