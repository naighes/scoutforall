use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
    Frame,
};

#[derive(Debug, Clone)]
pub struct NavigationFooter {}

impl NavigationFooter {
    pub fn new() -> Self {
        Self {}
    }
}

impl NavigationFooter {
    pub fn render(&self, f: &mut Frame, area: Rect, entries: Vec<(String, String)>) {
        let mut text = Vec::new();
        let mut current_line = Vec::new();
        let mut width = 0;
        let max_width = area.width as usize;
        for (label, value) in entries {
            let span_len = label.len() + value.len() + 3; // " = "
            if width + span_len > max_width {
                text.push(Line::from(current_line));
                current_line = Vec::new();
                width = 0;
            }
            current_line.push(Span::styled(
                label.clone(),
                Style::default().fg(Color::Cyan),
            ));
            current_line.push(Span::raw(" = "));
            current_line.push(Span::styled(
                value.clone(),
                Style::default().fg(Color::White),
            ));
            current_line.push(Span::raw("   "));
            width += span_len + 3;
        }
        if !current_line.is_empty() {
            text.push(Line::from(current_line));
        }
        let paragraph = Paragraph::new(Text::from(text))
            .block(
                Block::default()
                    .borders(Borders::NONE)
                    .padding(Padding::new(1, 0, 0, 0)),
            )
            .wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
    }
}
