use std::{path::PathBuf, sync::Arc};

use crate::{
    localization::current_labels,
    providers::{
        match_reader::MatchReader,
        match_writer::MatchWriter,
        set_writer::SetWriter,
        settings_reader::SettingsReader,
        settings_writer::SettingsWriter,
        team_reader::TeamReader,
        team_writer::{PlayerInput, TeamWriter},
    },
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_dialogue::NotifyDialogue,
            team_header::TeamHeader,
        },
        edit_player_screen::EditPlayerScreen,
        edit_team_screen::EditTeamScreen,
        export_team_screen::ExportTeamAction,
        file_system_screen::FileSystemScreen,
        match_list_screen::MatchListScreen,
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
    },
    shapes::{
        enums::ScreenActionEnum, keybinding::KeyBindings, player::PlayerEntry, settings::Settings,
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use crokey::*;
use dirs::home_dir;
use ratatui::widgets::*;
use ratatui::{layout::Alignment, widgets::Table};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    Frame,
};

#[derive(Debug)]
pub struct TeamDetailsScreen<
    TR: TeamReader + Send + Sync + 'static,
    TW: TeamWriter + Send + Sync + 'static,
    MR: MatchReader + Send + Sync + 'static,
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> {
    settings: Settings,
    list_state: ListState,
    team: TeamEntry,
    notifier: NotifyDialogue<PlayerEntry>,
    header: TeamHeader,
    footer: NavigationFooter,
    base_path: PathBuf,
    team_reader: Arc<TR>,
    team_writer: Arc<TW>,
    match_reader: Arc<MR>,
    match_writer: Arc<MW>,
    set_writer: Arc<SSW>,
    settings_reader: Arc<SR>,
    settings_writer: Arc<SW>,
    screen_key_bindings: KeyBindings,
    combiner: Combiner,
}

#[async_trait]
impl<
        TR: TeamReader + Send + Sync + 'static,
        TW: TeamWriter + Send + Sync + 'static,
        MR: MatchReader + Send + Sync + 'static,
        MW: MatchWriter + Send + Sync + 'static,
        SSW: SetWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
        SW: SettingsWriter + Send + Sync + 'static,
    > ScreenAsync for TeamDetailsScreen<TR, TW, MR, MW, SSW, SR, SW>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.combiner.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                &self.notifier.banner.has_value(),
                &self.notifier.has_value(),
            ) {
                //whatever the key event reset the banner when visible
                (_, _, true, false) => {
                    self.notifier.banner.reset();
                    AppAction::None
                }
                //dialog exits (y|n) have higher priority
                (_, KeyCode::Char(x), _, true) => {
                    let selected_player = self.notifier.entry.to_owned();
                    self.notifier.reset();
                    if x == *current_labels().y {
                        match selected_player {
                            Some(player) => {
                                self.remove(
                                    &mut self.team.clone(),
                                    player,
                                    self.team_writer.clone(),
                                )
                                .await
                            }

                            None => return AppAction::None,
                        }
                    } else {
                        AppAction::None
                    }
                }
                (Some(ScreenActionEnum::Quit), _, _, _) => AppAction::Quit(Ok(())),
                (Some(ScreenActionEnum::Next), _, _, _) => {
                    self.next_player();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Previous), _, _, _) => {
                    self.previous_player();
                    AppAction::None
                }
                (Some(ScreenActionEnum::NewPlayer), _, _, _) => {
                    AppAction::SwitchScreen(Box::new(EditPlayerScreen::new(
                        self.settings.clone(),
                        self.team.clone(),
                        self.team_writer.clone(),
                    )))
                }
                (Some(ScreenActionEnum::EditPlayer), _, _, _) => {
                    match self.list_state.selected().map(|selected| {
                        let player = self.team.active_players().get(selected).cloned();
                        match player {
                            Some(p) => AppAction::SwitchScreen(Box::new(EditPlayerScreen::edit(
                                self.settings.clone(),
                                self.team.clone(),
                                p.clone(),
                                self.team_writer.clone(),
                            ))),
                            None => AppAction::None,
                        }
                    }) {
                        Some(action) => action,
                        None => AppAction::None,
                    }
                }
                (Some(ScreenActionEnum::EditTeam), _, _, _) => {
                    AppAction::SwitchScreen(Box::new(EditTeamScreen::edit(
                        self.settings.clone(),
                        &self.team,
                        self.team_writer.clone(),
                    )))
                }
                (Some(ScreenActionEnum::MatchList), _, _, _) => {
                    let match_list = self.match_reader.read_all(&self.team).await;
                    match match_list {
                        Err(e) => {
                            self.notifier.banner.set_error(format!(
                                "{}: {}",
                                current_labels().could_not_load_matches,
                                e
                            ));
                            return AppAction::None;
                        }
                        Ok(ml) => AppAction::SwitchScreen(Box::new(MatchListScreen::new(
                            self.settings.clone(),
                            self.team.clone(),
                            ml,
                            self.base_path.clone(),
                            self.match_reader.clone(),
                            self.match_writer.clone(),
                            self.set_writer.clone(),
                            self.settings_reader.clone(),
                            self.settings_writer.clone(),
                        ))),
                    }
                }
                (Some(ScreenActionEnum::Export), _, _, _) => match home_dir() {
                    Some(path) => AppAction::SwitchScreen(Box::new(FileSystemScreen::new(
                        self.settings.clone(),
                        path,
                        current_labels().export,
                        ExportTeamAction::new(self.team.id, self.base_path.clone()),
                        self.settings_reader.clone(),
                        self.settings_writer.clone(),
                    ))),
                    None => {
                        self.notifier.banner.set_error(
                            current_labels()
                                .could_not_recognize_home_directory
                                .to_string(),
                        );
                        AppAction::None
                    }
                },
                (Some(ScreenActionEnum::Back), _, _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::RemovePlayer), _, _, _) => {
                    match self.list_state.selected().map(async |selected: usize| {
                        match self.team.active_players().get(selected).cloned() {
                            Some(p) => {
                                self.notifier.set(p.to_owned()).banner.set_warning(
                                    current_labels()
                                        .remove_player_confirmation
                                        .to_string()
                                        .replace("{}", p.name.as_str()),
                                );
                                AppAction::None
                            }
                            None => AppAction::None,
                        }
                    }) {
                        Some(action) => action.await,
                        None => AppAction::None,
                    }
                }
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {
        match self.team_reader.read_single(&self.team.id).await {
            Ok(team) => {
                self.team = team;
            }
            Err(e) => {
                self.notifier.banner.set_error(format!(
                    "{}: {}",
                    current_labels().could_not_load_teams,
                    e
                ));
            }
        }
    }
}

impl<
        TR: TeamReader + Send + Sync,
        TW: TeamWriter + Send + Sync,
        MR: MatchReader + Send + Sync,
        MW: MatchWriter + Send + Sync,
        SSW: SetWriter + Send + Sync,
        SR: SettingsReader + Send + Sync,
        SW: SettingsWriter + Send + Sync,
    > Renderable for TeamDetailsScreen<TR, TW, MR, MW, SSW, SR, SW>
{
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        self.notifier.render(f, footer_right);
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);
        self.header.render(f, container[0], Some(&self.team));
        let selected_player = match self.list_state.selected() {
            None => {
                self.list_state.select(Some(0));
                0
            }
            Some(p) => p,
        };
        let table = Table::new(
            self.get_rows(selected_player),
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
        if self.team.active_players().is_empty() {
            self.render_no_players_yet(f, container[1]);
        } else {
            f.render_widget(table, container[1]);
        }

        let screen_actions = &self.get_footer_entries();

        self.footer.render(
            f,
            footer_left,
            get_keybinding_actions(&self.settings.keybindings, Sba::Simple(screen_actions)),
        );
        self.screen_key_bindings = self.settings.keybindings.slice(screen_actions.to_owned());
    }
}

impl<
        TR: TeamReader + Send + Sync,
        TW: TeamWriter + Send + Sync,
        MR: MatchReader + Send + Sync,
        MW: MatchWriter + Send + Sync,
        SSW: SetWriter + Send + Sync,
        SR: SettingsReader + Send + Sync,
        SW: SettingsWriter + Send + Sync,
    > TeamDetailsScreen<TR, TW, MR, MW, SSW, SR, SW>
{
    pub fn new(
        settings: Settings,
        team: &TeamEntry,
        base_path: PathBuf,
        team_reader: Arc<TR>,
        team_writer: Arc<TW>,
        match_reader: Arc<MR>,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
    ) -> Self {
        let header = TeamHeader::default();
        TeamDetailsScreen {
            settings,
            team: team.clone(),
            list_state: ListState::default(),
            header,
            footer: NavigationFooter::new(),
            base_path,
            team_reader,
            team_writer,
            match_reader,
            match_writer,
            set_writer,
            settings_reader,
            settings_writer,
            notifier: NotifyDialogue::new(),
            combiner: Combiner::default(),
            screen_key_bindings: KeyBindings::empty(),
        }
    }

    fn next_player(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = (selected + 1).min(self.team.active_players().len() - 1);
            self.list_state.select(Some(new_selected));
        }
    }

    fn previous_player(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = if selected == 0 { 0 } else { selected - 1 };
            self.list_state.select(Some(new_selected));
        }
    }

    fn get_footer_entries(&self) -> Vec<&ScreenActionEnum> {
        let base_screen_actions = &mut vec![
            &ScreenActionEnum::NewPlayer,
            &ScreenActionEnum::MatchList,
            &ScreenActionEnum::Back,
            &ScreenActionEnum::Quit,
        ];
        if self.team.active_players().is_empty() {
            base_screen_actions.clone()
        } else {
            let scren_actions = &mut vec![
                &ScreenActionEnum::Previous,
                &ScreenActionEnum::Next,
                &ScreenActionEnum::EditPlayer,
                &ScreenActionEnum::RemovePlayer,
            ];
            scren_actions.append(base_screen_actions);
            scren_actions.clone()
        }
    }

    async fn remove(
        &mut self,
        team: &mut TeamEntry,
        player: PlayerEntry,
        team_writer: Arc<TW>,
    ) -> AppAction {
        let mut input = player.clone();
        input.deleted = true;
        let input = PlayerInput::Existing(input);
        let player = team_writer.save_player(input, team).await;
        match player {
            Ok(_) => {
                self.notifier
                    .banner
                    .set_info(current_labels().operation_successful.to_string());
                self.refresh_data().await;
                AppAction::None
            }
            Err(_) => {
                self.notifier
                    .banner
                    .set_error(current_labels().could_not_remove_player.to_string());
                AppAction::None
            }
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

    fn get_rows(&self, selected_player: usize) -> Vec<Row<'_>> {
        self.team
            .active_players()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut row = Row::new(vec![
                    p.number.to_string(),
                    p.name.clone(),
                    p.role.map_or_else(|| "-".to_string(), |r| r.to_string()),
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
            .collect()
    }
}
