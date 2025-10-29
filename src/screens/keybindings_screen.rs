use std::sync::Arc;

use crate::{
    localization::current_labels,
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{
            checkbox::CheckBox, navigation_footer::NavigationFooter,
            notify_dialogue::NotifyDialogue, select::Select,
        },
        keybindings_action_screen::KeyBindingActionScreen,
        report_an_issue_screen::ReportAnIssueScreen,
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
    },
    shapes::{
        enums::{LanguageEnum, ScreenActionEnum},
        keybinding::KeyBindings,
        settings::{set_settings, Settings},
    },
};
use async_trait::async_trait;
use crokey::{
    crossterm::{self, event::KeyCode},
    Combiner, KeyCombinationFormat,
};
use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, ListState, Row, Table},
    Frame,
};

#[derive(Debug)]
pub struct KeybindingScreen<SW: SettingsWriter + Send + Sync, SR: SettingsReader + Send + Sync> {
    language: Select<LanguageEnum>,
    analytics_enabled: CheckBox,
    key_binding_selection: ListState,
    notifier: NotifyDialogue<Settings>,
    back: bool,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    settings_writer: Arc<SW>,
    settings_reader: Arc<SR>,
    settings: Settings,
    screen_key_bindings: KeyBindings,
    format: KeyCombinationFormat,
    combiner: Combiner,
}

impl<SW: SettingsWriter + Send + Sync + 'static, SR: SettingsReader + Send + Sync + 'static>
    Renderable for KeybindingScreen<SW, SR>
{
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2),       // language
                Constraint::Length(2),       // analytics checkbox
                Constraint::Percentage(100), // keybindings
                Constraint::Min(1),
            ])
            .split(body);
        self.language.render(f, inner[0]);
        self.analytics_enabled.render(f, inner[1]);
        self.render_key_bindings_list(f, inner[2]);
        self.notifier.render(f, footer_right);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<SW: SettingsWriter + Send + Sync + 'static, SR: SettingsReader + Send + Sync + 'static>
    ScreenAsync for KeybindingScreen<SW, SR>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.combiner.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                &self.notifier.banner.has_value(),
                &self.notifier.has_value(),
            ) {
                (_, KeyCode::Char(x), _, true) => {
                    self.notifier.banner.reset();
                    let default_settings = self.notifier.entry.to_owned();
                    self.notifier.reset();
                    if x == *current_labels().y {
                        match default_settings {
                            Some(settings) => self.reset_settings(settings).await,

                            None => return AppAction::None,
                        }
                    } else {
                        AppAction::None
                    }
                }
                (_, _, true, _) => {
                    self.notifier.reset();
                    if self.back {
                        AppAction::Back(true, Some(1))
                    } else {
                        AppAction::None
                    }
                }
                (Some(ScreenActionEnum::Previous), _, _, _) => self.handle_backtab(),
                (Some(ScreenActionEnum::Next), _, _, _) => self.handle_tab(),
                (Some(ScreenActionEnum::ReportAnIssue), _, _, _) => AppAction::SwitchScreen(
                    Box::new(ReportAnIssueScreen::new(self.settings.clone())),
                ),
                (Some(ScreenActionEnum::Edit), _, _, _) => {
                    match self.key_binding_selection.selected().map(|selected| {
                        let action = ScreenActionEnum::ALL.get(selected);
                        match action {
                            Some(p) => {
                                let keybindings = self.settings.keybindings.keybindings_for(p);
                                AppAction::SwitchScreen(Box::new(KeyBindingActionScreen::new(
                                    self.settings.clone(),
                                    p.to_owned(),
                                    keybindings,
                                    self.format.clone(),
                                    self.settings_writer.clone(),
                                    self.settings_reader.clone(),
                                )))
                            }
                            None => AppAction::None,
                        }
                    }) {
                        Some(action) => action,
                        None => AppAction::None,
                    }
                }
                (Some(ScreenActionEnum::Reset), _, _, _) => {
                    let settings = Settings {
                        language: self.settings.language,
                        analytics_enabled: self.settings.analytics_enabled,
                        keybindings: KeyBindings::default(),
                        last_used_dir: self.settings.last_used_dir.to_owned(),
                    };
                    self.notifier
                        .set(settings.to_owned())
                        .banner
                        .set_warning(current_labels().reset_to_defaults_confirmation.to_string());
                    AppAction::None
                }
                (Some(ScreenActionEnum::Confirm), _, _, _) => self.handle_enter().await,
                (Some(ScreenActionEnum::Back), _, _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::Quit), _, _, _) => AppAction::Quit(Ok(())),
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {
        if let Ok(settings) = &self.settings_reader.read().await {
            self.settings = settings.to_owned();
            let (footer_entries, screen_key_bindings) = get_context_menu(settings);
            self.footer_entries = footer_entries;
            self.screen_key_bindings = screen_key_bindings;
        }
    }
}

impl<SW: SettingsWriter + Send + Sync + 'static, SR: SettingsReader + Send + Sync + 'static>
    KeybindingScreen<SW, SR>
{
    pub fn new(settings: Settings, settings_writer: Arc<SW>, settings_reader: Arc<SR>) -> Self {
        let language = Select::new(
            current_labels().language.to_owned(),
            LanguageEnum::ALL.to_vec(),
            Some(settings.language),
            false,
        );
        let analytics_enabled = CheckBox::new(
            current_labels().enable_send_analytics.to_owned(),
            false,
            settings.analytics_enabled,
        );
        let mut key_binding_selection = ListState::default();
        let index = ScreenActionEnum::ALL
            .iter()
            .position(|&r| r == ScreenActionEnum::Back);
        key_binding_selection.select(index);

        let (footer_entries, screen_key_bindings) = get_context_menu(&settings);

        KeybindingScreen {
            language,
            analytics_enabled,
            key_binding_selection,
            notifier: NotifyDialogue::new(),
            back: false,
            footer: NavigationFooter::new(),
            footer_entries,
            settings_writer,
            settings_reader,
            settings,
            screen_key_bindings,
            format: KeyCombinationFormat::default(),
            combiner: Combiner::default(),
        }
    }

    async fn handle_enter(&mut self) -> AppAction {
        match (
            self.language.get_selected_value(),
            self.analytics_enabled.get_selected_value(),
        ) {
            (Some(language), analytics_enabled) => {
                let settings = Settings {
                    language,
                    analytics_enabled,
                    keybindings: self.settings.keybindings.clone(),
                    last_used_dir: self.settings.last_used_dir.clone(),
                };
                match self.settings_writer.save(settings).await {
                    Ok(saved_settings) => {
                        set_settings(saved_settings);
                        self.notifier
                            .banner
                            .set_info(current_labels().operation_successful.to_string());
                        self.back = true;
                        AppAction::None
                    }
                    Err(_) => {
                        self.notifier
                            .banner
                            .set_error(current_labels().could_not_save_settings.to_string());
                        AppAction::None
                    }
                }
            }
            _ => {
                self.notifier
                    .banner
                    .set_error(current_labels().language_is_required.to_string());
                AppAction::None
            }
        }
    }

    fn handle_tab(&mut self) -> AppAction {
        if let Some(selected) = self.key_binding_selection.selected() {
            let next = if selected >= ScreenActionEnum::ALL.len() - 1 {
                0
            } else {
                selected + 1
            };
            self.key_binding_selection.select(Some(next));
        } else {
            self.key_binding_selection.select(Some(0));
        }
        AppAction::None
    }

    fn handle_backtab(&mut self) -> AppAction {
        if let Some(selected) = self.key_binding_selection.selected() {
            let prev = if selected == 0 {
                ScreenActionEnum::ALL.len() - 1
            } else {
                selected - 1
            };
            self.key_binding_selection.select(Some(prev));
        } else {
            self.key_binding_selection.select(Some(0));
        }
        AppAction::None
    }

    fn get_rows(&self, selected_action: usize) -> Vec<Row<'_>> {
        let actions_bindings_map = self.settings.keybindings.reverse_map();
        let items = ScreenActionEnum::ALL
            .iter()
            .map(|r| {
                let s = actions_bindings_map
                    .get(r)
                    .map(|f| {
                        f.iter()
                            .map(|q| self.format.to_string(*q))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or(current_labels().unassigned.to_string())
                    .to_string();
                (r.with_desc().1, s)
            })
            .enumerate()
            .map(|(index, (action, keybindings))| {
                let mut row = Row::new(vec![action, keybindings]);
                if index == selected_action {
                    row = row.style(
                        Style::default()
                            .add_modifier(Modifier::REVERSED)
                            .add_modifier(Modifier::BOLD),
                    );
                }
                row
            })
            .collect();
        items
    }

    fn render_key_bindings_list(&mut self, f: &mut Frame, area: Rect) {
        let selected_action = match self.key_binding_selection.selected() {
            None => {
                self.key_binding_selection.select(Some(0));
                0
            }
            Some(p) => p,
        };
        let table = Table::new(
            self.get_rows(selected_action),
            vec![Constraint::Length(20), Constraint::Length(30)],
        )
        .header(
            Row::new(vec!["action", current_labels().name])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(current_labels().keybinding_settings),
        )
        .widths([Constraint::Length(20), Constraint::Length(30)]);

        f.render_widget(table, area);
    }

    async fn reset_settings(&mut self, default_settings: Settings) -> AppAction {
        match self.settings_writer.save(default_settings).await {
            Ok(saved_settings) => {
                set_settings(saved_settings.clone());
                self.refresh_data().await;
                self.notifier
                    .banner
                    .set_info(current_labels().operation_successful.to_string());
                AppAction::None
            }
            Err(_) => {
                self.notifier
                    .banner
                    .set_error(current_labels().could_not_remove_keybinding.to_string());
                AppAction::None
            }
        }
    }
}

fn get_context_menu(settings: &Settings) -> (Vec<(String, String)>, KeyBindings) {
    let screen_actions = &[
        Sba::Simple(ScreenActionEnum::Previous),
        Sba::Simple(ScreenActionEnum::Next),
        Sba::Simple(ScreenActionEnum::Edit),
        Sba::Simple(ScreenActionEnum::Reset),
        Sba::Simple(ScreenActionEnum::ReportAnIssue),
        Sba::Simple(ScreenActionEnum::Back),
        Sba::Simple(ScreenActionEnum::Quit),
    ];

    let kb = &settings.keybindings;
    let footer_entries = get_keybinding_actions(kb, screen_actions);
    let screen_key_bindings = kb.slice(Sba::keys(screen_actions));
    (footer_entries, screen_key_bindings)
}
