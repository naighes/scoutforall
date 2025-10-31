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
        keybinding::{KeyBindings, ScreenKeyBindings},
        settings::{set_settings, Settings},
    },
};
use async_trait::async_trait;
use crokey::{
    crossterm::{self, event::KeyCode},
    KeyCombinationFormat,
};
use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Row, ScrollbarState, Table, TableState},
    Frame,
};

const ITEM_HEIGHT: usize = 2;

#[derive(Debug)]
pub struct KeybindingScreen<SW: SettingsWriter + Send + Sync, SR: SettingsReader + Send + Sync> {
    language: Select<LanguageEnum>,
    analytics_enabled: CheckBox,
    key_binding_state: TableState,
    notifier: NotifyDialogue<Settings>,
    back: bool,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    settings_writer: Arc<SW>,
    settings_reader: Arc<SR>,
    settings: Settings,
    screen_key_bindings: ScreenKeyBindings,
    format: KeyCombinationFormat,
    scroll_state: ScrollbarState,
    items: Vec<(String, String)>,
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
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
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
                    match self.key_binding_state.selected().map(|selected| {
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
            self.items = Self::get_items(&settings.keybindings, &self.format);
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
        let key_binding_state = TableState::default().with_selected(0);

        let format = KeyCombinationFormat::default();

        let items = Self::get_items(&settings.keybindings, &format);

        let scroll_state = ScrollbarState::new((items.len() - 1) * ITEM_HEIGHT);

        let (footer_entries, screen_key_bindings) = get_context_menu(&settings);

        KeybindingScreen {
            language,
            analytics_enabled,
            key_binding_state,
            notifier: NotifyDialogue::new(),
            back: false,
            footer: NavigationFooter::new(),
            footer_entries,
            settings_writer,
            settings_reader,
            settings,
            screen_key_bindings,
            format,
            scroll_state,
            items,
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
        let selected_index = match self.key_binding_state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.key_binding_state.select(Some(selected_index));
        self.scroll_state = self.scroll_state.position(selected_index * ITEM_HEIGHT);
        AppAction::None
    }

    fn handle_backtab(&mut self) -> AppAction {
        let selected_index = match self.key_binding_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.key_binding_state.select(Some(selected_index));
        self.scroll_state = self.scroll_state.position(selected_index * ITEM_HEIGHT);
        AppAction::None
    }

    fn get_rows(items: Vec<(String, String)>, selected_action: usize) -> Vec<Row<'static>> {
        let rows = items
            .iter()
            .enumerate()
            .map(|(index, (action, keybindings))| {
                let mut row = Row::new(vec![action.to_owned(), keybindings.to_owned()]);
                if index == selected_action {
                    row = row
                        .style(
                            Style::default()
                                .add_modifier(Modifier::REVERSED)
                                .add_modifier(Modifier::BOLD),
                        )
                        .height(ITEM_HEIGHT as u16);
                }
                row
            })
            .collect();
        rows
    }

    fn get_items(kc: &KeyBindings, format: &KeyCombinationFormat) -> Vec<(String, String)> {
        ScreenActionEnum::ALL
            .iter()
            .map(|r| {
                let s = kc
                    .reverse_map()
                    .get(r)
                    .map(|f| {
                        f.iter()
                            .map(|q| format.to_string(*q))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or(current_labels().unassigned.to_string())
                    .to_string();
                (r.with_desc().1, s)
            })
            .collect::<Vec<_>>()
    }

    fn render_key_bindings_list(&mut self, f: &mut Frame, area: Rect) {
        // First, get the selected index without borrowing self immutably for too long
        let selected_action = self.key_binding_state.selected().unwrap_or_else(|| {
            self.key_binding_state.select(Some(0));
            0
        });

        // Now, generate the rows
        let rows = Self::get_rows(self.items.clone(), selected_action);

        // Create the table widget
        let table = Table::new(rows, vec![Constraint::Length(20), Constraint::Length(30)])
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

        // Finally, render the widget with a mutable borrow
        f.render_stateful_widget(table, area, &mut self.key_binding_state);
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

fn get_context_menu(settings: &Settings) -> (Vec<(String, String)>, ScreenKeyBindings) {
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
