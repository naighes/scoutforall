use crate::{
    ops::create_match,
    screens::{
        screen::{AppAction, Screen},
        start_set_screen::StartSetScreen,
    },
    shapes::team::TeamEntry,
};
use chrono::{DateTime, FixedOffset, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Padding, Paragraph},
    Frame,
};

#[derive(Debug)]
pub struct AddMatchScreen {
    team: TeamEntry,
    opponent: String, // field 0
    year: String,     // field 1
    month: String,    // field 1
    day: String,      // field 1
    home: bool,       // field 2
    field: usize,
    error: Option<String>,
}

impl Screen for AddMatchScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, &self.error) {
            (_, Some(_)) => {
                self.error = None;
                AppAction::None
            }
            (KeyCode::Char(c), _) => match self.field {
                0 => {
                    self.opponent.push(c);
                    AppAction::None
                }
                1 => self.handle_date_input(c),
                2 => self.handle_home_input(c),
                _ => AppAction::None,
            },
            (KeyCode::Backspace, _) => match self.field {
                0 => {
                    self.opponent.pop();
                    AppAction::None
                }
                1 => self.handle_date_backspace(),
                _ => AppAction::None,
            },
            (KeyCode::Tab, _) => {
                self.on_date_input_leave();
                self.field = (self.field + 1) % 3;
                AppAction::None
            }
            (KeyCode::BackTab, _) => {
                self.on_date_input_leave();
                if self.field == 0 {
                    self.field = 2;
                } else {
                    self.field -= 1;
                }
                AppAction::None
            }
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _) => self.handle_submit(),
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
        self.render_opponent_widget(f, inner[0]);
        self.render_date_widget(f, inner[1]);
        self.render_home_widget(f, inner[2]);
        self.render_footer(f, footer_left);
    }
}

impl AddMatchScreen {
    pub fn new(team: TeamEntry) -> Self {
        AddMatchScreen {
            team,
            opponent: String::new(),
            year: String::new(),
            month: String::new(),
            day: String::new(),
            home: false,
            field: 0,
            error: None,
        }
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title("new match");
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

    fn render_home_widget(&self, f: &mut Frame, area: Rect) {
        let home_widget =
            Paragraph::new(format!("home: {}", if self.home { "[X]" } else { "[ ]" },)).style(
                if self.field == 2 {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                },
            );
        f.render_widget(home_widget, area);
    }

    fn render_opponent_widget(&self, f: &mut Frame, area: Rect) {
        let name_widget =
            Paragraph::new(format!("opponent: {}", self.opponent)).style(if self.field == 0 {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            });
        f.render_widget(name_widget, area);
    }

    fn render_date_widget(&self, f: &mut Frame, container: Rect) {
        let date_text = if self.year.len() < 4 {
            let spaces = 4 - self.year.len();
            format!(
                "date (yyyy-mm-dd): {}{}-__-__",
                self.year,
                "_".repeat(spaces)
            )
        } else if self.month.len() < 2 {
            let spaces = 2 - self.month.len();
            format!(
                "date (yyyy-mm-dd): {}-{}{}-__",
                self.year,
                self.month,
                "_".repeat(spaces)
            )
        } else {
            let spaces = 2 - self.day.len();
            format!(
                "date (yyyy-mm-dd): {}-{}-{}{}",
                self.year,
                self.month,
                self.day,
                "_".repeat(spaces)
            )
        };
        let date_widget = Paragraph::new(date_text).style(if self.field == 1 {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        });
        f.render_widget(date_widget, container);
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

    fn days_in_month(&self, year: i32, month: u32) -> Option<u32> {
        match month {
            1 => Some(31),
            2 => {
                if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                    Some(29)
                } else {
                    Some(28)
                }
            }
            3 => Some(31),
            4 => Some(30),
            5 => Some(31),
            6 => Some(30),
            7 => Some(31),
            8 => Some(31),
            9 => Some(30),
            10 => Some(31),
            11 => Some(30),
            12 => Some(31),
            _ => None,
        }
    }

    fn push_month(&mut self, c: char) {
        match self.month.len() {
            0 => match c {
                '0' | '1' => self.month.push(c),
                '2'..='9' => {
                    self.month.push('0');
                    self.month.push(c);
                }
                _ => {}
            },
            1 => {
                let first = self.month.chars().next().unwrap();
                match first {
                    '0' => {
                        if ('1'..='9').contains(&c) {
                            self.month.push(c);
                        }
                    }
                    '1' => {
                        if ('0'..='2').contains(&c) {
                            self.month.push(c);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn push_day(&mut self, c: char) {
        let year: i32 = self.year.parse().unwrap_or(0);
        let month: u32 = self.month.parse().unwrap_or(0);

        if let Some(max_days) = self.days_in_month(year, month) {
            match self.day.len() {
                0 => match c {
                    '0' | '1' | '2' => self.day.push(c),
                    '3' if max_days == 30 => self.day.push_str("30"),
                    '3' if max_days >= 31 => self.day.push('3'),
                    '3' => self.day.push_str("03"),
                    _ => self.day.push_str(&format!("0{}", c)),
                },
                1 => {
                    let value = format!("{}{}", self.day, c);
                    if value
                        .parse::<u32>()
                        .map_or(false, |val| (1..=max_days).contains(&val))
                    {
                        self.day.push(c);
                    }
                }
                _ => {}
            }
        }
    }

    fn parse_date(&self, input: &str) -> Result<DateTime<FixedOffset>, Box<dyn std::error::Error>> {
        let date = NaiveDate::parse_from_str(input, "%Y-%m-%d")?;
        let naive_datetime = date.and_hms_opt(0, 0, 0).ok_or("invalid time")?;
        let offset = FixedOffset::east_opt(0).ok_or("invalid offset")?;
        match naive_datetime.and_local_timezone(offset) {
            chrono::LocalResult::Single(dt) => Ok(dt),
            _ => Err("ambiguous or impossible datetime".into()),
        }
    }

    fn on_date_input_leave(&mut self) {
        if self.field == 1 && (self.year.len() != 4 || self.month.len() != 2 || self.day.len() != 2)
        {
            self.year.clear();
            self.month.clear();
            self.day.clear();
        }
    }

    fn handle_date_backspace(&mut self) -> AppAction {
        if self.day.len() > 0 {
            self.day.pop();
        } else if self.month.len() > 0 {
            self.month.pop();
        } else {
            self.year.pop();
        };
        AppAction::None
    }

    fn handle_date_input(&mut self, c: char) -> AppAction {
        if c.is_ascii_digit() {
            if self.year.len() < 4 {
                if !(self.year.is_empty() && c == '0') {
                    self.year.push(c);
                }
            } else if self.month.len() < 2 {
                self.push_month(c);
            } else if self.day.len() < 2 {
                self.push_day(c);
            }
        };
        AppAction::None
    }

    fn handle_home_input(&mut self, c: char) -> AppAction {
        if c == ' ' {
            self.home = !self.home;
        }
        AppAction::None
    }

    fn handle_submit(&mut self) -> AppAction {
        match (
            self.year.len(),
            self.month.len(),
            self.day.len(),
            self.parse_date(&format!("{}-{}-{}", self.year, self.month, self.day).to_string()),
            self.opponent.is_empty(),
        ) {
            (_, __, _, _, true) => {
                self.error = Some("opponent cannot be empty".to_string());
                AppAction::None
            }
            (4, 2, 2, Ok(date), _) => {
                match create_match(&self.team, self.opponent.clone(), date, self.home) {
                    Ok(m) => {
                        AppAction::SwitchScreen(Box::new(StartSetScreen::new(m, 1, None, Some(2))))
                    }
                    Err(_) => {
                        self.error = Some("could not create match".to_string());
                        AppAction::None
                    }
                }
            }
            _ => {
                self.error = Some("invalid date".into());
                AppAction::None
            }
        }
    }
}
