use crate::{
    localization::current_labels,
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
        match (key.code, &self.error) {
            (_, Some(_)) => self.handle_error_reset(),
            (KeyCode::Char(c), _) => self.handle_char(c),
            (KeyCode::Backspace, _) => self.handle_backspace(),
            (KeyCode::Tab, _) => self.handle_tab(),
            (KeyCode::BackTab, _) => self.handle_backtab(),
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _) => self.handle_enter(),
            _ => AppAction::None,
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(current_labels().new_team);
        f.render_widget(block, body);
        let container = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(1)])
            .split(body);
        let field_height = 3;
        let mut y_offset = container[0].y;
        for (label, value, idx) in vec![
            (current_labels().name, &self.name, 0),
            (current_labels().league, &self.league, 1),
            (current_labels().year, &self.year, 2),
        ]
        .into_iter()
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
        self.render_error(f, footer_right);
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

    fn handle_error_reset(&mut self) -> AppAction {
        self.error = None;
        AppAction::None
    }

    fn handle_tab(&mut self) -> AppAction {
        self.field = (self.field + 1) % 3;
        AppAction::None
    }

    fn handle_backtab(&mut self) -> AppAction {
        if self.field == 0 {
            self.field = 2;
        } else {
            self.field -= 1;
        }
        AppAction::None
    }

    fn handle_backspace(&mut self) -> AppAction {
        match self.field {
            0 => {
                self.name.pop();
            }
            1 => {
                self.league.pop();
            }
            2 => {
                self.year.pop();
            }
            _ => {}
        };
        AppAction::None
    }

    fn handle_enter(&mut self) -> AppAction {
        match (
            self.name.is_empty(),
            self.league.is_empty(),
            self.year.parse::<u16>(),
        ) {
            (true, _, _) => {
                self.error = Some(current_labels().name_cannot_be_empty.to_string());
                AppAction::None
            }
            (_, true, _) => {
                self.error = Some(current_labels().league_cannot_be_empty.to_string());
                AppAction::None
            }
            (_, _, Ok(year)) => match create_team(self.name.clone(), self.league.clone(), year) {
                Ok(_) => AppAction::Back(true, Some(1)),
                Err(_) => {
                    self.error = Some(current_labels().could_not_create_team.to_string());
                    AppAction::None
                }
            },
            (_, _, Err(_)) => {
                self.error = Some(
                    current_labels()
                        .year_must_be_a_four_digit_number
                        .to_string(),
                );
                AppAction::None
            }
        }
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        match (self.field, c.is_ascii_digit()) {
            (0, _) => {
                self.name.push(c);
            }
            (1, _) => {
                self.league.push(c);
            }
            (2, true) => {
                if self.year.len() < 4 {
                    self.year.push(c);
                }
            }
            _ => {}
        };
        AppAction::None
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

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph = Paragraph::new(format!(
            "Tab / Shift+Tab | Enter = {} | Esc = {} | Q = {}",
            current_labels().confirm,
            current_labels().back,
            current_labels().quit
        ))
        .block(block);
        f.render_widget(paragraph, area);
    }
}
