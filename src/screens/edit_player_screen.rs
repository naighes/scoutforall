use std::sync::Arc;

use crate::{
    localization::current_labels,
    providers::team_writer::{PlayerInput, TeamWriter},
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner, select::Select,
            team_header::TeamHeader, text_box::TextBox,
        },
        screen::{AppAction, Renderable, ScreenAsync},
    },
    shapes::{enums::RoleEnum, player::PlayerEntry, team::TeamEntry},
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    Frame,
};

fn validate_player_number(current: &str, c: char) -> bool {
    c.is_ascii_digit() && current.len() < 2 && !(current.is_empty() && c == '0')
}

#[derive(Debug)]
pub struct EditPlayerScreen<TW: TeamWriter + Send + Sync> {
    team: TeamEntry,
    name: TextBox,
    number: TextBox,
    role: Select<RoleEnum>,
    field: usize,
    notify_message: NotifyBanner,
    existing_player: Option<PlayerEntry>,
    back: bool,
    header: TeamHeader,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    team_writer: Arc<TW>,
}

impl<TW: TeamWriter + Send + Sync> Renderable for EditPlayerScreen<TW> {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);
        let area = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // name
                Constraint::Length(3), // role
                Constraint::Length(3), // number
                Constraint::Min(1),
            ])
            .split(container[1]);
        self.notify_message.render(f, footer_right);
        self.render_header(f, container[1]);
        self.name.render(f, area[0]);
        self.role.render(f, area[1]);
        self.number.render(f, area[2]);
        self.header.render(f, container[0], Some(&self.team));
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<TW: TeamWriter + Send + Sync> ScreenAsync for EditPlayerScreen<TW> {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, &self.notify_message.has_value()) {
            (_, true) => self.handle_error_reset(),
            (KeyCode::Char(c), _) => self.handle_char(c),
            (KeyCode::Backspace, _) => self.handle_backspace(),
            (KeyCode::Up, _) => self.handle_up(),
            (KeyCode::Down, _) => self.handle_down(),
            (KeyCode::Tab, _) => self.handle_tab(),
            (KeyCode::BackTab, _) => self.handle_backtab(),
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _) => self.handle_enter().await,
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<TW: TeamWriter + Send + Sync> EditPlayerScreen<TW> {
    pub fn new(team: TeamEntry, team_writer: Arc<TW>) -> Self {
        let role = Select::new(
            current_labels().role.to_owned(),
            RoleEnum::ALL.to_vec(),
            None,
            false,
        );
        let name = TextBox::new(current_labels().name.to_owned(), true, None);
        let number = TextBox::with_validator(
            current_labels().number.to_owned(),
            false,
            None,
            validate_player_number,
        );
        EditPlayerScreen {
            team,
            name,
            number,
            role,
            field: 0,
            notify_message: NotifyBanner::new(),
            existing_player: None,
            back: false,
            header: TeamHeader::default(),
            footer: NavigationFooter::new(),
            footer_entries: vec![
                (
                    "Tab / Shift+Tab".to_string(),
                    current_labels().navigate.to_string(),
                ),
                (
                    current_labels().enter.to_string(),
                    current_labels().confirm.to_string(),
                ),
                ("Esc".to_string(), current_labels().back.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ],
            team_writer,
        }
    }

    pub fn edit(team: TeamEntry, player: PlayerEntry, team_writer: Arc<TW>) -> Self {
        let role = Select::new(
            current_labels().role.to_owned(),
            RoleEnum::ALL.to_vec(),
            player.role,
            false,
        );
        let name = TextBox::new(current_labels().name.to_owned(), true, Some(&player.name));
        let number = TextBox::with_validator(
            current_labels().number.to_owned(),
            false,
            Some(&player.number.to_string()),
            validate_player_number,
        );
        EditPlayerScreen {
            team,
            name,
            number,
            role,
            field: 0,
            notify_message: NotifyBanner::new(),
            existing_player: Some(player),
            back: false,
            header: TeamHeader::default(),
            footer: NavigationFooter::new(),
            footer_entries: vec![
                (
                    "Tab / Shift+Tab".to_string(),
                    current_labels().navigate.to_string(),
                ),
                (
                    current_labels().enter.to_string(),
                    current_labels().confirm.to_string(),
                ),
                ("Esc".to_string(), current_labels().back.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ],
            team_writer,
        }
    }

    fn handle_error_reset(&mut self) -> AppAction {
        self.notify_message.reset();
        if self.back {
            AppAction::Back(true, Some(1))
        } else {
            AppAction::None
        }
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        self.name.handle_char(c);
        self.number.handle_char(c);
        AppAction::None
    }

    fn is_player_number_available(&self, number: u8) -> bool {
        let existing_id = self.existing_player.as_ref().map(|ep| ep.id);
        !self
            .team
            .active_players()
            .iter()
            .any(|p| p.number == number && Some(p.id) != existing_id)
    }

    async fn handle_enter(&mut self) -> AppAction {
        match (
            self.name.get_selected_value(),
            self.role.get_selected_value(),
            self.number
                .get_selected_value()
                .and_then(|v| v.parse::<u8>().ok()),
        ) {
            (None, _, _) => {
                self.notify_message
                    .set_error(current_labels().name_cannot_be_empty.to_string());
                AppAction::None
            }
            (_, None, _) => {
                self.notify_message
                    .set_error(current_labels().role_is_required.to_string());
                AppAction::None
            }
            (Some(name), Some(role), Some(number)) => {
                if self.is_player_number_available(number) {
                    let input = match &self.existing_player {
                        Some(player) => {
                            let mut updated = player.clone();
                            updated.name = name;
                            updated.role = Some(role);
                            updated.number = number;
                            PlayerInput::Existing(updated)
                        }
                        None => PlayerInput::New { name, role, number },
                    };
                    match self.team_writer.save_player(input, &mut self.team).await {
                        Ok(_) => {
                            self.back = true;
                            self.notify_message
                                .set_info(current_labels().operation_successful.to_string());
                            AppAction::None
                        }
                        Err(_) => {
                            self.notify_message
                                .set_error(current_labels().could_not_create_player.to_string());
                            AppAction::None
                        }
                    }
                } else {
                    self.notify_message
                        .set_error(current_labels().number_already_in_use.to_string());
                    AppAction::None
                }
            }
            (_, _, None) => {
                self.notify_message
                    .set_error(current_labels().number_must_be_between_0_and_99.to_string());
                AppAction::None
            }
        }
    }

    fn handle_tab(&mut self) -> AppAction {
        self.field = (self.field + 1) % 3;
        self.update_writing_modes();
        AppAction::None
    }

    fn handle_backtab(&mut self) -> AppAction {
        self.field = (self.field + 2) % 3;
        self.update_writing_modes();
        AppAction::None
    }

    fn update_writing_modes(&mut self) {
        self.name.writing_mode = self.field == 0;
        self.role.writing_mode = self.field == 1;
        self.number.writing_mode = self.field == 2;
    }

    fn handle_up(&mut self) -> AppAction {
        self.role.handle_up();
        AppAction::None
    }

    fn handle_down(&mut self) -> AppAction {
        self.role.handle_down();
        AppAction::None
    }

    fn handle_backspace(&mut self) -> AppAction {
        self.name.handle_backspace();
        self.number.handle_backspace();
        AppAction::None
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(match self.existing_player {
                Some(_) => current_labels().edit_player,
                None => current_labels().new_player,
            });
        f.render_widget(block, area);
    }
}
