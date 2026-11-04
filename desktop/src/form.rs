//! A form, as in the TUI: ↑ or ⇧Tab to the field before, ↓ or Tab to the
//! next one, ← → change a choice, Enter saves, Esc cancels. The text fields
//! take the focus as the keys move it (see `focus_id`).

use {
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length, Padding,
        widget::{Column, Id, button, column, container, row, text, text_input},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
};

#[derive(Debug, Clone)]
pub enum Value {
    Text(String),
    /// one of `options`, ← → change it
    Choice {
        options: Vec<String>,
        index: usize,
    },
    /// on or off, ← → or space switch it; fixed when it can't change
    Check {
        on: bool,
        fixed: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Field {
    pub label: String,
    pub value: Value,
    /// what to write, below the field
    pub hint: Option<String>,
}

impl Field {
    pub fn text(label: &str, value: impl Into<String>) -> Field {
        Field {
            label: label.to_string(),
            value: Value::Text(value.into()),
            hint: None,
        }
    }

    pub fn choice(label: &str, options: Vec<String>, index: usize) -> Field {
        Field {
            label: label.to_string(),
            value: Value::Choice { options, index },
            hint: None,
        }
    }

    pub fn check(label: &str, on: bool, fixed: bool) -> Field {
        Field {
            label: label.to_string(),
            value: Value::Check { on, fixed },
            hint: None,
        }
    }

    pub fn hint(self, hint: &str) -> Field {
        Field {
            hint: Some(hint.to_string()),
            ..self
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Input(usize, String),
    Choose(usize, usize),
    Focus(usize),
    Save,
    Cancel,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Stay,
    /// the value of the field changed: the screen may recompute others
    Changed(usize),
    Save,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub focused: usize,
    /// why it can't be saved, as the core says
    pub error: Option<String>,
    /// a title before the field, for a group of them
    pub sections: Vec<(usize, String)>,
    /// the buttons, when not to cancel and to save: Esc's, then Enter's
    pub actions: Option<(String, String)>,
}

/// The focus on nothing: on a choice, no text field takes the keys.
const NO_FIELD: &str = "form-no-field";

fn is_previous(ck: KeyCombination) -> bool {
    ck == key!(up) || ck == key!(shift - tab) || ck == key!(shift - backtab) || ck == key!(backtab)
}

fn is_next(ck: KeyCombination) -> bool {
    ck == key!(down) || ck == key!(tab)
}

impl Form {
    pub fn new(title: &str, fields: Vec<Field>) -> Form {
        Form {
            title: title.to_string(),
            fields,
            focused: 0,
            error: None,
            sections: vec![],
            actions: None,
        }
    }

    pub fn text(&self, field: usize) -> String {
        match self.fields.get(field).map(|f| &f.value) {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        }
    }

    pub fn index(&self, field: usize) -> usize {
        match self.fields.get(field).map(|f| &f.value) {
            Some(Value::Choice { index, .. }) => *index,
            _ => 0,
        }
    }

    /// The widget to focus: the text field the keys are on.
    pub fn focus_id(&self) -> Id {
        match self.fields.get(self.focused).map(|f| &f.value) {
            Some(Value::Text(_)) => Id::from(format!("form-field-{}", self.focused)),
            _ => Id::from(NO_FIELD),
        }
    }

    fn change(&mut self, field: usize, step: isize) -> Outcome {
        match self.fields.get_mut(field).map(|f| &mut f.value) {
            Some(Value::Choice { options, index }) if !options.is_empty() => {
                let n = options.len() as isize;
                *index = usize::try_from((*index as isize + step).rem_euclid(n)).unwrap_or(0);
                Outcome::Changed(field)
            }
            Some(Value::Check { on, fixed: false }) => {
                *on = !*on;
                Outcome::Changed(field)
            }
            _ => Outcome::Stay,
        }
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        match message {
            Message::Input(field, value) => match self.fields.get_mut(field) {
                Some(f) => {
                    f.value = Value::Text(value);
                    self.focused = field;
                    Outcome::Changed(field)
                }
                None => Outcome::Stay,
            },
            Message::Choose(field, chosen) => {
                self.focused = field;
                match self.fields.get_mut(field).map(|f| &mut f.value) {
                    Some(Value::Choice { index, options }) if chosen < options.len() => {
                        *index = chosen;
                        Outcome::Changed(field)
                    }
                    Some(Value::Check { on, fixed: false }) => {
                        *on = chosen == 1;
                        Outcome::Changed(field)
                    }
                    _ => Outcome::Stay,
                }
            }
            Message::Focus(field) => {
                self.focused = field.min(self.fields.len().saturating_sub(1));
                Outcome::Stay
            }
            Message::Save => Outcome::Save,
            Message::Cancel => Outcome::Cancel,
        }
    }

    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        let count = self.fields.len().max(1);
        match ck {
            k if k == BACK_KEY => Outcome::Cancel,
            k if k == key!(enter) => Outcome::Save,
            k if is_previous(k) => {
                self.focused = (self.focused + count - 1) % count;
                Outcome::Stay
            }
            k if is_next(k) => {
                self.focused = (self.focused + 1) % count;
                Outcome::Stay
            }
            k if k == key!(left) => self.change(self.focused, -1),
            k if k == key!(right) => self.change(self.focused, 1),
            k if k == key!(space) => match self.fields.get(self.focused).map(|f| &f.value) {
                Some(Value::Check { .. }) => self.change(self.focused, 1),
                _ => Outcome::Stay,
            },
            _ => Outcome::Stay,
        }
    }

    /// The keys of a form, for the bar at the bottom.
    pub fn hints(s: &Strings) -> Vec<(String, String)> {
        vec![
            ("↑ ⇧⇥".to_string(), s.previous.to_string()),
            ("↓ ⇥".to_string(), s.next.to_string()),
            (format!("← → {}", s.space), s.change.to_string()),
            ("Enter".to_string(), s.save.to_string()),
            ("Esc".to_string(), s.cancel.to_string()),
        ]
    }

    pub fn view<'a, M: Clone + 'a>(
        &'a self,
        s: &'static Strings,
        c: Colors,
        on: impl Fn(Message) -> M + Copy + 'a,
        aside: Option<Element<'a, M>>,
    ) -> Element<'a, M> {
        let fields = self.fields.iter().enumerate().map(|(i, f)| {
            let field = self.field(i, f, c, on);
            match self.sections.iter().find(|(at, _)| *at == i) {
                Some((_, title)) => column![
                    iced::widget::space::vertical().height(6),
                    ui::section(title, c),
                    field
                ]
                .spacing(8)
                .into(),
                None => field,
            }
        });
        let error: Element<'a, M> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => iced::widget::space::vertical().height(0).into(),
        };
        let (no, yes) = self
            .actions
            .clone()
            .unwrap_or_else(|| (s.cancel.to_string(), s.save.to_string()));
        let buttons = row![
            ui::pill(&no, false, Some(on(Message::Cancel)), c),
            ui::pill(&yes, true, Some(on(Message::Save)), c),
        ]
        .spacing(10);
        let form = ui::panel(
            Some(&self.title),
            column![
                Column::with_children(fields).spacing(14),
                error,
                container(buttons)
                    .width(Length::Fill)
                    .align_x(Alignment::End),
            ]
            .spacing(16),
            c,
        );
        let body: Element<'a, M> = match aside {
            Some(aside) => row![
                container(form).width(Length::FillPortion(3)),
                container(aside)
                    .width(Length::FillPortion(2))
                    .height(Length::Fill)
            ]
            .spacing(14)
            .into(),
            None => row![
                iced::widget::space::horizontal().width(Length::FillPortion(1)),
                container(form).width(Length::FillPortion(3)),
                iced::widget::space::horizontal().width(Length::FillPortion(1)),
            ]
            .into(),
        };
        iced::widget::scrollable(body).height(Length::Fill).into()
    }

    fn field<'a, M: Clone + 'a>(
        &'a self,
        i: usize,
        f: &'a Field,
        c: Colors,
        on: impl Fn(Message) -> M + Copy + 'a,
    ) -> Element<'a, M> {
        let focused = i == self.focused;
        let label = text(f.label.clone())
            .size(SIZE_SMALL)
            .font(theme::fonts().bold)
            .color(match focused {
                true => c.accent,
                false => c.muted,
            });
        let input: Element<'a, M> = match &f.value {
            Value::Text(value) => text_input("", value)
                .id(format!("form-field-{i}"))
                .on_input(move |v| on(Message::Input(i, v)))
                .size(SIZE_BODY)
                .padding(Padding::from([8, 12]))
                .style(move |_, status| {
                    let active = focused || matches!(status, text_input::Status::Focused { .. });
                    text_input::Style {
                        background: c.background.into(),
                        border: iced::Border {
                            color: match active {
                                true => c.accent,
                                false => c.border,
                            },
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        icon: c.muted,
                        placeholder: c.muted,
                        value: c.text,
                        selection: iced::Color {
                            a: 0.35,
                            ..c.accent
                        },
                    }
                })
                .into(),
            Value::Check { .. } => iced::widget::space::vertical().height(0).into(),
            Value::Choice { options, index } => {
                let arrow = |label: &'static str, to: usize| {
                    button(text(label).size(SIZE_BODY).color(c.muted))
                        .padding(Padding::from([4, 10]))
                        .on_press(on(Message::Choose(i, to)))
                        .style(|_, _| button::Style::default())
                };
                let n = options.len().max(1);
                let current = options.get(*index).cloned().unwrap_or_default();
                container(
                    row![
                        arrow("◀", (*index + n - 1) % n),
                        container(ui::body(current))
                            .width(Length::Fill)
                            .align_x(Alignment::Center),
                        arrow("▶", (*index + 1) % n),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding(Padding::from([2, 4]))
                .style(move |_| container::Style {
                    background: Some(c.background.into()),
                    border: iced::Border {
                        color: match focused {
                            true => c.accent,
                            false => c.border,
                        },
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..container::Style::default()
                })
                .into()
            }
        };
        let hint: Element<'a, M> = match &f.hint {
            Some(h) => text(h.clone()).size(SIZE_SMALL).color(c.muted).into(),
            None => iced::widget::space::vertical().height(0).into(),
        };
        let body: Element<'a, M> = match &f.value {
            // a check is one line: the box, then what it is
            Value::Check { on: checked, fixed } => ui::list_row(
                row![
                    text(match checked {
                        true => "■",
                        false => "□",
                    })
                    .size(SIZE_BODY)
                    .color(match fixed {
                        true => c.muted,
                        false => c.accent,
                    }),
                    ui::body(f.label.clone()),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                focused,
                Some(on(Message::Choose(i, usize::from(!checked)))),
                c,
            ),
            _ => column![label, input, hint].spacing(6).into(),
        };
        // a click on the label or around the field moves the keys there
        iced::widget::mouse_area(body)
            .on_press(on(Message::Focus(i)))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        Form::new(
            "t",
            vec![
                Field::text("name", "Anna"),
                Field::choice("role", vec!["a".into(), "b".into(), "c".into()], 0),
                Field::text("number", "9"),
            ],
        )
    }

    #[test]
    fn the_keys_move_among_the_fields_and_change_the_choices() {
        let mut f = form();
        assert_eq!(f.focus_id(), Id::from("form-field-0"));
        f.key(key!(tab));
        assert_eq!(f.focused, 1);
        assert_eq!(f.focus_id(), Id::from(NO_FIELD));
        assert_eq!(f.key(key!(left)), Outcome::Changed(1));
        assert_eq!(f.index(1), 2);
        f.key(key!(right));
        assert_eq!(f.index(1), 0);
        f.key(key!(down));
        assert_eq!(f.focused, 2);
        f.key(key!(down));
        assert_eq!(f.focused, 0);
        f.key(key!(up));
        assert_eq!(f.focused, 2);
        f.key(key!(shift - tab));
        assert_eq!(f.focused, 1);
        assert_eq!(f.key(key!(enter)), Outcome::Save);
        assert_eq!(f.key(BACK_KEY), Outcome::Cancel);
        // ← → on a text field are the text's
        f.focused = 0;
        assert_eq!(f.key(key!(left)), Outcome::Stay);
    }
}
