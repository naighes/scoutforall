use crate::{localization::current_labels, shapes::enums::FriendlyName};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

#[derive(Debug)]
pub struct Select<T>
where
    T: FriendlyName + Clone,
{
    selection: ListState,
    value: Option<T>,
    pub writing_mode: bool,
    label: String,
    values: Vec<T>,
}

impl<T: FriendlyName + Clone> Select<T> {
    pub fn new(label: String, values: Vec<T>, writing_mode: bool) -> Self {
        let mut selection = ListState::default();
        if !values.is_empty() {
            selection.select(Some(0));
        }
        Self {
            selection,
            value: values.first().cloned(),
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
            .block(Block::default().borders(Borders::ALL).title(Span::styled(
                self.label.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut self.selection);
    }

    pub fn handle_up(&mut self) {
        match (self.writing_mode, &self.selection.selected()) {
            (true, Some(selected)) => {
                self.select_value(if *selected == 0 {
                    self.values.len() - 1
                } else {
                    selected - 1
                });
            }
            (true, None) => {
                self.select_value(0);
            }
            _ => {}
        };
    }

    pub fn handle_down(&mut self) {
        match (self.writing_mode, self.selection.selected()) {
            (true, Some(selected)) => {
                self.select_value((selected + 1) % self.values.len());
            }
            (true, None) => {
                self.select_value(0);
            }
            _ => {}
        };
    }

    fn select_value(&mut self, new_selected: usize) {
        self.value = Some(self.values[new_selected].clone());
        self.selection.select(Some(new_selected));
    }
}
