use std::{fmt::Debug, sync::Arc};

use crate::{
    localization::current_labels,
    providers::settings_writer::SettingsWriter,
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner, text_box::TextBox,
        },
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
    },
    shapes::{
        enums::ScreenActionEnum,
        keybinding::KeyBindings,
        settings::{current_settings, set_settings, Settings},
    },
};
use async_trait::async_trait;
use crokey::{crossterm::event::KeyEvent, key, Combiner, KeyCombinationFormat};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    Frame,
};

#[derive(Debug)]
pub struct AddKeyBindings<SW: SettingsWriter + Send + Sync> {
    settings: Settings,
    action: ScreenActionEnum,
    shortcut: TextBox,
    notify_message: NotifyBanner,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    settings_writer: Arc<SW>,
    combiner: Combiner,
    screen_key_bindings: KeyBindings,
    fmt: KeyCombinationFormat,
}

impl<SW: SettingsWriter + Send + Sync> Renderable for AddKeyBindings<SW> {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);

        let area = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // opponent
                Constraint::Min(1),
            ])
            .split(container[1]);
        self.notify_message.render(f, footer_right);
        self.render_header(f, container[1]);
        self.shortcut.render(f, area[0]);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<SW: SettingsWriter + Send + Sync> ScreenAsync for AddKeyBindings<SW> {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.combiner.transform(key) {
            match (
                key_combination,
                self.screen_key_bindings.get(key_combination),
                &self.notify_message.has_value(),
            ) {
                (_, _, true) => {
                    self.notify_message.reset();
                    AppAction::None
                }
                (key!(Backspace), _, _) => self.handle_backspace(),
                (_, Some(ScreenActionEnum::Back), _) => AppAction::Back(true, Some(1)),
                (_, Some(ScreenActionEnum::Confirm), _) => self.handle_confirm().await,
                (_, None, false) => self.handle_combination(self.fmt.to_string(key_combination)),
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<SW: SettingsWriter + Send + Sync> AddKeyBindings<SW> {
    pub fn new(action: ScreenActionEnum, settings_writer: Arc<SW>) -> Self {
        let shortcut = TextBox::new("shortcut".to_owned(), true, None);
        let screen_actions = &[
            Sba::Simple(ScreenActionEnum::Next),
            Sba::Simple(ScreenActionEnum::Previous),
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
        ];
        let settings = current_settings();
        let kb = settings.keybindings.clone();
        let footer_entries = get_keybinding_actions(&kb, screen_actions);
        let screen_key_bindings = kb.slice(Sba::keys(screen_actions));
        AddKeyBindings {
            settings,
            action,
            shortcut,
            notify_message: NotifyBanner::new(),
            footer: NavigationFooter::new(),
            footer_entries,
            combiner: Combiner::default(),
            screen_key_bindings,
            settings_writer,
            fmt: KeyCombinationFormat::default(),
        }
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.action.with_desc().1);
        f.render_widget(block, area);
    }

    async fn handle_confirm(&mut self) -> AppAction {
        match self.shortcut.get_selected_value() {
            None => {
                self.notify_message
                    .set_error(current_labels().opponent_cannot_be_empty.to_string());
                AppAction::None
            }
            Some(sc) => {
                let kc = crokey::parse(&sc);
                match kc {
                    Ok(kc) => {
                        let mut settings = self.settings.clone();
                        let mut keybindings = settings.keybindings.clone();
                        if keybindings.set(self.action, kc) {
                            settings.keybindings = keybindings.clone();
                            match self.settings_writer.save(settings).await {
                                Ok(saved_settings) => {
                                    set_settings(saved_settings.clone());
                                    return AppAction::Back(true, Some(1));
                                }
                                Err(_) => {
                                    self.notify_message.set_error(
                                        current_labels().could_not_save_settings.to_string(),
                                    );
                                    return AppAction::None;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        self.notify_message
                            .set_error(current_labels().invalid_shortcut.to_string());
                        return AppAction::None;
                    }
                }
                AppAction::None
            }
        }
    }

    fn handle_backspace(&mut self) -> AppAction {
        self.shortcut.handle_backspace();
        AppAction::None
    }

    fn handle_combination(&mut self, c: String) -> AppAction {
        c.chars().for_each(|ch| self.shortcut.handle_char(ch));
        AppAction::None
    }
}
