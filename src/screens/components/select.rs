use crate::screens::{components::panel, screen::cycle};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{List, ListItem, ListState, Paragraph},
};
use scout4all_core::{localization::current_labels, shapes::enums::FriendlyName};

#[derive(Debug)]
pub struct Select<T>
where
    T: FriendlyName + Clone + PartialEq,
{
    selection: ListState,
    value: Option<T>,
    pub writing_mode: bool,
    label: String,
    values: Vec<T>,
}

impl<T: FriendlyName + Clone + PartialEq> Select<T> {
    pub fn new(label: String, values: Vec<T>, value: Option<T>, writing_mode: bool) -> Self {
        let mut selection = ListState::default();
        let value_clone = value.clone();
        let selected_value = match value_clone {
            None => values.first().cloned(),
            Some(ref v) => values.iter().find(|&x| *x == *v).cloned(),
        };
        selection.select(match (values.is_empty(), &value) {
            (true, _) => None,
            (false, None) => Some(0),
            (false, Some(v)) => values.iter().position(|x| x == v),
        });
        Self {
            selection,
            value: selected_value,
            writing_mode,
            label,
            values,
        }
    }

    pub fn get_selected_value(&self) -> Option<T> {
        self.value.clone()
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        if self.writing_mode {
            self.render_writing(f, area);
        } else {
            self.render_reading(f, area);
        }
    }

    fn render_reading(&mut self, f: &mut Frame, area: Rect) {
        let line = if let Some(value) = self.value.clone() {
            Line::from(vec![
                Span::styled(
                    format!("{}: ", self.label),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(value.friendly_name(current_labels())),
            ])
        } else {
            Line::from(vec![Span::styled(
                format!("{}:", self.label),
                Style::default().add_modifier(Modifier::BOLD),
            )])
        };
        let widget = Paragraph::new(Text::from(line)).style(Style::default());
        f.render_widget(widget, area);
    }

    fn render_writing(&mut self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .values
            .iter()
            .map(|entry| ListItem::new(entry.friendly_name(current_labels())))
            .collect();
        let list = List::new(items)
            .block(panel::section(self.label.clone()))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut self.selection);
    }

    pub fn handle_up(&mut self) {
        self.move_selection(false);
    }

    pub fn handle_down(&mut self) {
        self.move_selection(true);
    }

    /// Selects the value after (`forward`) or before the selected one,
    /// going round, or the first one with none selected.
    fn move_selection(&mut self, forward: bool) {
        let target = match self.selection.selected() {
            Some(selected) => cycle(selected, self.values.len(), forward),
            None => Some(0),
        };
        if let (true, Some(i), Some(value)) = (
            self.writing_mode,
            target,
            target.and_then(|i| self.values.get(i)),
        ) {
            self.value = Some(value.clone());
            self.selection.select(Some(i));
        }
    }
}
