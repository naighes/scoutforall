use chrono::{DateTime, FixedOffset, NaiveDate};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::Paragraph,
    Frame,
};

use crate::errors::{AppError, IOError};

#[derive(Debug)]
pub struct DatePicker {
    year: String,
    month: String,
    day: String,
    label: String,
    pub writing_mode: bool,
}

impl DatePicker {
    pub fn new(label: String, writing_mode: bool) -> Self {
        Self {
            year: String::new(),
            month: String::new(),
            day: String::new(),
            label,
            writing_mode,
        }
    }

    pub fn render(&self, f: &mut Frame, container: Rect) {
        let text = if self.year.len() < 4 {
            let spaces = 4 - self.year.len();
            format!(
                "{} (yyyy-mm-dd): {}{}-__-__",
                self.label,
                self.year,
                "_".repeat(spaces)
            )
        } else if self.month.len() < 2 {
            let spaces = 2 - self.month.len();
            format!(
                "{} (yyyy-mm-dd): {}-{}{}-__",
                self.label,
                self.year,
                self.month,
                "_".repeat(spaces)
            )
        } else {
            let spaces = 2 - self.day.len();
            format!(
                "{} (yyyy-mm-dd): {}-{}-{}{}",
                self.label,
                self.year,
                self.month,
                self.day,
                "_".repeat(spaces)
            )
        };
        let widget = Paragraph::new(text).style(if self.writing_mode {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        });
        f.render_widget(widget, container);
    }

    pub fn handle_backspace(&mut self) {
        if !self.writing_mode {
            return;
        }
        if !self.day.is_empty() {
            self.day.pop();
        } else if !self.month.is_empty() {
            self.month.pop();
        } else {
            self.year.pop();
        }
    }

    pub fn handle_char(&mut self, c: char) {
        if !self.writing_mode {
            return;
        }
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
                        .is_ok_and(|val| (1..=max_days).contains(&val))
                    {
                        self.day.push(c);
                    }
                }
                _ => {}
            }
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

    pub fn handle_tab(&mut self) {
        if !self.writing_mode {
            return;
        }
        if self.get_selected_value().is_err() {
            self.year.clear();
            self.month.clear();
            self.day.clear();
        }
    }

    pub fn get_selected_value(&self) -> Result<DateTime<FixedOffset>, AppError> {
        let str = format!("{}-{}-{}", self.year, self.month, self.day);
        let date = NaiveDate::parse_from_str(&str, "%Y-%m-%d")
            .map_err(|e| AppError::IO(IOError::Msg(format!("failed to parse date: {}", e))))?;
        let naive_datetime = date
            .and_hms_opt(0, 0, 0)
            .ok_or("invalid time")
            .map_err(|e| {
                AppError::IO(IOError::Msg(format!(
                    "failed to create naive datetime: {}",
                    e
                )))
            })?;
        let offset = FixedOffset::east_opt(0)
            .ok_or("invalid offset")
            .map_err(|e| {
                AppError::IO(IOError::Msg(format!(
                    "failed to create fixed offset: {}",
                    e
                )))
            })?;
        match naive_datetime.and_local_timezone(offset) {
            chrono::LocalResult::Single(dt) => Ok(dt),
            _ => Err(AppError::IO(IOError::Msg(
                "ambiguous or impossible datetime".to_string(),
            ))),
        }
    }
}
