use crate::{
    localization::current_labels,
    shapes::{enums::FriendlyName, team::TeamEntry},
};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

#[derive(Debug, Clone)]
pub struct TeamHeader {
    pub title_color: Color,
}

impl TeamHeader {
    pub fn new(title_color: Color) -> Self {
        Self { title_color }
    }
}

impl Default for TeamHeader {
    fn default() -> Self {
        Self::new(Color::Cyan)
    }
}

impl TeamHeader {
    pub fn render(&self, f: &mut Frame, area: Rect, team: Option<&TeamEntry>) {
        let header_text: Text = if let Some(team) = team {
            Text::from(vec![
                Line::from(vec![Span::styled(
                    &team.name,
                    Style::default()
                        .fg(self.title_color)
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::from(vec![
                    Span::styled(
                        format!("{}: ", current_labels().team_classification),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(
                        team.classification
                            .map(|c| c.friendly_name(current_labels()))
                            .unwrap_or_default(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        format!("{}: ", current_labels().gender),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(
                        team.gender
                            .map(|g| g.friendly_name(current_labels()))
                            .unwrap_or_default(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        format!("{}: ", current_labels().year),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(team.year.to_string()),
                ]),
            ])
        } else {
            Text::from(vec![Line::from(
                current_labels().team_not_found.to_string(),
            )])
        };
        let header = Paragraph::new(header_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(current_labels().team),
            )
            .alignment(Alignment::Center);

        f.render_widget(header, area);
    }
}
