use std::sync::Arc;

use crate::{
    localization::{current_labels, set_language},
    providers::settings_writer::SettingsWriter,
    screens::{
        components::{
            checkbox::CheckBox, navigation_footer::NavigationFooter, notify_banner::NotifyBanner,
            select::Select,
        },
        report_an_issue_screen::ReportAnIssueScreen,
        screen::{AppAction, Renderable, ScreenAsync},
    },
    shapes::{enums::LanguageEnum, settings::Settings},
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

#[derive(Debug)]
pub struct SettingsScreen<SW: SettingsWriter + Send + Sync> {
    language: Select<LanguageEnum>,
    analytics_enabled: CheckBox,
    field: usize,
    notify_message: NotifyBanner,
    back: bool,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    settings_writer: Arc<SW>,
    settings: Settings,
}

impl<SW: SettingsWriter + Send + Sync> Renderable for SettingsScreen<SW> {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // language
                Constraint::Length(3), // analytics checkbox
                Constraint::Min(1),
            ])
            .split(body);
        self.language.render(f, inner[0]);
        self.analytics_enabled.render(f, inner[1]);
        self.notify_message.render(f, footer_right);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<SW: SettingsWriter + Send + Sync> ScreenAsync for SettingsScreen<SW> {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, &self.notify_message.has_value()) {
            (_, true) => {
                self.notify_message.reset();
                if self.back {
                    AppAction::Back(true, Some(1))
                } else {
                    AppAction::None
                }
            }
            (KeyCode::Up, _) => self.handle_up(),
            (KeyCode::Down, _) => self.handle_down(),
            (KeyCode::Tab, _) => self.handle_tab(),
            (KeyCode::BackTab, _) => self.handle_backtab(),
            (KeyCode::Char(c), _) => self.handle_char(c),
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _) => self.handle_enter().await,
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<SW: SettingsWriter + Send + Sync> SettingsScreen<SW> {
    pub fn new(settings: Settings, settings_writer: Arc<SW>) -> Self {
        let language = Select::new(
            current_labels().language.to_owned(),
            LanguageEnum::ALL.to_vec(),
            Some(settings.language),
            true,
        );
        let analytics_enabled = CheckBox::new(
            current_labels().enable_send_analytics.to_owned(),
            false,
            settings.analytics_enabled,
        );
        SettingsScreen {
            language,
            analytics_enabled,
            field: 0,
            notify_message: NotifyBanner::new(),
            back: false,
            footer: NavigationFooter::new(),
            footer_entries: vec![
                (
                    current_labels().enter.to_string(),
                    current_labels().confirm.to_string(),
                ),
                (
                    "I".to_string(),
                    current_labels().report_an_issue.to_string(),
                ),
                ("Esc".to_string(), current_labels().back.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ],
            settings_writer,
            settings,
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
                    last_used_dir: self.settings.last_used_dir.clone(),
                };
                match self.settings_writer.save(settings).await {
                    Ok(_) => {
                        set_language(language);
                        self.notify_message
                            .set_info(current_labels().operation_successful.to_string());
                        self.back = true;
                        AppAction::None
                    }
                    Err(_) => {
                        self.notify_message
                            .set_error(current_labels().could_not_save_settings.to_string());
                        AppAction::None
                    }
                }
            }
            _ => {
                self.notify_message
                    .set_error(current_labels().language_is_required.to_string());
                AppAction::None
            }
        }
    }

    fn handle_tab(&mut self) -> AppAction {
        self.field = (self.field + 1) % 2;
        self.update_writing_modes();
        AppAction::None
    }

    fn handle_backtab(&mut self) -> AppAction {
        self.field = (self.field + 2) % 2;
        self.update_writing_modes();
        AppAction::None
    }

    fn update_writing_modes(&mut self) {
        self.language.writing_mode = self.field == 0;
        self.analytics_enabled.writing_mode = self.field == 1;
    }

    fn handle_up(&mut self) -> AppAction {
        self.language.handle_up();
        AppAction::None
    }

    fn handle_down(&mut self) -> AppAction {
        self.language.handle_down();
        AppAction::None
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        if c == 'i' || c == 'I' {
            AppAction::SwitchScreen(Box::new(ReportAnIssueScreen::new()))
        } else {
            self.analytics_enabled.handle_char(c);
            AppAction::None
        }
    }
}
