use std::{collections::HashSet, fmt::Debug, sync::Arc};

use crate::{
    localization::current_labels,
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{navigation_footer::NavigationFooter, notify_dialogue::NotifyDialogue},
        keybindings_action_add_screen::AddKeyBindingScreen,
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
    },
    shapes::{
        enums::{ScreenActionEnum, WithDesc},
        keybinding::{ActionsKeyBindings, KeyBindings},
        settings::{set_settings, Settings},
    },
};
use async_trait::async_trait;
use crokey::{
    crossterm::event::{KeyCode, KeyEvent},
    KeyCombination, KeyCombinationFormat,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};
use std::hash::Hash;

#[derive(Debug)]
pub struct KeyBindingScreen<
    SW: SettingsWriter + Send + Sync,
    SR: SettingsReader + Send + Sync,
    T: Clone + Copy + Send + Sync + Eq + Hash + WithDesc<T> + 'static,
> {
    settings: Settings,
    list_state: ListState,
    notifier: NotifyDialogue<KeyCombination>,
    footer: NavigationFooter,
    settings_writer: Arc<SW>,
    settings_reader: Arc<SR>,
    keybindings: KeyBindings<T>,
    action: T,
    format: KeyCombinationFormat,
    key_combinations: HashSet<KeyCombination>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
    footer_entries: Vec<(String, String)>,
    fn_kb_retriever: fn(s: &Settings) -> KeyBindings<T>,
    fn_settings_updater: fn(s: &Settings, &KeyBindings<T>) -> Settings,
}

#[async_trait]
impl<
        SW: SettingsWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
        T: Clone + Copy + Send + Sync + Eq + Hash + WithDesc<T> + 'static,
    > ScreenAsync for KeyBindingScreen<SW, SR, T>
{
    async fn refresh_data(&mut self) {
        if let Ok(settings) = &self.settings_reader.read().await {
            self.settings = settings.to_owned();
            self.keybindings = (self.fn_kb_retriever)(&self.settings);
            if let Some(key_combination) = self.keybindings.reverse_map().get(&self.action) {
                let kb = &settings.keybindings.clone();
                self.key_combinations = key_combination.clone();
                let length = self.key_combinations.len();
                let screen_actions = &Self::get_screen_actions(&length);
                let footer_entries = get_keybinding_actions(kb, screen_actions);
                let screen_key_bindings = settings.keybindings.slice(Sba::keys(screen_actions));
                self.footer_entries = footer_entries;
                self.screen_key_bindings = screen_key_bindings;
            } else {
                self.key_combinations = HashSet::new();
            }
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
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
                (Some(ScreenActionEnum::New), _, _, _) => {
                    AppAction::SwitchScreen(Box::new(AddKeyBindingScreen::new(
                        self.action,
                        self.settings_writer.clone(),
                        self.keybindings.clone(),
                        self.fn_settings_updater,
                    )))
                }
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

impl<
        SW: SettingsWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
        T: Clone + Copy + Send + Sync + Eq + Hash + WithDesc<T> + 'static,
    > Renderable for KeyBindingScreen<SW, SR, T>
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

impl<
        SW: SettingsWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
        T: Clone + Copy + Send + Sync + Eq + Hash + WithDesc<T> + 'static,
    > KeyBindingScreen<SW, SR, T>
{
    pub fn new(
        settings: Settings,
        action: T,
        key_combinations: HashSet<KeyCombination>,
        format: KeyCombinationFormat,
        settings_writer: Arc<SW>,
        settings_reader: Arc<SR>,
        keybindings: KeyBindings<T>,
        fn_kb_retriever: fn(s: &Settings) -> KeyBindings<T>,
        fn_settings_updater: fn(s: &Settings, &KeyBindings<T>) -> Settings,
    ) -> Self {
        let length = key_combinations.len();
        let screen_actions = &Self::get_screen_actions(&length);
        let kb = &settings.keybindings.clone();
        let footer_entries = get_keybinding_actions(kb, screen_actions);
        let screen_key_bindings = settings.keybindings.slice(Sba::keys(screen_actions));

        KeyBindingScreen {
            settings,
            action,
            key_combinations,
            format,
            list_state: ListState::default(),
            notifier: NotifyDialogue::new(),
            footer: NavigationFooter::new(),
            settings_writer,
            settings_reader,
            footer_entries,
            screen_key_bindings,
            keybindings,
            fn_kb_retriever,
            fn_settings_updater,
        }
    }

    fn get_screen_actions(length: &usize) -> Vec<Sba<ScreenActionEnum>> {
        if *length > 1 {
            vec![
                Sba::Simple(ScreenActionEnum::Previous),
                Sba::Simple(ScreenActionEnum::Next),
                Sba::Simple(ScreenActionEnum::New),
                Sba::Simple(ScreenActionEnum::Delete),
                Sba::Simple(ScreenActionEnum::Back),
            ]
        } else {
            vec![
                Sba::Simple(ScreenActionEnum::New),
                Sba::Simple(ScreenActionEnum::Back),
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
        action: &mut T,
        key_combination: KeyCombination,
        settings_writer: Arc<SW>,
    ) -> AppAction {
        let keybindings = &mut (self.fn_kb_retriever)(&self.settings);

        if keybindings.remove(*action, key_combination) {
            let settings = (self.fn_settings_updater)(&self.settings, keybindings);
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
                        .set_error(current_labels().could_not_reset_keybindings.to_string());
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
