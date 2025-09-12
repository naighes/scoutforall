use crate::{
    ops::create_player,
    screens::screen::{AppAction, Screen},
    shapes::{enums::RoleEnum, team::TeamEntry},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph},
    Frame,
};

#[derive(Debug)]
pub struct AddPlayerScreen {
    team: TeamEntry,
    name: String,
    role: Option<RoleEnum>,
    number: String,
    field: usize,
    role_selection: ListState,
    error: Option<String>,
}

impl Screen for AddPlayerScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if self.error.is_some() {
            self.error = None;
            return AppAction::None;
        }
        match key.code {
            KeyCode::Char(c) => match self.field {
                0 => {
                    self.name.push(c);
                    AppAction::None
                }
                2 => {
                    if c.is_ascii_digit() {
                        if self.number.len() < 2 {
                            if !(self.number.is_empty() && c == '0') {
                                self.number.push(c);
                            }
                        }
                    }
                    AppAction::None
                }
                _ => AppAction::None,
            },
            KeyCode::Backspace => match self.field {
                0 => {
                    self.name.pop();
                    AppAction::None
                }
                2 => {
                    self.number.pop();
                    AppAction::None
                }
                _ => AppAction::None,
            },
            KeyCode::Up => match self.field {
                1 => {
                    if let Some(selected) = self.role_selection.selected() {
                        let new_selected = if selected == 0 {
                            RoleEnum::ALL.len() - 1
                        } else {
                            selected - 1
                        };
                        self.role = Some(RoleEnum::ALL[new_selected]);
                        self.role_selection.select(Some(new_selected));
                    } else {
                        self.role_selection.select(Some(0));
                        self.role = Some(RoleEnum::ALL[0]);
                    }
                    AppAction::None
                }
                _ => AppAction::None,
            },
            KeyCode::Down => match self.field {
                1 => {
                    if let Some(selected) = self.role_selection.selected() {
                        let new_selected = (selected + 1) % RoleEnum::ALL.len();
                        self.role_selection.select(Some(new_selected));
                        self.role = Some(RoleEnum::ALL[new_selected]);
                    } else {
                        self.role_selection.select(Some(0));
                        self.role = Some(RoleEnum::ALL[0]);
                    }
                    AppAction::None
                }
                _ => AppAction::None,
            },
            KeyCode::Tab => {
                self.field = (self.field + 1) % 3;
                AppAction::None
            }
            KeyCode::BackTab => {
                if self.field == 0 {
                    self.field = 2;
                } else {
                    self.field -= 1;
                }
                AppAction::None
            }
            KeyCode::Esc => AppAction::Back(true, Some(1)),
            KeyCode::Enter => {
                if self.name.is_empty() {
                    self.error = Some("name cannot be empty".to_string());
                    AppAction::None
                } else if self.role.is_none() {
                    self.error = Some("role cannot be empty".to_string());
                    AppAction::None
                } else {
                    match (self.number.parse::<u8>(), self.role) {
                        (Ok(number), Some(role)) => {
                            match create_player(self.name.clone(), role, number, &mut self.team) {
                                Ok(_) => AppAction::Back(true, Some(1)),
                                Err(_) => {
                                    self.error = Some("could not create player".to_string());
                                    AppAction::None
                                }
                            }
                        }
                        (Err(_), _) => {
                            self.error = Some("number must be a 4-digit number".into());
                            AppAction::None
                        }
                        (_, None) => {
                            self.error = Some("invalid role".into());
                            AppAction::None
                        }
                    }
                }
            }
            _ => AppAction::None,
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        self.render_error(f, footer_right);
        self.render_header(f, body);
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(1),
            ])
            .split(body);
        let name_widget =
            Paragraph::new(format!("name: {}", self.name)).style(if self.field == 0 {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            });
        f.render_widget(name_widget, inner[0]);
        if self.field == 1 {
            self.render_role_list(f, inner[1]);
        } else {
            self.render_role_widget(f, inner[1]);
        }
        let number_widget =
            Paragraph::new(format!("number: {}", self.number)).style(if self.field == 2 {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            });
        f.render_widget(number_widget, inner[2]);
        self.render_footer(f, footer_left);
    }
}

impl AddPlayerScreen {
    pub fn new(team: TeamEntry) -> Self {
        AddPlayerScreen {
            team,
            name: String::new(),
            role: None,
            number: String::new(),
            field: 0,
            role_selection: ListState::default(),
            error: None,
        }
    }

    fn render_role_widget(&mut self, f: &mut Frame, area: Rect) {
        let role_widget = Paragraph::new(if let Some(role) = self.role {
            format!("role: {}", role)
        } else {
            "role:".into()
        })
        .style(Style::default());
        f.render_widget(role_widget, area);
    }

    fn render_role_list(&mut self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = RoleEnum::ALL
            .iter()
            .map(|r| ListItem::new(r.to_string()))
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("role"))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .highlight_symbol(">> ");
        f.render_stateful_widget(list, area, &mut self.role_selection);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("new player");
        f.render_widget(block, area);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph =
            Paragraph::new("Tab / Shift+Tab = navigate | Enter = confirm | Esc = back | Q = quit")
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
                .block(Block::default().borders(Borders::ALL).title("error"));
            f.render_widget(error_widget, area);
        }
    }
}
