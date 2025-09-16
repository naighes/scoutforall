use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::Paragraph,
    Frame,
};

#[derive(Debug)]
pub struct CheckBox {
    value: bool,
    pub writing_mode: bool,
    label: String,
}

impl CheckBox {
    pub fn new(label: String, writing_mode: bool) -> Self {
        Self {
            writing_mode,
            label,
            value: false,
        }
    }

    pub fn get_selected_value(&self) -> bool {
        self.value
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let widget = Paragraph::new(format!(
            "{}: {}",
            self.label,
            if self.value { "[X]" } else { "[ ]" },
        ))
        .style(if self.writing_mode {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        });
        f.render_widget(widget, area);
    }

    pub fn handle_char(&mut self, c: char) {
        if self.writing_mode && c == ' ' {
            self.value = !self.value;
        }
    }
}
