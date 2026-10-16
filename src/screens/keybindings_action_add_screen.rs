use crate::shapes::keybinding::navigation_keybindings;
use std::{fmt::Debug, sync::Arc};

use crate::{
    localization::current_labels,
    providers::settings_writer::SettingsWriter,
    screens::{
        components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner},
        screen::{AppAction, Renderable, Sba, ScreenAsync, get_keybinding_actions},
    },
    shapes::{
        enums::ScreenActionEnum,
        keybinding::{ActionsKeyBindings, KeyBindings, combination_of, is_fixed_key},
        settings::{ScoutingShortcut, Settings, set_settings},
        symbol::KeyCombinationFormatExt,
    },
};
use async_trait::async_trait;
use crokey::{
    KeyCombination, KeyCombinationFormat,
    crossterm::event::{KeyCode, KeyEvent},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Assigns one more key to a scouting shortcut: the key pressed is taken as it
/// is (enter confirms it, backspace clears it, esc goes back), unless it's a
/// fixed key or already bound to another shortcut of the same kind.
#[derive(Debug)]
pub struct AddKeyBindingScreen<SW: SettingsWriter + Send + Sync, T: ScoutingShortcut> {
    settings: Settings,
    action: T,
    shortcut: Option<KeyCombination>,
    notify_message: NotifyBanner,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    settings_writer: Arc<SW>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
    fmt: KeyCombinationFormat,
}

impl<SW: SettingsWriter + Send + Sync, T: ScoutingShortcut + Debug> Renderable
    for AddKeyBindingScreen<SW, T>
{
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(1)])
            .split(body);
        let shortcut = self
            .shortcut
            .map(|ck| KeyCombinationFormatExt::new(&self.fmt).to_string(ck))
            .unwrap_or_default();
        let widget = Paragraph::new(vec![
            Line::from(current_labels().press_the_shortcut),
            Line::from(Span::styled(
                shortcut,
                Style::default().add_modifier(Modifier::BOLD),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(self.action.with_desc().1),
        );
        f.render_widget(widget, area[0]);
        self.notify_message.render(f, footer_right);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<SW: SettingsWriter + Send + Sync, T: ScoutingShortcut + Debug> ScreenAsync
    for AddKeyBindingScreen<SW, T>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let Some(key_combination) = self.screen_key_bindings.transform(key) else {
            return AppAction::None;
        };
        self.notify_message.reset();
        match (self.screen_key_bindings.get(key_combination), key.code) {
            (Some(ScreenActionEnum::Back), _) => AppAction::Back(true, Some(1)),
            (Some(ScreenActionEnum::Confirm), _) => self.handle_confirm().await,
            (_, KeyCode::Backspace) => {
                self.shortcut = None;
                AppAction::None
            }
            _ => {
                let ck = combination_of(key);
                self.shortcut = None;
                match self.conflict(&ck) {
                    Some(message) => self.notify_message.set_error(message),
                    None => self.shortcut = Some(ck),
                }
                AppAction::None
            }
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<SW: SettingsWriter + Send + Sync, T: ScoutingShortcut + Debug> AddKeyBindingScreen<SW, T> {
    pub fn new(action: T, settings: Settings, settings_writer: Arc<SW>) -> Self {
        let screen_actions = &[
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
        ];
        let kb = navigation_keybindings();
        let footer_entries = get_keybinding_actions(kb, screen_actions);
        let screen_key_bindings = kb.slice(Sba::keys(screen_actions));
        AddKeyBindingScreen {
            settings,
            action,
            shortcut: None,
            notify_message: NotifyBanner::new(),
            footer: NavigationFooter::new(),
            footer_entries,
            screen_key_bindings,
            settings_writer,
            fmt: KeyCombinationFormat::default(),
        }
    }

    /// why the key can't be assigned, if it can't
    fn conflict(&self, ck: &KeyCombination) -> Option<String> {
        let key = KeyCombinationFormatExt::new(&self.fmt).to_string(*ck);
        if is_fixed_key(ck) {
            return Some(current_labels().reserved_shortcut.replace("{}", &key));
        }
        T::bindings(&self.settings)
            .action_for(ck)
            .filter(|other| **other != self.action)
            .map(|other| {
                current_labels()
                    .shortcut_already_assigned
                    .replacen("{}", &key, 1)
                    .replacen("{}", &other.with_desc().1, 1)
            })
    }

    async fn handle_confirm(&mut self) -> AppAction {
        let Some(ck) = self.shortcut else {
            self.notify_message
                .set_error(current_labels().press_the_shortcut.to_string());
            return AppAction::None;
        };
        if let Some(message) = self.conflict(&ck) {
            self.notify_message.set_error(message);
            return AppAction::None;
        }
        let mut keybindings: KeyBindings<T> = T::bindings(&self.settings);
        keybindings.set(self.action, ck);
        let settings = T::with_bindings(&self.settings, &keybindings);
        match self.settings_writer.save(settings).await {
            Ok(saved_settings) => {
                set_settings(saved_settings);
                AppAction::Back(true, Some(1))
            }
            Err(_) => {
                self.notify_message
                    .set_error(current_labels().could_not_save_settings.to_string());
                AppAction::None
            }
        }
    }
}
