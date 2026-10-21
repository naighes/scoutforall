use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Padding, Paragraph},
};

/// The space between two entries on a line.
const SEPARATOR: &str = "   ";
/// The column left empty before the entries.
const LEFT_PADDING: u16 = 1;

#[derive(Debug, Clone)]
pub struct NavigationFooter {}

impl NavigationFooter {
    pub fn new() -> Self {
        Self {}
    }
}

/// The entries `key = description` as they go on the lines of a footer as
/// wide as `width`: as many on a line as fit, then on the next one; an
/// entry wider than the footer gets a line of its own. The width is the one
/// shown on screen, not the bytes (`⬆` takes one column).
fn footer_lines(entries: &[(String, String)], width: u16) -> Vec<Vec<&(String, String)>> {
    let width = width.saturating_sub(LEFT_PADDING) as usize;
    let entry_width = |(key, value): &(String, String)| {
        Line::from(vec![
            Span::raw(key.as_str()),
            Span::raw(" = "),
            Span::raw(value.as_str()),
        ])
        .width()
    };
    entries
        .iter()
        .fold(Vec::<(Vec<_>, usize)>::new(), |mut lines, entry| {
            let entry_width = entry_width(entry);
            match lines.last_mut() {
                Some((line, line_width))
                    if *line_width + SEPARATOR.len() + entry_width <= width =>
                {
                    line.push(entry);
                    *line_width += SEPARATOR.len() + entry_width;
                }
                _ => lines.push((vec![entry], entry_width)),
            }
            lines
        })
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

impl NavigationFooter {
    /// The lines the footer takes to list all the entries in `width`
    /// columns.
    pub fn height(entries: &[(String, String)], width: u16) -> u16 {
        footer_lines(entries, width).len() as u16
    }

    pub fn render(&self, f: &mut Frame, area: Rect, entries: Vec<(String, String)>) {
        let text: Vec<Line> = footer_lines(&entries, area.width)
            .into_iter()
            .map(|line| {
                let spans: Vec<Span> = line
                    .into_iter()
                    .enumerate()
                    .flat_map(|(i, (key, value))| {
                        [
                            Span::raw(if i == 0 { "" } else { SEPARATOR }),
                            Span::styled(key.clone(), Style::default().fg(Color::Cyan)),
                            Span::raw(" = "),
                            Span::styled(value.clone(), Style::default().fg(Color::White)),
                        ]
                    })
                    .collect();
                Line::from(spans)
            })
            .collect();
        let paragraph = Paragraph::new(Text::from(text)).block(
            Block::default()
                .borders(Borders::NONE)
                .padding(Padding::new(LEFT_PADDING, 0, 0, 0)),
        );
        f.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn entries_go_on_the_next_line_when_they_dont_fit() {
        // "a = one   b = two" is 17 columns, plus the padding
        let e = entries(&[("a", "one"), ("b", "two"), ("c", "three")]);
        assert_eq!(NavigationFooter::height(&e, 18), 2);
        assert_eq!(NavigationFooter::height(&e, 17), 3);
        assert_eq!(NavigationFooter::height(&e, 100), 1);
    }

    #[test]
    fn symbols_take_the_columns_they_show_not_their_bytes() {
        // "⬆ = up" is 6 columns but 8 bytes: both fit in 6 + 3 + 6 columns
        let e = entries(&[("⬆", "up"), ("⬇", "do")]);
        assert_eq!(NavigationFooter::height(&e, 16), 1);
        assert_eq!(NavigationFooter::height(&e, 15), 2);
    }

    #[test]
    fn an_entry_wider_than_the_footer_takes_a_line() {
        let e = entries(&[("a", "a very long description"), ("b", "two")]);
        assert_eq!(NavigationFooter::height(&e, 10), 2);
    }

    #[test]
    fn no_entries_no_lines() {
        assert_eq!(NavigationFooter::height(&[], 80), 0);
    }
}
