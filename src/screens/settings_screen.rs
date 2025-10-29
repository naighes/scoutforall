use std::sync::Arc;

use crate::{
    localization::current_labels,
    providers::settings_writer::SettingsWriter,
    screens::{
        components::{
            checkbox::CheckBox, navigation_footer::NavigationFooter, notify_banner::NotifyBanner,
            select::Select,
        },
        report_an_issue_screen::ReportAnIssueScreen,
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
    },
    shapes::{
        enums::{LanguageEnum, ScreenActionEnum},
        settings::{set_settings, Settings},
    },
};
use async_trait::async_trait;
use crokey::{crossterm, Combiner, KeyCombinationFormat};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Paragraph},
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
    format: KeyCombinationFormat,
    combiner: crokey::Combiner,
    screen_key_bindings: crate::shapes::keybinding::KeyBindings,
}

impl<SW: SettingsWriter + Send + Sync> Renderable for SettingsScreen<SW> {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),       // language
                Constraint::Length(2),       // analytics checkbox
                Constraint::Percentage(100), // keybindings
                Constraint::Min(1),
            ])
            .split(body);
        self.language.render(f, inner[0]);
        self.analytics_enabled.render(f, inner[1]);
        self.render_key_bindings_widget(f, inner[2]);
        self.notify_message.render(f, footer_right);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<SW: SettingsWriter + Send + Sync> ScreenAsync for SettingsScreen<SW> {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.combiner.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                &self.notify_message.has_value(),
            ) {
                (_, _, true) => {
                    self.notify_message.reset();
                    if self.back {
                        AppAction::Back(true, Some(1))
                    } else {
                        AppAction::None
                    }
                }
                (_, KeyCode::Up, _) => self.handle_up(),
                (_, KeyCode::Down, _) => self.handle_down(),
                (Some(&ScreenActionEnum::Next), _, _) => self.handle_tab(),
                (Some(&ScreenActionEnum::Previous), _, _) => self.handle_backtab(),
                (_, KeyCode::Char(c), _) => self.handle_char(c),
                (Some(&ScreenActionEnum::Back), _, _) => AppAction::Back(true, Some(1)),
                (Some(&ScreenActionEnum::Confirm), _, _) => self.handle_enter().await,
                _ => AppAction::None,
            }
        } else {
            AppAction::None
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
        let screen_actions = &[
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::ReportAnIssue),
            Sba::Simple(ScreenActionEnum::Back),
            Sba::Simple(ScreenActionEnum::Quit),
        ];
        let kb = &settings.keybindings;
        let footer_entries = get_keybinding_actions(kb, screen_actions);
        let screen_key_bindings = kb.slice(Sba::keys(screen_actions));
        SettingsScreen {
            language,
            analytics_enabled,
            field: 0,
            notify_message: NotifyBanner::new(),
            back: false,
            footer: NavigationFooter::new(),
            footer_entries,
            settings_writer,
            settings,
            format: KeyCombinationFormat::default(),
            combiner: Combiner::default(),
            screen_key_bindings,
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
            AppAction::SwitchScreen(Box::new(ReportAnIssueScreen::new(self.settings.clone())))
        } else {
            self.analytics_enabled.handle_char(c);
            AppAction::None
        }
    }

    fn render_key_bindings_widget(&mut self, f: &mut Frame, area: Rect) {
        let keybindings = &self.settings.keybindings;
        let key_binding_widget = Paragraph::new({
            let actions_bindings_map = keybindings.reverse_map();
            let items: Vec<String> = ScreenActionEnum::ALL
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
                    format!("{:?}: {}", r.with_desc().1, s)
                })
                .collect();
            format!("{}:", items.join("\n"))
        })
        .style(Style::default())
        .block(
            Block::new()
                .title(current_labels().keybinding_settings.to_string())
                .borders(Borders::ALL),
        );
        f.render_widget(key_binding_widget, area);
    }
}
