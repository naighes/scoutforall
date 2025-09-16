use crate::{
    localization::current_labels,
    ops::create_player,
    screens::{
        components::{select::Select, text_box::TextBox},
        screen::{AppAction, Screen},
    },
    shapes::{enums::RoleEnum, team::TeamEntry},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Padding, Paragraph},
    Frame,
};

#[derive(Debug)]
pub struct AddPlayerScreen {
    team: TeamEntry,
    name: TextBox,
    number: TextBox,
    role: Select<RoleEnum>,
    field: usize,
    error: Option<String>,
}

impl Screen for AddPlayerScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, &self.error) {
            (_, Some(_)) => self.handle_error_reset(),
            (KeyCode::Char(c), _) => self.handle_char(c),
            (KeyCode::Backspace, _) => self.handle_backspace(),
            (KeyCode::Up, _) => self.handle_up(),
            (KeyCode::Down, _) => self.handle_down(),
            (KeyCode::Tab, _) => self.handle_tab(),
            (KeyCode::BackTab, _) => self.handle_backtab(),
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _) => self.handle_enter(),
            _ => AppAction::None,
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let area = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // name
                Constraint::Length(3), // role
                Constraint::Length(3), // number
                Constraint::Min(1),
            ])
            .split(body);
        self.render_error(f, footer_right);
        self.render_header(f, body);
        self.name.render(f, area[0]);
        self.role.render(f, area[1]);
        self.number.render(f, area[2]);
        self.render_footer(f, footer_left);
    }
}

impl AddPlayerScreen {
    pub fn new(team: TeamEntry) -> Self {
        let role = Select::new(
            current_labels().role.to_owned(),
            RoleEnum::ALL.to_vec(),
            false,
        );
        let name = TextBox::new(current_labels().name.to_owned(), true);
        let number = TextBox::with_validator(
            current_labels().number.to_owned(),
            false,
            |current: &str, c: char| {
                c.is_ascii_digit() && current.len() < 2 && !(current.is_empty() && c == '0')
            },
        );
        AddPlayerScreen {
            team,
            name,
            number,
            role,
            field: 0,
            error: None,
        }
    }

    fn handle_error_reset(&mut self) -> AppAction {
        self.error = None;
        AppAction::None
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        self.name.handle_char(c);
        self.number.handle_char(c);
        AppAction::None
    }

    fn handle_enter(&mut self) -> AppAction {
        match (
            self.name.get_selected_value(),
            self.role.get_selected_value(),
            self.number.get_selected_value().map(|v| v.parse::<u8>()),
        ) {
            (None, _, _) => {
                self.error = Some(current_labels().name_cannot_be_empty.to_string());
                AppAction::None
            }
            (_, None, _) => {
                self.error = Some(current_labels().role_is_required.to_string());
                AppAction::None
            }
            (Some(name), Some(role), Some(Ok(number))) => {
                match create_player(name, role, number, &mut self.team) {
                    Ok(_) => AppAction::Back(true, Some(1)),
                    Err(_) => {
                        self.error = Some(current_labels().could_not_create_player.to_string());
                        AppAction::None
                    }
                }
            }
            (_, _, None | Some(Err(_))) => {
                self.error = Some(current_labels().number_must_be_between_0_and_99.into());
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
            .title(current_labels().new_player);
        f.render_widget(block, area);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph = Paragraph::new(format!(
            "Tab / Shift+Tab = {} | Enter = {} | Esc = {} | Q = {}",
            current_labels().navigate,
            current_labels().confirm,
            current_labels().back,
            current_labels().quit
        ))
        .block(block);
        f.render_widget(paragraph, area);
    }

    fn render_error(&self, f: &mut Frame, area: Rect) {
        if let Some(err) = &self.error {
            let error_widget = Paragraph::new(err.clone())
                .style(
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(current_labels().error),
                );
            f.render_widget(error_widget, area);
        }
    }
}
