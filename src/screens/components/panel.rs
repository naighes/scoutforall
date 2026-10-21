//! The bordered panels of the screens, so that they all look the same: the
//! panel naming the screen has its title in yellow, the others in bold.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Paragraph},
};

pub const SCREEN_TITLE: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
pub const SECTION_TITLE: Style = Style::new().add_modifier(Modifier::BOLD);

/// The panel naming the screen: a team list, a form, the events of a set.
pub fn screen<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::bordered().title(title.into().patch_style(SCREEN_TITLE))
}

/// Any other panel of the screen.
pub fn section<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::bordered().title(title.into().patch_style(SECTION_TITLE))
}

/// The message of an empty list, centered in the area.
pub fn render_empty(f: &mut Frame, area: Rect, message: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(3),
            Constraint::Percentage(40),
        ])
        .split(area);
    let paragraph = Paragraph::new(message).alignment(Alignment::Center);
    f.render_widget(paragraph, chunks[1]);
}
