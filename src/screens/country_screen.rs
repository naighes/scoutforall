use crate::shapes::keybinding::navigation_keybindings;
use std::sync::Arc;

use crate::localization::current_labels;
use crate::{
    providers::settings_writer::SettingsWriter,
    screens::{
        components::{
            country_picker::CountryPicker, navigation_footer::NavigationFooter,
            notify_banner::NotifyBanner, panel,
        },
        screen::{
            AppAction, FieldKey, Renderable, Sba, ScreenAsync, get_keybinding_actions,
            navigation_entries,
        },
    },
    shapes::{
        actions::ScreenActionEnum,
        keybinding::ActionsKeyBindings,
        settings::{Settings, set_settings},
    },
};
use async_trait::async_trait;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect};
use scout4all_core::settings::TeamDefaults;

/// The country a new team starts with, chosen from the settings: typing
/// narrows the list, up and down (and the tabs) go through it, enter saves.
#[derive(Debug)]
pub struct CountryScreen<SW: SettingsWriter + Send + Sync> {
    settings: Settings,
    picker: CountryPicker,
    notify_message: NotifyBanner,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    settings_writer: Arc<SW>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

impl<SW: SettingsWriter + Send + Sync> Renderable for CountryScreen<SW> {
    fn footer_entries(&self) -> Vec<(String, String)> {
        self.footer_entries.clone()
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let block = panel::screen(current_labels().country);
        let inner = block.inner(body);
        f.render_widget(block, body);
        self.picker.render_list(f, inner);
        self.notify_message.render(f, footer_right);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<SW: SettingsWriter + Send + Sync> ScreenAsync for CountryScreen<SW> {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let Some(key_combination) = self.screen_key_bindings.transform(key) else {
            return AppAction::None;
        };
        self.notify_message.reset();
        match (
            FieldKey::of(key.code),
            key.code,
            self.screen_key_bindings.get(key_combination),
        ) {
            (Some(FieldKey::Previous), _, _) => self.picker.handle_up(),
            (Some(FieldKey::Next), _, _) => self.picker.handle_down(),
            (_, KeyCode::Backspace, _) => self.picker.handle_backspace(),
            (_, _, Some(ScreenActionEnum::Back)) => return AppAction::Back(true, Some(1)),
            (_, _, Some(ScreenActionEnum::Confirm)) => return self.handle_confirm().await,
            (_, KeyCode::Char(c), _) => self.picker.handle_char(c),
            _ => {}
        }
        AppAction::None
    }

    async fn refresh_data(&mut self) {}
}

impl<SW: SettingsWriter + Send + Sync> CountryScreen<SW> {
    pub fn new(settings: Settings, settings_writer: Arc<SW>) -> Self {
        let actions = &[
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
        ];
        let kb = navigation_keybindings();
        let footer_entries = navigation_entries()
            .into_iter()
            .chain(get_keybinding_actions(kb, actions))
            .collect();
        let screen_key_bindings = kb.slice(Sba::keys(actions));
        let picker = CountryPicker::new(
            current_labels().country.to_string(),
            settings.team_defaults.country,
            true,
        );
        CountryScreen {
            settings,
            picker,
            notify_message: NotifyBanner::new(),
            footer: NavigationFooter::new(),
            footer_entries,
            settings_writer,
            screen_key_bindings,
        }
    }

    async fn handle_confirm(&mut self) -> AppAction {
        let settings = Settings {
            team_defaults: TeamDefaults {
                country: self.picker.get_selected_value(),
            },
            ..self.settings.clone()
        };
        match self.settings_writer.save(settings).await {
            Ok(saved) => {
                set_settings(saved);
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
