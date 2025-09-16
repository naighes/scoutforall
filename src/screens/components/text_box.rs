use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::Paragraph,
    Frame,
};
use std::fmt::Debug;
use std::fmt::Formatter;

pub struct TextBox {
    value: String,
    pub writing_mode: bool,
    label: String,
    pub validator: Box<dyn Fn(&str, char) -> bool>,
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
    pub fn new(label: String, writing_mode: bool) -> Self {
        Self {
            value: String::new(),
            writing_mode,
            label,
            validator: Box::new(|_, _| true),
        }
    }

    pub fn with_validator<F>(label: String, writing_mode: bool, validator: F) -> Self
    where
        F: Fn(&str, char) -> bool + 'static,
    {
        Self {
            value: String::new(),
            writing_mode,
            label,
            validator: Box::new(validator),
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let widget =
            Paragraph::new(format!("{}: {}", self.label, self.value)).style(if self.writing_mode {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            });
        f.render_widget(widget, area);
    }

    pub fn handle_char(&mut self, c: char) {
        if self.writing_mode && self.validator.as_ref()(&self.value, c) {
            self.value.push(c);
        }
    }

    pub fn handle_backspace(&mut self) {
        if self.writing_mode {
            self.value.pop();
        }
    }

    pub fn get_selected_value(&self) -> Option<String> {
        self.value.clone().into()
    }
}
