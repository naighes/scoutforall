use std::{collections::HashSet, fmt::Debug, sync::Arc};

use crate::{
    localization::current_labels,
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{navigation_footer::NavigationFooter, notify_dialogue::NotifyDialogue},
        keybindings_action_add_screen::AddKeyBindings,
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
    },
    shapes::{
        enums::ScreenActionEnum,
        keybinding::KeyBindings,
        settings::{set_settings, Settings},
    },
};
use async_trait::async_trait;
use crokey::{
    crossterm::event::{KeyCode, KeyEvent},
    Combiner, KeyCombination, KeyCombinationFormat,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

#[derive(Debug)]
pub struct KeyBindingActionScreen<
    SW: SettingsWriter + Send + Sync,
    SR: SettingsReader + Send + Sync,
> {
    settings: Settings,
    list_state: ListState,
    notifier: NotifyDialogue<KeyCombination>,
    footer: NavigationFooter,
    settings_writer: Arc<SW>,
    settings_reader: Arc<SR>,
    action: ScreenActionEnum,
    format: KeyCombinationFormat,
    key_combinations: HashSet<KeyCombination>,
    screen_key_bindings: KeyBindings,
    combiner: Combiner,
    footer_entries: Vec<(String, String)>,
}

#[async_trait]
impl<SW: SettingsWriter + Send + Sync + 'static, SR: SettingsReader + Send + Sync + 'static>
    ScreenAsync for KeyBindingActionScreen<SW, SR>
{
    async fn refresh_data(&mut self) {
        if let Ok(settings) = &self.settings_reader.read().await {
            self.settings = settings.to_owned();
            if let Some(key_combination) = settings.keybindings.reverse_map().get(&self.action) {
                let kb = &settings.keybindings.clone();
                self.key_combinations = key_combination.clone();
                let length = self.key_combinations.len();
                let screen_actions = Self::get_screen_actions(&length);
                let footer_entries =
                    get_keybinding_actions(kb, Sba::ScreenActions(&screen_actions));
                let screen_key_bindings = settings.keybindings.slice(screen_actions);
                self.footer_entries = footer_entries;
                self.screen_key_bindings = screen_key_bindings;
            } else {
                self.key_combinations = HashSet::new();
            }
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.combiner.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                &self.notifier.banner.has_value(),
                &self.notifier.has_value(),
            ) {
                (_, _, true, false) => {
                    self.notifier.banner.reset();
                    AppAction::None
                }
                //dialog exits (y|n) have higher priority
                (_, KeyCode::Char(x), _, true) => {
                    let selected = self.notifier.entry.to_owned();
                    self.notifier.reset();
                    if x == *current_labels().y {
                        match selected {
                            Some(player) => {
                                self.remove(
                                    &mut self.action.clone(),
                                    player,
                                    self.settings_writer.clone(),
                                )
                                .await
                            }

                            None => return AppAction::None,
                        }
                    } else {
                        AppAction::None
                    }
                }
                (Some(ScreenActionEnum::Next), _, _, _) => {
                    self.next_team();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Previous), _, _, _) => {
                    self.previous_team();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Back), _, _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::New), _, _, _) => AppAction::SwitchScreen(Box::new(
                    AddKeyBindings::new(self.action, self.settings_writer.clone()),
                )),
                (Some(ScreenActionEnum::Delete), _, _, _) => {
                    match self.list_state.selected().map(|selected: usize| {
                        let u = self.key_combinations.iter().nth(selected).cloned();
                        match u {
                            Some(p) => {
                                self.notifier.set(p.to_owned()).banner.set_warning(
                                    current_labels()
                                        .remove_keybinding_confirmation
                                        .to_string()
                                        .replace("{}", self.format.to_string(p).as_str()),
                                );
                                AppAction::None
                            }
                            None => AppAction::None,
                        }
                    }) {
                        Some(action) => action,
                        None => AppAction::None,
                    }
                }
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }
}

impl<SW: SettingsWriter + Send + Sync + 'static, SR: SettingsReader + Send + Sync + 'static>
    Renderable for KeyBindingActionScreen<SW, SR>
{
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        self.notifier.render(f, footer_right);
        let items: Vec<ListItem> = self
            .key_combinations
            .iter()
            .map(|t| ListItem::new(self.format.to_string(*t)))
            .collect();

        self.render_list(f, body, items);

        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

impl<SW: SettingsWriter + Send + Sync + 'static, SR: SettingsReader + Send + Sync + 'static>
    KeyBindingActionScreen<SW, SR>
{
    pub fn new(
        settings: Settings,
        action: ScreenActionEnum,
        key_combinations: HashSet<KeyCombination>,
        format: KeyCombinationFormat,
        settings_writer: Arc<SW>,
        settings_reader: Arc<SR>,
    ) -> Self {
        let length = key_combinations.len();
        let screen_actions = Self::get_screen_actions(&length);
        let kb = &settings.keybindings.clone();
        let footer_entries = get_keybinding_actions(kb, Sba::ScreenActions(&screen_actions));
        let screen_key_bindings = settings.keybindings.slice(screen_actions);

        KeyBindingActionScreen {
            settings,
            action,
            key_combinations,
            format,
            list_state: ListState::default(),
            notifier: NotifyDialogue::new(),
            footer: NavigationFooter::new(),
            settings_writer,
            settings_reader,
            combiner: Combiner::default(),
            footer_entries,
            screen_key_bindings,
        }
    }

    fn get_screen_actions(length: &usize) -> Vec<&ScreenActionEnum> {
        if *length > 1 {
            vec![
                &ScreenActionEnum::Previous,
                &ScreenActionEnum::Next,
                &ScreenActionEnum::New,
                &ScreenActionEnum::Delete,
                &ScreenActionEnum::Back,
                &ScreenActionEnum::Quit,
            ]
        } else {
            vec![
                &ScreenActionEnum::New,
                &ScreenActionEnum::Back,
                &ScreenActionEnum::Quit,
            ]
        }
    }

    fn next_team(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = (selected + 1).min(self.key_combinations.len() - 1);
            self.list_state.select(Some(new_selected));
        }
    }

    fn previous_team(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = if selected == 0 { 0 } else { selected - 1 };
            self.list_state.select(Some(new_selected));
        }
    }

    fn render_list(&mut self, f: &mut Frame, area: Rect, items: Vec<ListItem>) {
        if self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(self.action.with_desc().1),
            )
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }
    async fn remove(
        &mut self,
        action: &mut ScreenActionEnum,
        key_combination: KeyCombination,
        settings_writer: Arc<SW>,
    ) -> AppAction {
        let keybindings = &mut self.settings.keybindings;

        if keybindings.remove(*action, key_combination) {
            let settings = Settings {
                language: self.settings.language,
                analytics_enabled: self.settings.analytics_enabled,
                keybindings: keybindings.clone(),
                last_used_dir: self.settings.last_used_dir.to_owned(),
            };
            match settings_writer.save(settings).await {
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
                        .set_error(current_labels().could_not_remove_player.to_string());
                    AppAction::None
                }
            }
        } else {
            self.notifier
                .banner
                .set_error(current_labels().could_not_remove_player.to_string());
            AppAction::None
        }
    }
}
