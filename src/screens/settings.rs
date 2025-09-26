use std::str::FromStr;

use crate::{
    localization::{current_labels, set_language},
    ops::save_settings,
    screens::{
        components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner},
        screen::{AppAction, Screen},
    },
    shapes::{enums::LanguageEnum, settings::Settings},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

#[derive(Debug)]
pub struct SettingsScreen {
    language: Option<LanguageEnum>,
    field: usize,
    language_selection: ListState,
    notify_message: NotifyBanner,
    back: bool,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
}

impl Screen for SettingsScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
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
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _) => self.handle_enter(),
            _ => AppAction::None,
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(1)])
            .split(body);
        if self.field == 0 {
            self.render_language_list(f, inner[0]);
        } else {
            self.render_language_widget(f, inner[0]);
        }
        self.notify_message.render(f, footer_right);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

impl SettingsScreen {
    pub fn new(settings: Settings) -> Self {
        let language = LanguageEnum::from_str(&settings.language).ok();
        let language_selection = match language {
            Some(language) => {
                let mut state = ListState::default();
                let index = LanguageEnum::ALL.iter().position(|&r| r == language);
                state.select(index);
                state
            }
            None => ListState::default(),
        };
        SettingsScreen {
            language: None,
            field: 0,
            language_selection,
            notify_message: NotifyBanner::new(),
            back: false,
            footer: NavigationFooter::new(),
            footer_entries: vec![
                (
                    current_labels().enter.to_string(),
                    current_labels().confirm.to_string(),
                ),
                ("Esc".to_string(), current_labels().back.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ],
        }
    }

    fn handle_enter(&mut self) -> AppAction {
        match self.language {
            Some(language) => match save_settings(language.iso_code().to_string()) {
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
            },
            _ => {
                self.notify_message
                    .set_error(current_labels().language_is_required.to_string());
                AppAction::None
            }
        }
    }

    fn handle_up(&mut self) -> AppAction {
        match (self.field, &self.language_selection.selected()) {
            (0, Some(selected)) => {
                let new_selected = if *selected == 0 {
                    LanguageEnum::ALL.len() - 1
                } else {
                    selected - 1
                };
                self.language = Some(LanguageEnum::ALL[new_selected]);
                self.language_selection.select(Some(new_selected));
            }
            (0, None) => {
                self.language_selection.select(Some(0));
                self.language = Some(LanguageEnum::ALL[0]);
            }
            _ => {}
        };
        AppAction::None
    }

    fn handle_down(&mut self) -> AppAction {
        match (self.field, self.language_selection.selected()) {
            (0, Some(selected)) => {
                let new_selected = (selected + 1) % LanguageEnum::ALL.len();
                self.language_selection.select(Some(new_selected));
                self.language = Some(LanguageEnum::ALL[new_selected]);
            }
            (0, None) => {
                self.language_selection.select(Some(0));
                self.language = Some(LanguageEnum::ALL[0]);
            }
            _ => {}
        };
        AppAction::None
    }

    fn render_language_widget(&mut self, f: &mut Frame, area: Rect) {
        let language_widget = Paragraph::new(if let Some(language) = self.language {
            format!("{}: {}", current_labels().language, language)
        } else {
            format!("{}:", current_labels().language)
        })
        .style(Style::default());
        f.render_widget(language_widget, area);
    }

    fn render_language_list(&mut self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = LanguageEnum::ALL
            .iter()
            .map(|r| ListItem::new(r.to_string()))
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(current_labels().language),
            )
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .highlight_symbol(">> ");
        f.render_stateful_widget(list, area, &mut self.language_selection);
    }
}
