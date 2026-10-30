use crate::localization::{current_labels, language};
use crate::screens::{components::panel, screen::cycle};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};
use scout4all_core::shapes::country::CountryCode;

/// A country of the ISO list: what's typed narrows it (the name in the
/// language of the app, or the code), the arrows go through what's left.
/// The choice stays while it's among the countries found, else it's the
/// first one; nothing found keeps it.
#[derive(Debug)]
pub struct CountryPicker {
    label: String,
    query: String,
    value: Option<CountryCode>,
    pub writing_mode: bool,
}

impl CountryPicker {
    pub fn new(label: String, value: Option<CountryCode>, writing_mode: bool) -> Self {
        Self {
            label,
            query: String::new(),
            value,
            writing_mode,
        }
    }

    pub fn get_selected_value(&self) -> Option<CountryCode> {
        self.value
    }

    fn matches(&self) -> Vec<(CountryCode, &'static str)> {
        CountryCode::search(&self.query, language().get())
    }

    fn position(&self, matches: &[(CountryCode, &'static str)]) -> Option<usize> {
        matches
            .iter()
            .position(|(code, _)| Some(*code) == self.value)
    }

    pub fn handle_char(&mut self, c: char) {
        if self.writing_mode && !c.is_control() {
            self.query.push(c);
            self.choose_first();
        }
    }

    pub fn handle_backspace(&mut self) {
        if self.writing_mode && self.query.pop().is_some() {
            self.choose_first();
        }
    }

    fn choose_first(&mut self) {
        let matches = self.matches();
        let first = match (self.query.is_empty(), self.position(&matches)) {
            (false, None) => matches.first().map(|(code, _)| *code),
            _ => None,
        };
        self.value = first.or(self.value);
    }

    pub fn handle_up(&mut self) {
        self.move_selection(false);
    }

    pub fn handle_down(&mut self) {
        self.move_selection(true);
    }

    /// The country found after (`forward`) or before the chosen one, going
    /// round; the first one when the chosen one isn't among them.
    fn move_selection(&mut self, forward: bool) {
        let matches = self.matches();
        let target = match self.position(&matches) {
            Some(i) => cycle(i, matches.len(), forward),
            None => Some(0),
        };
        if let (true, Some((code, _))) = (self.writing_mode, target.and_then(|i| matches.get(i))) {
            self.value = Some(*code);
        }
    }

    fn name(&self) -> String {
        self.value
            .map(|code| code.name(language().get()))
            .unwrap_or_default()
    }

    /// One line, as the other fields of a form: label and country; in
    /// writing mode, in a frame, what's typed and the country it finds.
    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let widget = match self.writing_mode {
            false => Paragraph::new(Line::from(vec![
                Span::styled(format!("{}: ", self.label), bold),
                Span::raw(self.name()),
            ])),
            true => {
                Paragraph::new(self.search_line(true)).block(panel::section(self.label.clone()))
            }
        };
        f.render_widget(widget, area);
    }

    /// What's typed (a hint before typing), with the country chosen
    /// (`chosen`) or nothing found.
    fn search_line(&self, chosen: bool) -> Line<'static> {
        let labels = current_labels();
        let found = self.query.is_empty() || !self.matches().is_empty();
        let mut spans = vec![match self.query.is_empty() {
            true => Span::styled(
                labels.type_to_search.to_string(),
                Style::default().add_modifier(Modifier::DIM),
            ),
            false => Span::styled(
                format!("{}█", self.query),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        }];
        spans.extend(
            match (found, chosen) {
                (true, true) => Some(Span::styled(
                    format!("◀ {} ▶", self.name()),
                    Style::default().add_modifier(Modifier::REVERSED),
                )),
                (true, false) => None,
                (false, _) => Some(Span::raw(labels.no_country_found.to_string())),
            }
            .map(|span| [Span::raw("  "), span])
            .into_iter()
            .flatten(),
        );
        Line::from(spans)
    }

    /// The whole list of the countries found, under what's typed, the
    /// chosen one highlighted: for a screen of its own, which has the
    /// label as its title.
    pub fn render_list(&mut self, f: &mut Frame, area: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(area);
        f.render_widget(
            Paragraph::new(self.search_line(false)).block(panel::section(String::new())),
            rows[0],
        );
        let matches = self.matches();
        let mut state = ListState::default().with_selected(self.position(&matches));
        let items: Vec<ListItem> = matches
            .iter()
            .map(|(code, name)| ListItem::new(format!("{name} ({code})")))
            .collect();
        let list = List::new(items)
            .block(panel::section(String::new()))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED))
            .highlight_symbol(">> ");
        f.render_stateful_widget(list, rows[1], &mut state);
    }
}
