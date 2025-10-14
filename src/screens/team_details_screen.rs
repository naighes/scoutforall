use std::{path::PathBuf, sync::Arc};

use crate::{
    localization::current_labels,
    providers::{
        match_reader::MatchReader,
        match_writer::MatchWriter,
        set_writer::SetWriter,
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
        screen::{AppAction, Renderable, ScreenAsync},
    },
    shapes::{player::PlayerEntry, team::TeamEntry},
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
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
> {
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
}

#[async_trait]
impl<
        TR: TeamReader + Send + Sync + 'static,
        TW: TeamWriter + Send + Sync + 'static,
        MR: MatchReader + Send + Sync + 'static,
        MW: MatchWriter + Send + Sync + 'static,
        SSW: SetWriter + Send + Sync + 'static,
    > ScreenAsync for TeamDetailsScreen<TR, TW, MR, MW, SSW>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (
            key.code,
            &self.notifier.banner.has_value(),
            &self.notifier.has_value(),
        ) {
            (_, true, false) => {
                self.notifier.banner.reset();
                AppAction::None
            }
            (KeyCode::Down, _, _) => {
                self.next_player();
                AppAction::None
            }
            (KeyCode::Up, _, _) => {
                self.previous_player();
                AppAction::None
            }
            (KeyCode::Char(x), _, true) => {
                let selected_player = self.notifier.entry.to_owned();
                self.notifier.reset();
                if x == *current_labels().y {
                    match selected_player {
                        Some(player) => {
                            self.remove(&mut self.team.clone(), player, self.team_writer.clone())
                                .await
                        }

                        None => return AppAction::None,
                    }
                } else {
                    AppAction::None
                }
            }
            (KeyCode::Char('n'), _, _) => AppAction::SwitchScreen(Box::new(EditPlayerScreen::new(
                self.team.clone(),
                self.team_writer.clone(),
            ))),
            (KeyCode::Char('m'), _, _) => {
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
                        self.team.clone(),
                        ml,
                        self.base_path.clone(),
                        self.match_reader.clone(),
                        self.match_writer.clone(),
                        self.set_writer.clone(),
                    ))),
                }
            }
            (KeyCode::Char('e'), _, _) => AppAction::SwitchScreen(Box::new(EditTeamScreen::edit(
                &self.team,
                self.team_writer.clone(),
            ))),
            (KeyCode::Char('s'), _, _) => match home_dir() {
                Some(path) => AppAction::SwitchScreen(Box::new(FileSystemScreen::new(
                    path,
                    current_labels().export,
                    ExportTeamAction::new(self.team.id, self.base_path.clone()),
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
            (KeyCode::Esc, _, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _, _) => {
                match self.list_state.selected().map(|selected| {
                    let player = self.team.active_players().get(selected).cloned();
                    match player {
                        Some(p) => AppAction::SwitchScreen(Box::new(EditPlayerScreen::edit(
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
            (KeyCode::Char('r'), _, _) => {
                match self.list_state.selected().map(async |selected: usize| {
                    let u = self.team.active_players().get(selected).cloned();
                    let player = async move { u };
                    match player.await {
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
    > Renderable for TeamDetailsScreen<TR, TW, MR, MW, SSW>
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
        self.footer
            .render(f, footer_left, self.get_footer_entries());
    }
}

impl<
        TR: TeamReader + Send + Sync,
        TW: TeamWriter + Send + Sync,
        MR: MatchReader + Send + Sync,
        MW: MatchWriter + Send + Sync,
        SSW: SetWriter + Send + Sync,
    > TeamDetailsScreen<TR, TW, MR, MW, SSW>
{
    pub fn new(
        team: &TeamEntry,
        base_path: PathBuf,
        team_reader: Arc<TR>,
        team_writer: Arc<TW>,
        match_reader: Arc<MR>,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
    ) -> Self {
        let header = TeamHeader::default();
        TeamDetailsScreen {
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
            notifier: NotifyDialogue::new(),
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

    fn get_footer_entries(&self) -> Vec<(String, String)> {
        if self.team.active_players().is_empty() {
            vec![
                ("E".to_string(), current_labels().edit_team.to_string()),
                ("N".to_string(), current_labels().new_player.to_string()),
                ("M".to_string(), current_labels().match_list.to_string()),
                ("Esc".to_string(), current_labels().back.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ]
        } else {
            vec![
                ("↑↓".to_string(), current_labels().navigate.to_string()),
                (
                    current_labels().enter.to_string(),
                    current_labels().edit_player.to_string(),
                ),
                ("R".to_string(), current_labels().remove_player.to_string()),
                ("E".to_string(), current_labels().edit_team.to_string()),
                ("N".to_string(), current_labels().new_player.to_string()),
                ("M".to_string(), current_labels().match_list.to_string()),
                ("S".to_string(), current_labels().export.to_string()),
                ("Esc".to_string(), current_labels().back.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ]
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
        self.refresh_data().await;
        match player {
            Ok(_) => {
                self.notifier
                    .banner
                    .set_info(current_labels().operation_successful.to_string());
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
