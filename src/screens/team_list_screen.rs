use crate::shapes::keybinding::navigation_keybindings;
use std::{fmt::Debug, sync::Arc};

use crate::localization::current_labels;
use crate::{
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel},
        edit_team_screen::EditTeamScreen,
        file_system_screen::FileSystemScreen,
        import_team_screen::ImportTeamAction,
        screen::{AppAction, LIST_NAVIGATION, Renderable, Sba, ScreenAsync, list_entries, step},
        settings_screen::{SettingsPanel, SettingsScreen},
        team_details_screen::TeamDetailsScreen,
    },
    shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings, settings::Settings},
};
use async_trait::async_trait;
use crokey::crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{List, ListItem, ListState},
};
use scout4all_core::{
    providers::{
        match_reader::MatchReader, match_writer::MatchWriter, set_writer::SetWriter,
        team_reader::TeamReader, team_writer::TeamWriter,
    },
    shapes::{enums::FriendlyName, listed::Listed, team::TeamEntry},
    storage::Store,
};

#[derive(Debug)]
pub struct TeamListScreen<
    TR: TeamReader + Send + Sync,
    TW: TeamWriter + Send + Sync,
    SW: SettingsWriter + Send + Sync,
    MR: MatchReader + Send + Sync,
    MW: MatchWriter + Send + Sync,
    SSW: SetWriter + Send + Sync,
    SR: SettingsReader + Send + Sync,
> {
    list_state: ListState,
    teams: Vec<Listed<TeamEntry>>,
    settings: Settings,
    notify_message: NotifyBanner,
    footer: NavigationFooter,
    store: Arc<dyn Store>,
    team_reader: Arc<TR>,
    team_writer: Arc<TW>,
    settings_writer: Arc<SW>,
    match_reader: Arc<MR>,
    match_writer: Arc<MW>,
    set_writer: Arc<SSW>,
    settings_reader: Arc<SR>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

#[async_trait]
impl<
    TR: TeamReader + Send + Sync + 'static,
    TW: TeamWriter + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
    MR: MatchReader + Send + Sync + 'static,
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
> ScreenAsync for TeamListScreen<TR, TW, SW, MR, MW, SSW, SR>
{
    async fn refresh_data(&mut self) {
        if let Ok(s) = self.settings_reader.read().await {
            self.settings = s;
        }
        match self.team_reader.read_all().await {
            Ok(teams) => {
                self.teams = teams;
                if self.teams.is_empty() {
                    self.list_state.select(None);
                } else if let Some(selected) = self.list_state.selected() {
                    if selected >= self.teams.len() {
                        self.list_state.select(Some(self.teams.len() - 1));
                    }
                } else {
                    self.list_state.select(Some(0));
                }
            }
            Err(_) => {
                self.notify_message
                    .set_error(current_labels().could_not_load_teams.to_string());
                self.teams = vec![];
            }
        }
    }

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
                (Some(ScreenActionEnum::Next | ScreenActionEnum::Down), _, _) => {
                    self.next_team();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Previous | ScreenActionEnum::Up), _, _) => {
                    self.previous_team();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Select), _, _) => match self.get_selected_team() {
                    None => AppAction::None,
                    Some(team) => AppAction::SwitchScreen(Box::new(TeamDetailsScreen::new(
                        self.settings.clone(),
                        team,
                        Arc::clone(&self.store),
                        self.team_reader.clone(),
                        self.team_writer.clone(),
                        self.match_reader.clone(),
                        self.match_writer.clone(),
                        self.set_writer.clone(),
                        self.settings_reader.clone(),
                        self.settings_writer.clone(),
                    ))),
                },
                (Some(ScreenActionEnum::Back), _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::New), _, _) => {
                    AppAction::SwitchScreen(Box::new(EditTeamScreen::new(self.team_writer.clone())))
                }
                (Some(ScreenActionEnum::Import), _, _) => {
                    let default_path = self.settings.get_default_path();
                    match default_path {
                        Some(path) => AppAction::SwitchScreen(Box::new(FileSystemScreen::new(
                            path,
                            current_labels().import_team,
                            ImportTeamAction::new(
                                self.team_reader.clone(),
                                self.team_writer.clone(),
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
                    }
                }
                (Some(ScreenActionEnum::Settings), _, _) => {
                    AppAction::SwitchScreen(Box::new(SettingsScreen::new(
                        self.settings.clone(),
                        self.settings_writer.clone(),
                        self.settings_reader.clone(),
                        SettingsPanel::General,
                    )))
                }
                (Some(ScreenActionEnum::Quit), _, _) => AppAction::Quit(Ok(())),
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }
}

impl<
    TR: TeamReader + Send + Sync + 'static,
    TW: TeamWriter + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
    MR: MatchReader + Send + Sync + 'static,
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
> Renderable for TeamListScreen<TR, TW, SW, MR, MW, SSW, SR>
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        list_entries(navigation_keybindings(), &self.screen_actions())
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        self.notify_message.render(f, footer_right);
        let items: Vec<ListItem> = self
            .teams
            .iter()
            .map(|t| match t {
                Ok(t) => {
                    let labels = current_labels();
                    let details = [
                        t.classification.map(|c| c.friendly_name(labels.domain)),
                        t.gender.map(|g| g.friendly_name(labels.domain)),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("/");
                    ListItem::new(match details.as_str() {
                        "" => format!("{} ({})", t.name, t.year),
                        _ => format!("{} ({}, {})", t.name, details, t.year),
                    })
                }
                Err(unreadable) => ListItem::new(format!(
                    "{} ({})",
                    unreadable.id,
                    current_labels().unreadable
                ))
                .style(Style::default().fg(Color::Yellow)),
            })
            .collect();
        if items.is_empty() {
            panel::render_empty(f, body, current_labels().no_teams_yet);
        } else {
            self.render_list(f, body, items);
        }
        let screen_actions = &self.screen_actions();
        self.footer.render(f, footer_left, self.footer_entries());

        self.screen_key_bindings = navigation_keybindings().slice(Sba::keys(screen_actions));
    }
}

impl<
    TR: TeamReader + Send + Sync + 'static,
    TW: TeamWriter + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
    MR: MatchReader + Send + Sync + 'static,
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
> TeamListScreen<TR, TW, SW, MR, MW, SSW, SR>
{
    pub fn new(
        settings: Settings,
        teams: Vec<Listed<TeamEntry>>,
        store: Arc<dyn Store>,
        team_reader: Arc<TR>,
        team_writer: Arc<TW>,
        settings_writer: Arc<SW>,
        match_reader: Arc<MR>,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
        settings_reader: Arc<SR>,
    ) -> Self {
        TeamListScreen {
            teams,
            list_state: ListState::default(),
            notify_message: NotifyBanner::new(),
            settings: settings.clone(),
            footer: NavigationFooter::new(),
            store,
            team_reader,
            team_writer,
            settings_writer,
            match_reader,
            match_writer,
            set_writer,
            settings_reader,
            screen_key_bindings: ActionsKeyBindings::empty(),
        }
    }

    fn get_selected_team(&self) -> Option<&TeamEntry> {
        match self.list_state.selected().and_then(|x| self.teams.get(x)) {
            Some(Ok(team)) => Some(team),
            _ => None,
        }
    }

    fn next_team(&mut self) {
        self.move_selection(true);
    }

    fn previous_team(&mut self) {
        self.move_selection(false);
    }

    fn move_selection(&mut self, forward: bool) {
        if let Some(selected) = self.list_state.selected() {
            self.list_state
                .select(step(selected, self.teams.len(), forward));
        }
    }

    fn screen_actions(&self) -> Vec<Sba<ScreenActionEnum>> {
        if self.teams.is_empty() {
            vec![
                Sba::Simple(ScreenActionEnum::New),
                Sba::Simple(ScreenActionEnum::Settings),
                Sba::Redacted(ScreenActionEnum::Import, |lbl| -> String {
                    lbl.replace("{}", current_labels().team)
                }),
                Sba::Simple(ScreenActionEnum::Quit),
            ]
        } else {
            LIST_NAVIGATION
                .into_iter()
                .chain([
                    Sba::Simple(ScreenActionEnum::New),
                    if self.get_selected_team().is_some() {
                        Sba::Simple(ScreenActionEnum::Select)
                    } else {
                        Sba::Hidden(ScreenActionEnum::Select)
                    },
                    Sba::Simple(ScreenActionEnum::Settings),
                    Sba::Redacted(ScreenActionEnum::Import, |lbl| -> String {
                        lbl.replace("{}", current_labels().team)
                    }),
                    Sba::Simple(ScreenActionEnum::Quit),
                ])
                .collect()
        }
    }

    fn render_list(&mut self, f: &mut Frame, area: Rect, items: Vec<ListItem>) {
        if self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
        let list = List::new(items)
            .block(panel::screen(current_labels().teams))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }
}
