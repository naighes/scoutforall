use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::Paragraph,
    Frame,
};
use std::{
    fmt::{Debug, Formatter},
    time::{Duration, Instant},
};

pub struct TextBox {
    value: String,
    pub writing_mode: bool,
    label: String,
    pub validator: Box<dyn Fn(&str, char) -> bool + Send + Sync + 'static>,
    pub multiline: bool,
    last_blink: Instant,
    show_cursor: bool,
}

impl Debug for TextBox {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextBox")
            .field("value", &self.value)
            .field("writing_mode", &self.writing_mode)
            .field("label", &self.label)
            .finish()
    }
}

impl TextBox {
    pub fn new(label: String, writing_mode: bool, value: Option<&str>) -> Self {
        Self {
            value: value.unwrap_or_default().to_string(),
            writing_mode,
            label,
            validator: Box::new(|_, _| true),
            multiline: false,
            last_blink: Instant::now(),
            show_cursor: true,
        }
    }

    pub fn enable_multiline(mut self, enabled: bool) -> Self {
        self.multiline = enabled;
        self
    }

    pub fn with_validator<F>(
        label: String,
        writing_mode: bool,
        value: Option<&str>,
        validator: F,
    ) -> Self
    where
        F: Fn(&str, char) -> bool + Send + Sync + 'static,
    {
        Self {
            value: value.unwrap_or_default().to_string(),
            writing_mode,
            label,
            validator: Box::new(validator),
            multiline: false,
            last_blink: Instant::now(),
            show_cursor: true,
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        if self.last_blink.elapsed() >= Duration::from_millis(500) {
            self.show_cursor = !self.show_cursor;
            self.last_blink = Instant::now();
        }
        let mut content = if self.multiline {
            format!("{}:\n{}", self.label, self.value)
        } else {
            format!("{}: {}", self.label, self.value)
        };
        if self.writing_mode && self.show_cursor {
            content.push('█');
        }
        let widget = Paragraph::new(content).style(if self.writing_mode {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        });
        f.render_widget(widget, area);
    }

    pub fn handle_char(&mut self, c: char) {
        if self.writing_mode {
            if c == '\n' && !self.multiline {
                return;
            }
            if (self.validator)(&self.value, c) {
                self.value.push(c);
            }
        }
    }

    pub fn handle_backspace(&mut self) {
        if self.writing_mode {
            self.value.pop();
        }
    }

    pub fn get_selected_value(&self) -> Option<String> {
        Some(self.value.clone())
    }
}
