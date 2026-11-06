//! A form, as in the TUI: ↑ or ⇧Tab to the field before, ↓ or Tab to the
//! next one, ← → change a choice, Enter saves, Esc cancels. The text fields
//! take the focus as the keys move it (see `focus_id`).

use {
    crate::{
        field,
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length, Padding,
        widget::{
            Column, Id, Row, button, column, container, overlay::menu, pick_list, row, text,
            text_input,
        },
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
};

#[derive(Debug, Clone)]
pub enum Value {
    Text(String),
    /// a date or an hour, written with the help of `mask`: only what can
    /// be one is taken, the separators come by themselves
    Masked {
        text: String,
        mask: Mask,
    },
    /// a text with what it may be offered below, as a search engine does:
    /// the rest of the first one written in grey, for Tab or Enter to take;
    /// ↑ ↓ go through the others, each written in the field as it's lit
    Search {
        /// what the field shows
        text: String,
        /// what was written, before ↑ ↓ wrote a suggestion
        typed: String,
        suggestions: Vec<Suggestion>,
        /// the suggestion ↑ ↓ lit: none, what was written
        highlighted: Option<usize>,
        /// written since the last one taken: the suggestions are shown
        open: bool,
    },
    /// one of `options`, in a menu: ← → change it
    Choice {
        options: Vec<String>,
        index: usize,
    },
    /// one of a few `options`, side by side: ← → change it
    Segments {
        options: Vec<String>,
        index: usize,
    },
    /// on or off, ← → or space switch it; fixed when it can't change
    Check {
        on: bool,
        fixed: bool,
    },
}

/// What a search offers: how it's shown (a flag before a country), what
/// it writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub label: String,
    pub value: String,
}

/// What a guided field is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mask {
    /// YYYY-MM-DD
    Date,
    /// HH:MM
    Time,
}

impl Mask {
    fn digits(self) -> usize {
        match self {
            Mask::Date => 8,
            Mask::Time => 4,
        }
    }

    /// The digits written, with the separators.
    fn format(self, digits: &str) -> String {
        digits
            .chars()
            .enumerate()
            .flat_map(|(i, d)| {
                let separator = match (self, i) {
                    (Mask::Date, 4 | 6) => Some('-'),
                    (Mask::Time, 2) => Some(':'),
                    _ => None,
                };
                separator.into_iter().chain(std::iter::once(d))
            })
            .collect()
    }

    /// Whether `digits` can begin a date or an hour.
    fn can_begin(self, digits: &str) -> bool {
        let number = |from: usize, to: usize| {
            digits
                .get(from..to.min(digits.len()))
                .and_then(|n| n.parse::<u32>().ok())
        };
        let n = digits.len();
        match self {
            Mask::Date => {
                let month = number(4, 6);
                match n {
                    0 => true,
                    1..=4 => matches!(number(0, 1), Some(1 | 2)),
                    5 => matches!(month, Some(0 | 1)),
                    6 => matches!(month, Some(1..=12)),
                    7 => match (month, number(6, 7)) {
                        (Some(2), Some(day)) => day <= 2,
                        (Some(1..=12), Some(day)) => day <= 3,
                        _ => false,
                    },
                    _ => matches!(
                        (number(0, 4), month, number(6, 8)),
                        (Some(y), Some(m), Some(d))
                            if i32::try_from(y).ok().and_then(|y| chrono::NaiveDate::from_ymd_opt(y, m, d)).is_some()
                    ),
                }
            }
            Mask::Time => match n {
                0 => true,
                1 => matches!(number(0, 1), Some(0..=2)),
                2 => matches!(number(0, 2), Some(0..=23)),
                3 => matches!(number(2, 3), Some(0..=5)),
                _ => matches!(number(2, 4), Some(0..=59)),
            },
        }
    }

    /// The text `typed` as far as it can be a date or an hour: a digit that
    /// can only be the second of its number gets its 0 before ("4" for the
    /// month is "04"), what can't be is left out.
    pub fn apply(self, typed: &str) -> String {
        let digits = typed
            .chars()
            .filter(char::is_ascii_digit)
            .fold(String::new(), |digits, d| {
                let padded = format!("{digits}0{d}");
                let starts_a_number = match self {
                    Mask::Date => matches!(digits.len(), 4 | 6),
                    Mask::Time => digits.is_empty(),
                };
                match (digits.len() < self.digits(), format!("{digits}{d}")) {
                    (false, _) => digits,
                    (true, next) if self.can_begin(&next) => next,
                    (true, _) if starts_a_number && self.can_begin(&padded) => padded,
                    (true, _) => digits,
                }
            });
        self.format(&digits)
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub label: String,
    pub value: Value,
    /// what to write, below the field
    pub hint: Option<String>,
    /// what to write, in the empty field
    pub placeholder: String,
    /// the icons of the segments, before their names
    pub icons: Vec<&'static [u8]>,
    /// a password: its letters not shown
    pub secret: bool,
}

impl Field {
    fn of(label: &str, value: Value) -> Field {
        Field {
            label: label.to_string(),
            value,
            hint: None,
            placeholder: String::new(),
            icons: vec![],
            secret: false,
        }
    }

    /// A password: written, not shown.
    pub fn secret(label: &str, value: impl Into<String>) -> Field {
        Field {
            secret: true,
            ..Field::text(label, value)
        }
    }

    pub fn placeholder(self, placeholder: &str) -> Field {
        Field {
            placeholder: placeholder.to_string(),
            ..self
        }
    }

    pub fn text(label: &str, value: impl Into<String>) -> Field {
        Field::of(label, Value::Text(value.into()))
    }

    /// A date or an hour, guided; `placeholder` says how to write it.
    pub fn masked(label: &str, value: &str, mask: Mask, placeholder: &str) -> Field {
        Field {
            placeholder: placeholder.to_string(),
            ..Field::of(
                label,
                Value::Masked {
                    text: mask.apply(value),
                    mask,
                },
            )
        }
    }

    /// A text with suggestions, given by the screen as it changes.
    pub fn search(label: &str, value: impl Into<String>) -> Field {
        Field::of(label, {
            let text: String = value.into();
            Value::Search {
                typed: text.clone(),
                text,
                suggestions: vec![],
                highlighted: None,
                open: false,
            }
        })
    }

    pub fn choice(label: &str, options: Vec<String>, index: usize) -> Field {
        Field::of(label, Value::Choice { options, index })
    }

    pub fn segments(label: &str, options: Vec<String>, index: usize) -> Field {
        Field::of(label, Value::Segments { options, index })
    }

    pub fn check(label: &str, on: bool, fixed: bool) -> Field {
        Field::of(label, Value::Check { on, fixed })
    }

    /// The segments with their icons, in their order.
    pub fn icons(self, icons: Vec<&'static [u8]>) -> Field {
        Field { icons, ..self }
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
    /// a suggestion of a search taken
    Take(usize, usize),
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
    /// no buttons: each change is saved as it's made
    pub plain: bool,
    /// the fields shown, and the keys move among: all, if none
    pub shown: Option<std::ops::Range<usize>>,
    /// the texts before the changes, to undo them: the field, its value
    undo: Vec<(usize, Value)>,
    /// the changes undone, to make again
    redo: Vec<(usize, Value)>,
}

/// How many suggestions a search shows.
const SUGGESTIONS: usize = 6;

/// The focus on nothing: on a choice, no text field takes the keys.
const NO_FIELD: &str = "form-no-field";

fn is_previous(ck: KeyCombination) -> bool {
    ck == key!(up) || ck == key!(shift - tab) || ck == key!(shift - backtab) || ck == key!(backtab)
}

fn is_next(ck: KeyCombination) -> bool {
    ck == key!(down) || ck == key!(tab)
}

/// ⌘Z on macOS, ⌃Z elsewhere (see `keymap`).
fn is_undo(ck: KeyCombination) -> bool {
    ck == key!(ctrl - z)
}

pub fn is_redo(ck: KeyCombination) -> bool {
    ck == key!(ctrl - y) || ck == key!(ctrl - shift - z) || ck == key!(ctrl - 'Z')
}

/// The text of a field that's written.
fn written(value: &Value) -> Option<&String> {
    match value {
        Value::Text(text) | Value::Masked { text, .. } | Value::Search { text, .. } => Some(text),
        _ => None,
    }
}

/// The suggestions shown: written since the last taken, and not just the
/// text there is; the one lit, if any.
fn offered(value: &Value) -> Option<(&[Suggestion], Option<usize>)> {
    match value {
        Value::Search {
            text,
            suggestions,
            highlighted,
            open: true,
            ..
        } if !suggestions.is_empty()
            && !(suggestions.len() == 1
                && suggestions.first().is_some_and(|s| &s.value == text)) =>
        {
            Some((
                &suggestions[..suggestions.len().min(SUGGESTIONS)],
                *highlighted,
            ))
        }
        _ => None,
    }
}

/// What the first suggestion adds to what was written, when it begins
/// with it and none is lit: written in grey after the text.
fn completion(value: &Value) -> Option<(usize, String)> {
    match offered(value) {
        Some((suggestions, None)) => {
            let text = written(value)?;
            let first = suggestions.first()?;
            let rest = first.value.get(text.len()..)?;
            match first
                .value
                .get(..text.len())
                .is_some_and(|start| start.to_lowercase() == text.to_lowercase())
                && !rest.is_empty()
            {
                true => Some((0, rest.to_string())),
                false => None,
            }
        }
        _ => None,
    }
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
            plain: false,
            shown: None,
            undo: vec![],
            redo: vec![],
        }
    }

    pub fn text(&self, field: usize) -> String {
        self.fields
            .get(field)
            .and_then(|f| written(&f.value))
            .cloned()
            .unwrap_or_default()
    }

    pub fn index(&self, field: usize) -> usize {
        match self.fields.get(field).map(|f| &f.value) {
            Some(Value::Choice { index, .. } | Value::Segments { index, .. }) => *index,
            _ => 0,
        }
    }

    /// What the search `field` offers, for the text it has: none lit.
    pub fn suggest(&mut self, field: usize, offered: Vec<Suggestion>) {
        if let Some(Value::Search {
            suggestions,
            highlighted,
            ..
        }) = self.fields.get_mut(field).map(|f| &mut f.value)
        {
            *suggestions = offered;
            *highlighted = None;
        }
    }

    /// The widget to focus: the text field the keys are on.
    pub fn focus_id(&self) -> Id {
        match self.fields.get(self.focused).map(|f| written(&f.value)) {
            Some(Some(_)) => Id::from(format!("form-field-{}", self.focused)),
            _ => Id::from(NO_FIELD),
        }
    }

    /// The text of `field` becomes `text`, the one before kept to undo it.
    fn write(&mut self, field: usize, text: String) -> Outcome {
        self.write_as(field, text, false)
    }

    /// As `write`; `alone`, the change is undone by itself (a suggestion
    /// taken), not with the word written before.
    fn write_as(&mut self, field: usize, text: String, alone: bool) -> Outcome {
        let Some(f) = self.fields.get_mut(field) else {
            return Outcome::Stay;
        };
        let before = f.value.clone();
        f.value = match before.clone() {
            Value::Masked { mask, .. } => Value::Masked {
                text: mask.apply(&text),
                mask,
            },
            Value::Search { suggestions, .. } => Value::Search {
                typed: text.clone(),
                text,
                suggestions,
                highlighted: None,
                open: true,
            },
            _ => Value::Text(text),
        };
        self.focused = field;
        // a word at a time: the letters of a word undone together
        let word = match (self.undo.last(), written(&before)) {
            (Some((last, _)), Some(was)) => {
                alone || *last != field || was.ends_with(' ') || was.is_empty()
            }
            _ => true,
        };
        if word && written(&before) != written(&f.value) {
            self.undo.push((field, before));
        }
        self.redo.clear();
        Outcome::Changed(field)
    }

    /// The suggestion `chosen` written in the search `field`.
    fn take(&mut self, field: usize, chosen: usize) -> Outcome {
        let value = self.fields.get(field).and_then(|f| match &f.value {
            Value::Search { suggestions, .. } => suggestions.get(chosen).map(|s| s.value.clone()),
            _ => None,
        });
        match value {
            Some(value) => {
                let outcome = self.write_as(field, value, true);
                if let Some(Value::Search { open, .. }) =
                    self.fields.get_mut(field).map(|f| &mut f.value)
                {
                    *open = false;
                }
                outcome
            }
            None => Outcome::Stay,
        }
    }

    /// The last change undone (`back`), or made again.
    fn undo(&mut self, back: bool) -> Outcome {
        let popped = match back {
            true => self.undo.pop(),
            false => self.redo.pop(),
        };
        let Some((field, value)) = popped else {
            return Outcome::Stay;
        };
        let Some(f) = self.fields.get_mut(field) else {
            return Outcome::Stay;
        };
        let now = std::mem::replace(&mut f.value, value);
        if let Value::Search {
            text,
            typed,
            highlighted,
            open,
            ..
        } = &mut f.value
        {
            *typed = text.clone();
            *highlighted = None;
            *open = false;
        }
        match back {
            true => self.redo.push((field, now)),
            false => self.undo.push((field, now)),
        }
        self.focused = field;
        Outcome::Changed(field)
    }

    /// The suggestion lit moved by `step`, and written in the field: past
    /// the first or the last one, none, and what was written comes back.
    fn highlight(&mut self, step: isize) {
        let shown = self
            .fields
            .get(self.focused)
            .and_then(|f| offered(&f.value))
            .map(|(shown, _)| shown.len());
        if let (
            Some(n),
            Some(Value::Search {
                text,
                typed,
                suggestions,
                highlighted,
                ..
            }),
        ) = (
            shown,
            self.fields.get_mut(self.focused).map(|f| &mut f.value),
        ) {
            // none is n: the ring is the suggestions and what was written
            let at = highlighted.unwrap_or(n) as isize;
            let next = usize::try_from((at + step).rem_euclid(n as isize + 1)).unwrap_or(n);
            *highlighted = (next < n).then_some(next);
            *text = highlighted
                .and_then(|h| suggestions.get(h))
                .map_or_else(|| typed.clone(), |s| s.value.clone());
        }
    }

    /// Whether the field the keys are on shows its suggestions.
    pub fn offering(&self) -> bool {
        self.fields
            .get(self.focused)
            .and_then(|f| offered(&f.value))
            .is_some()
    }

    /// The suggestion Enter or Tab would take: the one lit, or the first
    /// when it completes what was written.
    pub fn lit(&self) -> Option<usize> {
        self.fields
            .get(self.focused)
            .and_then(|f| match offered(&f.value) {
                Some((_, Some(lit))) => Some(lit),
                _ => completion(&f.value).map(|(first, _)| first),
            })
    }

    /// The suggestions of the field the keys are on closed: what was
    /// written back, if ↑ ↓ wrote one.
    fn close(&mut self) {
        if let Some(Value::Search {
            text,
            typed,
            highlighted,
            open,
            ..
        }) = self.fields.get_mut(self.focused).map(|f| &mut f.value)
        {
            if highlighted.take().is_some() {
                *text = typed.clone();
            }
            *open = false;
        }
    }

    fn change(&mut self, field: usize, step: isize) -> Outcome {
        match self.fields.get_mut(field).map(|f| &mut f.value) {
            Some(Value::Choice { options, index } | Value::Segments { options, index })
                if !options.is_empty() =>
            {
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
            Message::Input(field, value) => self.write(field, value),
            Message::Take(field, chosen) => self.take(field, chosen),
            Message::Choose(field, chosen) => {
                self.focused = field;
                match self.fields.get_mut(field).map(|f| &mut f.value) {
                    Some(Value::Choice { index, options } | Value::Segments { index, options })
                        if chosen < options.len() =>
                    {
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

    /// The fields shown.
    fn range(&self) -> std::ops::Range<usize> {
        self.shown.clone().unwrap_or(0..self.fields.len())
    }

    /// The field after the one the keys are on, among the ones shown;
    /// before it, if `back`.
    fn step(&self, back: bool) -> usize {
        let r = self.range();
        match (back, self.focused) {
            (true, f) if f <= r.start => r.end.saturating_sub(1),
            (true, f) => f - 1,
            (false, f) if f + 1 >= r.end => r.start,
            (false, f) => f + 1,
        }
    }

    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        let (offering, lit) = (self.offering(), self.lit());
        match (ck, offering, lit) {
            (k, _, _) if is_undo(k) => self.undo(true),
            (k, _, _) if is_redo(k) => self.undo(false),
            // the suggestions shown: they take the keys
            (k, true, _) if k == BACK_KEY => {
                self.close();
                Outcome::Stay
            }
            (k, true, Some(lit)) if k == key!(enter) => self.take(self.focused, lit),
            (k, true, Some(lit)) if k == key!(tab) => {
                let outcome = self.take(self.focused, lit);
                self.focused = self.step(false);
                outcome
            }
            (k, true, _) if k == key!(down) => {
                self.highlight(1);
                Outcome::Stay
            }
            (k, true, _) if k == key!(up) => {
                self.highlight(-1);
                Outcome::Stay
            }
            // Tab on: what was written stays, the suggestions close
            (k, true, None) if k == key!(tab) => {
                self.close();
                self.move_key(k)
            }
            (k, _, _) => self.move_key(k),
        }
    }

    fn move_key(&mut self, ck: KeyCombination) -> Outcome {
        match ck {
            k if k == BACK_KEY => Outcome::Cancel,
            k if k == key!(enter) => Outcome::Save,
            k if is_previous(k) => {
                self.focused = self.step(true);
                Outcome::Stay
            }
            k if is_next(k) => {
                self.focused = self.step(false);
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
        let range = self.range();
        let fields = self
            .fields
            .iter()
            .enumerate()
            .filter(move |(i, _)| range.contains(i))
            .map(|(i, f)| {
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
                match self.plain {
                    true => Element::from(iced::widget::space::vertical().height(0)),
                    false => container(buttons)
                        .width(Length::Fill)
                        .align_x(Alignment::End)
                        .into(),
                },
            ]
            .spacing(16),
            c,
        );
        let body: Element<'a, M> = match aside {
            Some(aside) => row![
                container(form).width(Length::FillPortion(3)),
                // as tall as it is: in the scrollable, not cut to the form
                container(aside).width(Length::FillPortion(2))
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
        let edge = move |active: bool| iced::Border {
            color: match active {
                true => c.accent,
                false => c.border,
            },
            width: 1.0,
            radius: 8.0.into(),
        };
        let input = |value: &str, follow: field::Follow| -> Element<'a, M> {
            field::guard(
                text_input(&f.placeholder, value)
                    .secure(f.secret)
                    .id(format!("form-field-{i}"))
                    .on_input(move |v| on(Message::Input(i, v)))
                    .size(SIZE_BODY)
                    .padding(Padding::from([8, 12]))
                    .style(move |_, status| {
                        let active =
                            focused || matches!(status, text_input::Status::Focused { .. });
                        text_input::Style {
                            background: c.background.into(),
                            border: edge(active),
                            icon: c.muted,
                            placeholder: c.muted,
                            value: c.text,
                            selection: iced::Color {
                                a: 0.35,
                                ..c.accent
                            },
                        }
                    }),
                follow,
            )
            .into()
        };
        let input: Element<'a, M> = match &f.value {
            Value::Text(value) => input(value, field::Follow::Never),
            Value::Masked { text, .. } => input(text, field::Follow::Typing),
            Value::Search { text: written, .. } => {
                let shown: Vec<Element<'a, M>> = match offered(&f.value) {
                    Some((suggestions, lit)) if focused => suggestions
                        .iter()
                        .enumerate()
                        .map(|(n, s)| {
                            ui::list_row(
                                ui::body(s.label.clone()).shaping(text::Shaping::Advanced),
                                Some(n) == lit,
                                Some(on(Message::Take(i, n))),
                                c,
                            )
                        })
                        .collect(),
                    _ => vec![],
                };
                // the rest of the first suggestion in grey, over the field
                // where the text ends
                let ghost: Element<'a, M> = match completion(&f.value) {
                    Some((_, rest)) if focused => container(
                        row![
                            text(written.clone())
                                .size(SIZE_BODY)
                                .wrapping(text::Wrapping::None)
                                .color(iced::Color::TRANSPARENT),
                            text(rest)
                                .size(SIZE_BODY)
                                .wrapping(text::Wrapping::None)
                                .color(c.muted),
                        ]
                        .clip(true),
                    )
                    .padding(Padding::from([8, 12]))
                    .into(),
                    _ => iced::widget::space::horizontal().width(0).into(),
                };
                // the same widgets whether offered or not: the field keeps
                // the keys as the suggestions come and go
                let list: Element<'a, M> = match shown.is_empty() {
                    true => iced::widget::space::vertical().height(0).into(),
                    false => container(Column::with_children(shown).spacing(2))
                        .padding(4)
                        .style(move |_| container::Style {
                            background: Some(c.background.into()),
                            border: edge(false),
                            ..container::Style::default()
                        })
                        .into(),
                };
                column![
                    iced::widget::stack![input(written, field::Follow::Arrows), ghost],
                    list
                ]
                .spacing(4)
                .into()
            }
            Value::Check { .. } => iced::widget::space::vertical().height(0).into(),
            Value::Choice { options, index } => {
                let all = options.clone();
                pick_list(
                    options.as_slice(),
                    options.get(*index),
                    move |chosen: String| {
                        on(Message::Choose(
                            i,
                            all.iter().position(|o| *o == chosen).unwrap_or(0),
                        ))
                    },
                )
                .width(Length::Fill)
                .text_size(SIZE_BODY)
                .text_shaping(text::Shaping::Advanced)
                .padding(Padding::from([8, 12]))
                .style(move |_, status| pick_list::Style {
                    text_color: c.text,
                    placeholder_color: c.muted,
                    handle_color: match status {
                        pick_list::Status::Active => c.muted,
                        _ => c.accent,
                    },
                    background: c.background.into(),
                    border: edge(focused || !matches!(status, pick_list::Status::Active)),
                })
                .menu_style(move |_| menu::Style {
                    background: c.panel.into(),
                    border: edge(true),
                    text_color: c.text,
                    selected_text_color: c.text,
                    selected_background: iced::Color {
                        a: 0.18,
                        ..c.accent
                    }
                    .into(),
                    shadow: iced::Shadow::default(),
                })
                .into()
            }
            Value::Segments { options, index } => container(
                Row::with_children(options.iter().enumerate().map(|(n, option)| {
                    let chosen = n == *index;
                    let ink = match chosen {
                        true => c.on_accent,
                        false => c.text,
                    };
                    let name = text(option.clone()).size(SIZE_BODY);
                    let label: Element<'a, M> = match f.icons.get(n) {
                        Some(icon) => row![ui::icon(icon, 18.0, ink), name]
                            .spacing(8)
                            .align_y(Alignment::Center)
                            .into(),
                        None => name.into(),
                    };
                    button(
                        container(label)
                            .width(Length::Fill)
                            .align_x(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding(Padding::from([7, 12]))
                    .on_press(on(Message::Choose(i, n)))
                    .style(move |_, status| button::Style {
                        background: match (chosen, status) {
                            (true, _) => Some(c.accent.into()),
                            (false, button::Status::Hovered | button::Status::Pressed) => {
                                Some(iced::Color { a: 0.7, ..c.button }.into())
                            }
                            _ => None,
                        },
                        text_color: match chosen {
                            true => c.on_accent,
                            false => c.text,
                        },
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..iced::Border::default()
                        },
                        ..button::Style::default()
                    })
                    .into()
                }))
                .spacing(4),
            )
            .padding(3)
            .style(move |_| container::Style {
                background: Some(c.background.into()),
                border: edge(focused),
                ..container::Style::default()
            })
            .into(),
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
            // no label: the field alone (home or away)
            _ if f.label.is_empty() => column![input, hint].spacing(6).into(),
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

    #[test]
    fn a_date_and_an_hour_are_guided_as_they_are_written() {
        let date = |typed: &str| Mask::Date.apply(typed);
        assert_eq!(date("2026"), "2026");
        assert_eq!(date("20261"), "2026-1");
        assert_eq!(date("202611"), "2026-11");
        assert_eq!(date("2026112"), "2026-11-2");
        assert_eq!(date("20261124"), "2026-11-24");
        // a month of 4: 04, a day of 5: 05
        assert_eq!(date("20264"), "2026-04");
        assert_eq!(date("2026045"), "2026-04-05");
        // 4, 4: the 4th of April; no 13th month, no 31st of April, no 30th
        // of February
        assert_eq!(date("202644"), "2026-04-04");
        assert_eq!(date("2026-13"), "2026-1");
        assert_eq!(date("2026-04-31"), "2026-04-3");
        assert_eq!(date("2026-02-3"), "2026-02-03");
        assert_eq!(date("2024-02-29"), "2024-02-29");
        assert_eq!(date("2026-02-29"), "2026-02-2");
        // no year 3000, nothing but digits, no more than a date
        assert_eq!(date("3"), "");
        assert_eq!(date("19x9"), "199");
        assert_eq!(date("2026-11-249"), "2026-11-24");
        let time = |typed: &str| Mask::Time.apply(typed);
        assert_eq!(time("2"), "2");
        assert_eq!(time("20"), "20");
        assert_eq!(time("203"), "20:3");
        assert_eq!(time("2030"), "20:30");
        assert_eq!(time("9"), "09");
        assert_eq!(time("24"), "2");
        assert_eq!(time("206"), "20");
    }

    fn searching() -> Form {
        let mut f = Form::new(
            "t",
            vec![Field::search("country", ""), Field::text("name", "")],
        );
        f.update(Message::Input(0, "it".into()));
        let offered = ["Italia", "Haiti"]
            .map(|v| Suggestion {
                label: format!("· {v}"),
                value: v.to_string(),
            })
            .to_vec();
        f.suggest(0, offered);
        f
    }

    #[test]
    fn a_search_completes_as_a_search_engine_does() {
        // "it": the rest of Italia in grey, Enter takes it, the field stays
        let mut f = searching();
        assert!(f.offering());
        assert_eq!(completion(&f.fields[0].value), Some((0, "alia".into())));
        assert_eq!(f.lit(), Some(0));
        assert_eq!(f.key(key!(enter)), Outcome::Changed(0));
        assert_eq!((f.text(0).as_str(), f.focused), ("Italia", 0));
        assert!(!f.offering());
        // Tab takes it and goes on
        let mut f = searching();
        f.key(key!(tab));
        assert_eq!((f.text(0).as_str(), f.focused), ("Italia", 1));
        // ↓ ↓ write each one in the field, no grey; ↓ past the last, what
        // was written comes back
        let mut f = searching();
        f.key(key!(down));
        assert_eq!(f.text(0), "Italia");
        f.key(key!(down));
        assert_eq!((f.text(0).as_str(), f.lit()), ("Haiti", Some(1)));
        assert_eq!(completion(&f.fields[0].value), None);
        f.key(key!(down));
        assert_eq!(f.text(0), "it");
        f.key(key!(up));
        assert_eq!(f.text(0), "Haiti");
        assert_eq!(f.key(key!(enter)), Outcome::Changed(0));
        assert_eq!(f.text(0), "Haiti");
        // Esc closes them, what was written back; the form stays
        let mut f = searching();
        f.key(key!(down));
        assert_eq!(f.key(BACK_KEY), Outcome::Stay);
        assert_eq!(f.text(0), "it");
        assert!(!f.offering());
        assert_eq!(f.key(key!(enter)), Outcome::Save);
        // a click takes one; undone alone, what was written comes back
        let mut f = searching();
        f.update(Message::Take(0, 1));
        assert_eq!(f.text(0), "Haiti");
        f.key(key!(ctrl - z));
        assert_eq!(f.text(0), "it");
    }

    #[test]
    fn what_no_suggestion_begins_with_is_left_as_written() {
        // "ait": Haiti offered, not completed; Enter saves what's written
        let mut f = Form::new(
            "t",
            vec![Field::search("country", ""), Field::text("name", "")],
        );
        f.update(Message::Input(0, "ait".into()));
        f.suggest(
            0,
            vec![Suggestion {
                label: "Haiti".into(),
                value: "Haiti".into(),
            }],
        );
        assert!(f.offering());
        assert_eq!(f.lit(), None);
        assert_eq!(f.key(key!(enter)), Outcome::Save);
        assert_eq!(f.text(0), "ait");
        // Tab goes on with it
        f.key(key!(tab));
        assert_eq!((f.text(0).as_str(), f.focused), ("ait", 1));
    }

    #[test]
    fn the_writing_is_undone_a_word_at_a_time() {
        let mut f = form();
        let typed = ["Anna ", "Anna M", "Anna Ma"];
        typed.iter().for_each(|t| {
            f.update(Message::Input(0, t.to_string()));
        });
        f.update(Message::Input(2, "10".into()));
        assert_eq!(f.key(key!(ctrl - z)), Outcome::Changed(2));
        assert_eq!(f.text(2), "9");
        f.key(key!(ctrl - z));
        assert_eq!((f.text(0).as_str(), f.focused), ("Anna ", 0));
        f.key(key!(ctrl - z));
        assert_eq!(f.text(0), "Anna");
        assert_eq!(f.key(key!(ctrl - z)), Outcome::Stay);
        f.key(key!(ctrl - y));
        assert_eq!(f.text(0), "Anna ");
        // something written: nothing to do again
        f.update(Message::Input(0, "Anna B".into()));
        assert_eq!(f.key(key!(ctrl - y)), Outcome::Stay);
    }

    #[test]
    fn segments_change_as_a_choice() {
        let mut f = Form::new(
            "t",
            vec![Field::segments(
                "where",
                vec!["home".into(), "away".into()],
                0,
            )],
        );
        f.key(key!(right));
        assert_eq!(f.index(0), 1);
        f.update(Message::Choose(0, 0));
        assert_eq!(f.index(0), 0);
    }
}
