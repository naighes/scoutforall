use crate::{
    ops::create_team,
    screens::screen::{AppAction, Screen},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Padding, Paragraph},
    Frame,
};

#[derive(Debug)]
pub struct AddTeamScreen {
    name: String,
    league: String,
    year: String,
    field: usize,
    error: Option<String>,
}

impl Screen for AddTeamScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        self.error = None;
        match key.code {
            KeyCode::Char(c) => match self.field {
                0 => {
                    self.name.push(c);
                    AppAction::None
                }
                1 => {
                    self.league.push(c);
                    AppAction::None
                }
                2 => {
                    if c.is_ascii_digit() && self.year.len() < 4 {
                        self.year.push(c);
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
                1 => {
                    self.league.pop();
                    AppAction::None
                }
                2 => {
                    self.year.pop();
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
                } else if self.league.is_empty() {
                    self.error = Some("league cannot be empty".to_string());
                    AppAction::None
                } else if let Ok(year) = self.year.parse::<u16>() {
                    match create_team(self.name.clone(), self.league.clone(), year) {
                        Ok(_) => AppAction::Back(true, Some(1)),
                        Err(_) => {
                            self.error = Some("could not create team".to_string());
                            AppAction::None
                        }
                    }
                } else {
                    self.error = Some("year must be a 4-digit number".to_string());
                    AppAction::None
                }
            }
            _ => AppAction::None,
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        self.render_error(f, footer_right);
        let block = Block::default().borders(Borders::ALL).title("new team");
        f.render_widget(block, body);
        let container = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(1)])
            .split(body);
        let field_height = 3;
        let mut y_offset = container[0].y;
        for (_, (label, value, idx)) in vec![
            ("name", &self.name, 0),
            ("league", &self.league, 1),
            ("year", &self.year, 2),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect {
                x: container[0].x,
                y: y_offset,
                width: container[0].width,
                height: field_height,
            };
            y_offset += field_height;
            f.render_widget(
                Paragraph::new(format!("{}: {}", label, value)).style(if self.field == idx {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                }),
                rect,
            );
        }
        self.render_footer(f, footer_left);
    }
}

impl AddTeamScreen {
    pub fn new() -> Self {
        AddTeamScreen {
            name: String::new(),
            league: String::new(),
            year: String::new(),
            field: 0,
            error: None,
        }
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

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph =
            Paragraph::new("Tab / Shift+Tab | Enter = confirm | Esc = cancel | Q = quit")
                .block(block);
        f.render_widget(paragraph, area);
    }
}
