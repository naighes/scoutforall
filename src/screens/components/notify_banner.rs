use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::localization::current_labels;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyMessage {
    Error(String),
    Warning(String),
    Info(String),
}

#[derive(Debug)]
pub struct NotifyBanner {
    pub message: Option<NotifyMessage>,
}

impl NotifyBanner {
    pub fn new() -> Self {
        Self { message: None }
    }

    pub fn set_error(&mut self, msg: String) {
        self.message = Some(NotifyMessage::Error(msg));
    }

    pub fn set_info(&mut self, msg: String) {
        self.message = Some(NotifyMessage::Info(msg));
    }

    pub fn set_warning(&mut self, msg: String) {
        self.message = Some(NotifyMessage::Warning(msg));
    }

    pub fn reset(&mut self) {
        self.message = None;
    }

    pub fn has_value(&self) -> bool {
        self.message.is_some()
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if let Some(message) = &self.message {
            let msg = match message {
                NotifyMessage::Info(m) => m,
                NotifyMessage::Error(m) => m,
                NotifyMessage::Warning(m) => m,
            };
            let widget = Paragraph::new(msg.clone())
                .style(
                    Style::default()
                        .fg(Color::White)
                        .bg(match message {
                            NotifyMessage::Info(_) => Color::Blue,
                            NotifyMessage::Error(_) => Color::Red,
                            NotifyMessage::Warning(_) => Color::Yellow,
                        })
                        .add_modifier(Modifier::BOLD),
                )
                .block(Block::default().borders(Borders::ALL).title(match message {
                    NotifyMessage::Info(_) => current_labels().info,
                    NotifyMessage::Error(_) => current_labels().error,
                    NotifyMessage::Warning(_) => current_labels().warning,
                }));
            f.render_widget(widget, area);
        }
    }
}
