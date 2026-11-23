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
    chrono::{Datelike, Months, NaiveDate},
    iced::{
        Alignment, Element, Length, Padding,
        widget::{
            Column, Id, Row, button, column, container, overlay::menu, pick_list, row, text,
            text_editor, text_input,
        },
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    std::collections::HashSet,
};

/// The days of the month `day` is in.
fn days_in(day: NaiveDate) -> u32 {
    day.checked_add_months(Months::new(1)).map_or(31, |next| {
        u32::try_from((next - day).num_days()).unwrap_or(31)
    })
}

#[derive(Debug, Clone)]
pub enum Value {
    Text(String),
    /// a text of more lines (goals, notes): Enter starts a new line, ↑ on
    /// the first line and ↓ on the last go to the field before and after,
    /// ⌘Enter saves; `text` is what `content`, the editor's, holds
    Lines {
        text: String,
        content: text_editor::Content,
    },
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
        /// a drop-down: an arrow on its right opens all it may be, the
        /// list scrolling (`Field::dropdown`)
        dropdown: bool,
        /// opened by its arrow: all of them, whatever is written
        browsing: bool,
        /// the first of the rows a drop-down shows, the lit one among them
        top: usize,
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
    /// a date with a button that opens a month to choose the day from
    pub calendar: bool,
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
            calendar: false,
        }
    }

    /// The date can be chosen from a calendar, the button beside it.
    pub fn with_calendar(self) -> Field {
        Field {
            calendar: true,
            ..self
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

    /// A text of more lines.
    pub fn lines(label: &str, value: impl Into<String>) -> Field {
        let text = value.into();
        Field::of(
            label,
            Value::Lines {
                content: text_editor::Content::with_text(&text),
                text,
            },
        )
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
                dropdown: false,
                browsing: false,
                top: 0,
            }
        })
    }

    /// A text with suggestions that is also a drop-down, as a select of
    /// the web: its arrow opens all it may be, writing narrows them.
    pub fn dropdown(label: &str, value: impl Into<String>) -> Field {
        let mut field = Field::search(label, value);
        if let Value::Search { dropdown, .. } = &mut field.value {
            *dropdown = true;
        }
        field
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
    /// what the editor of a text of more lines did: written, clicked, moved
    Edit(usize, text_editor::Action),
    Choose(usize, usize),
    /// a suggestion of a search taken
    Take(usize, usize),
    /// the arrow of a drop-down: all it may be shown, or hidden
    Open(usize),
    /// the wheel over the list of a drop-down: down (positive) or up
    Scroll(usize, i32),
    Focus(usize),
    /// the button of a date: its calendar open or closed
    Calendar(usize),
    /// the calendar shows the month after (positive) or before
    Month(i32),
    /// a day of the calendar chosen
    Day(NaiveDate),
    /// more about a field asked (the number and the role of a player on
    /// the score sheet): for the screen, the form stays as it is
    Details(usize),
    /// the other answer of a question asked over the form (not the one
    /// Save gives, not Cancel): for the screen, the form stays as it is
    Another,
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

/// A button of a form: its icon and its name.
pub type Answer = (&'static [u8], String);

#[derive(Debug, Clone)]
pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub focused: usize,
    /// why it can't be saved, as the core says
    pub error: Option<String>,
    /// a title before the field, for a group of them
    pub sections: Vec<(usize, String)>,
    /// the buttons, when not to cancel and to save: Esc's, then Enter's,
    /// each with its icon
    pub actions: Option<(Answer, Answer)>,
    /// a button between the two, giving `Message::Another` (to go on to
    /// another entry, say)
    pub another: Option<Answer>,
    /// no buttons: each change is saved as it's made
    pub plain: bool,
    /// the fields shown, and the keys move among: all, if none
    pub shown: Option<std::ops::Range<usize>>,
    /// the fields left out of the ones shown, until they apply
    pub hidden: Vec<usize>,
    /// the texts before the changes, to undo them: the field, its value
    undo: Vec<(usize, Value)>,
    /// the changes undone, to make again
    redo: Vec<(usize, Value)>,
    /// the calendar open: its field, and the first day of the month shown
    pub picker: Option<(usize, NaiveDate)>,
    /// the days already taken, marked in the calendar
    pub busy: HashSet<NaiveDate>,
}

/// How many suggestions a search shows.
const SUGGESTIONS: usize = 6;

/// How many rows of its list a drop-down shows at a time.
const DROPDOWN_ROWS: usize = 8;

/// The first row shown so that `row` is among the ones shown, from `top`.
fn in_sight(top: usize, row: usize) -> usize {
    match row {
        r if r < top => r,
        r if r >= top + DROPDOWN_ROWS => r + 1 - DROPDOWN_ROWS,
        _ => top,
    }
}

/// The key that saves from a text of more lines, where Enter starts a line.
const SAVE_FROM_LINES: &str = match cfg!(target_os = "macos") {
    true => "⌘Enter",
    false => "⌃Enter",
};

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
        Value::Text(text)
        | Value::Masked { text, .. }
        | Value::Search { text, .. }
        | Value::Lines { text, .. } => Some(text),
        _ => None,
    }
}

/// The suggestions shown: written since the last taken, and not just the
/// text there is (unless ↑ ↓ wrote it: it's lit, for Enter to take); the
/// one lit, if any.
fn offered(value: &Value) -> Option<(&[Suggestion], Option<usize>)> {
    match value {
        Value::Search {
            text,
            suggestions,
            highlighted,
            open: true,
            dropdown,
            browsing,
            ..
        } if !suggestions.is_empty()
            && (*browsing
                || !(highlighted.is_none()
                    && suggestions.len() == 1
                    && suggestions.first().is_some_and(|s| &s.value == text))) =>
        {
            // a drop-down scrolls through them all
            let shown = match dropdown {
                true => suggestions.len(),
                false => suggestions.len().min(SUGGESTIONS),
            };
            Some((&suggestions[..shown], *highlighted))
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

/// The least height of a text of more lines: three of them.
const LINES_HEIGHT: f32 = 84.0;

/// The keys of a text of more lines: the editor's, except ↑ on its first
/// line and ↓ on its last (to the field before, after), ⌘Enter (saves) and
/// Tab (to the next field), left to the form.
fn lines_binding<M>(
    content: &text_editor::Content,
    press: text_editor::KeyPress,
) -> Option<text_editor::Binding<M>> {
    use iced::keyboard::{Key, key::Named};
    let line = content.cursor().position.line;
    let last = content.line_count().saturating_sub(1);
    let plain = !(press.modifiers.shift() || press.modifiers.command() || press.modifiers.alt());
    match (press.key.as_ref(), plain, press.modifiers.command()) {
        (Key::Named(Named::ArrowUp), true, _) if line == 0 => None,
        (Key::Named(Named::ArrowDown), true, _) if line >= last => None,
        (Key::Named(Named::Enter), _, true) => None,
        (Key::Named(Named::Tab), ..) => None,
        _ => text_editor::Binding::from_key_press(press),
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
            another: None,
            plain: false,
            shown: None,
            hidden: vec![],
            undo: vec![],
            redo: vec![],
            picker: None,
            busy: HashSet::new(),
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
            text,
            suggestions,
            highlighted,
            browsing,
            top,
            ..
        }) = self.fields.get_mut(field).map(|f| &mut f.value)
        {
            // all of them, opened: the one written in sight, a few above it
            *top = match browsing {
                true => offered
                    .iter()
                    .position(|s| &s.value == text)
                    .map_or(0, |at| at.saturating_sub(2)),
                false => 0,
            };
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
            Value::Search {
                suggestions,
                dropdown,
                ..
            } => Value::Search {
                typed: text.clone(),
                text,
                suggestions,
                highlighted: None,
                open: true,
                dropdown,
                browsing: false,
                top: 0,
            },
            Value::Lines { .. } => Value::Lines {
                content: text_editor::Content::with_text(&text),
                text,
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

    /// Whether the drop-down `field` was opened by its arrow: all it may be
    /// to offer, whatever is written.
    pub fn browsing(&self, field: usize) -> bool {
        matches!(
            self.fields.get(field).map(|f| &f.value),
            Some(Value::Search { browsing: true, .. })
        )
    }

    /// The arrow of the drop-down `field`: its list opened with all it may
    /// be (the screen offers them on the change), or closed.
    fn open(&mut self, field: usize) -> Outcome {
        self.focused = field;
        match self.fields.get_mut(field).map(|f| &mut f.value) {
            Some(Value::Search {
                open,
                browsing,
                highlighted,
                text,
                typed,
                dropdown: true,
                ..
            }) => {
                let opening = !(*open && *browsing);
                if highlighted.take().is_some() {
                    *text = typed.clone();
                }
                *open = opening;
                *browsing = opening;
                Outcome::Changed(field)
            }
            _ => Outcome::Stay,
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
            browsing,
            ..
        } = &mut f.value
        {
            *typed = text.clone();
            *highlighted = None;
            *open = false;
            *browsing = false;
        }
        match back {
            true => self.redo.push((field, now)),
            false => self.undo.push((field, now)),
        }
        self.focused = field;
        Outcome::Changed(field)
    }

    /// What the editor of the text of more lines `field` did; a change is
    /// undone a word at a time, as in the other fields.
    fn edit(&mut self, field: usize, action: text_editor::Action) -> Outcome {
        let last = self.undo.last().map(|(f, _)| *f);
        let Some(f) = self.fields.get_mut(field) else {
            return Outcome::Stay;
        };
        let editing = action.is_edit();
        let before = f.value.clone();
        let Value::Lines { text, content } = &mut f.value else {
            return Outcome::Stay;
        };
        self.focused = field;
        content.perform(action);
        let written = content.text();
        let word = last != Some(field) || text.ends_with(char::is_whitespace) || text.is_empty();
        match (editing, written == *text) {
            (true, false) => {
                *text = written;
                if word {
                    self.undo.push((field, before));
                }
                self.redo.clear();
                Outcome::Changed(field)
            }
            _ => Outcome::Stay,
        }
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
                top,
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
            if let Some(lit) = *highlighted {
                *top = in_sight(*top, lit);
            }
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
            browsing,
            ..
        }) = self.fields.get_mut(self.focused).map(|f| &mut f.value)
        {
            if highlighted.take().is_some() {
                *text = typed.clone();
            }
            *open = false;
            *browsing = false;
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
            Message::Edit(field, action) => self.edit(field, action),
            Message::Take(field, chosen) => self.take(field, chosen),
            Message::Open(field) => self.open(field),
            Message::Scroll(field, step) => {
                let total = self
                    .fields
                    .get(field)
                    .and_then(|f| offered(&f.value))
                    .map_or(0, |(all, _)| all.len());
                if let Some(Value::Search { top, .. }) =
                    self.fields.get_mut(field).map(|f| &mut f.value)
                {
                    let last = total.saturating_sub(DROPDOWN_ROWS);
                    *top = top.saturating_add_signed(step as isize).min(last);
                }
                Outcome::Stay
            }
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
            Message::Calendar(field) => self.toggle_calendar(field),
            Message::Month(step) => {
                self.picker = self.picker.map(|(at, month)| {
                    let months = Months::new(step.unsigned_abs());
                    let moved = match step < 0 {
                        true => month.checked_sub_months(months),
                        false => month.checked_add_months(months),
                    };
                    (at, moved.unwrap_or(month))
                });
                Outcome::Stay
            }
            Message::Day(day) => match self.picker.take() {
                Some((field, _)) => self.write(field, day.format("%Y-%m-%d").to_string()),
                None => Outcome::Stay,
            },
            Message::Details(_) | Message::Another => Outcome::Stay,
            Message::Save => Outcome::Save,
            Message::Cancel => Outcome::Cancel,
        }
    }

    /// Opens the calendar of a date at the month it holds (this one when
    /// it holds none), or closes it.
    fn toggle_calendar(&mut self, field: usize) -> Outcome {
        self.focused = field;
        let shown = self.picker.is_some_and(|(at, _)| at == field);
        let calendar = self.fields.get(field).is_some_and(|f| f.calendar);
        self.picker = match (shown, calendar) {
            (false, true) => {
                let day = NaiveDate::parse_from_str(&self.text(field), "%Y-%m-%d")
                    .unwrap_or_else(|_| chrono::Local::now().date_naive());
                Some((field, day.with_day(1).unwrap_or(day)))
            }
            _ => None,
        };
        Outcome::Stay
    }

    /// The fields shown, in order.
    pub fn visible(&self) -> Vec<usize> {
        self.shown
            .clone()
            .unwrap_or(0..self.fields.len())
            .filter(|i| !self.hidden.contains(i))
            .collect()
    }

    /// The field after the one the keys are on, among the ones shown;
    /// before it, if `back`. Around at the ends.
    fn step(&self, back: bool) -> usize {
        let visible = self.visible();
        let after = visible.iter().position(|f| *f > self.focused);
        let before = visible.iter().rposition(|f| *f < self.focused);
        let at = match (back, after, before) {
            (false, Some(next), _) => visible.get(next),
            (false, None, _) => visible.first(),
            (true, _, Some(previous)) => visible.get(previous),
            (true, _, None) => visible.last(),
        };
        at.copied().unwrap_or(self.focused)
    }

    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        let (offering, lit) = (self.offering(), self.lit());
        match (ck, offering, lit) {
            (k, _, _) if is_undo(k) => self.undo(true),
            // Alt ↓ opens a drop-down, as a select of the web
            (k, _, _) if k == key!(alt - down) && self.has_calendar(self.focused) => {
                self.toggle_calendar(self.focused)
            }
            (k, _, _) if k == key!(alt - down) => self.open(self.focused),
            // a calendar open: Esc closes it, not the form
            (k, _, _) if k == BACK_KEY && self.picker.is_some() => {
                self.picker = None;
                Outcome::Stay
            }
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

    fn has_calendar(&self, field: usize) -> bool {
        self.fields.get(field).is_some_and(|f| f.calendar)
    }

    fn move_key(&mut self, ck: KeyCombination) -> Outcome {
        match ck {
            k if k == BACK_KEY => Outcome::Cancel,
            k if k == key!(enter) || k == key!(ctrl - enter) => Outcome::Save,
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

    /// The hints as they are on the field the keys are on: in a text of
    /// more lines Enter starts a line, ⌘Enter does what Enter does
    /// elsewhere.
    pub fn on_field(&self, hints: Vec<(String, String)>) -> Vec<(String, String)> {
        let lines = matches!(
            self.fields.get(self.focused).map(|f| &f.value),
            Some(Value::Lines { .. })
        );
        hints
            .into_iter()
            .map(|(k, what)| match (lines, k.as_str()) {
                (true, "Enter") => (SAVE_FROM_LINES.to_string(), what),
                _ => (k, what),
            })
            .collect()
    }

    pub fn view<'a, M: Clone + 'a>(
        &'a self,
        s: &'static Strings,
        c: Colors,
        on: impl Fn(Message) -> M + Copy + 'a,
        aside: Option<Element<'a, M>>,
    ) -> Element<'a, M> {
        let visible = self.visible();
        let fields = self
            .fields
            .iter()
            .enumerate()
            .filter(move |(i, _)| visible.contains(i))
            .map(|(i, f)| {
                let field = self.field(i, f, s, c, on);
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
        let ((no_icon, no), (yes_icon, yes)) = self.actions.clone().unwrap_or_else(|| {
            (
                (theme::icons::CLOSE, s.cancel.to_string()),
                (theme::icons::CHECK, s.save.to_string()),
            )
        });
        let buttons = ui::answers_with(
            Some((no_icon, &no, on(Message::Cancel))),
            self.another
                .iter()
                .map(|(icon, label)| (*icon, label.as_str(), on(Message::Another)))
                .collect(),
            yes_icon,
            &yes,
            Some(on(Message::Save)),
            c,
        );
        let form = ui::panel(
            Some(&self.title),
            column![
                Column::with_children(fields).spacing(14),
                error,
                match self.plain {
                    true => Element::from(iced::widget::space::vertical().height(0)),
                    false => buttons,
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

    /// A month of days to choose from: the arrows on its name, the weeks
    /// from Monday, the day written in the accent, today ringed, the days
    /// already taken marked by a dot.
    fn calendar<'a, M: Clone + 'a>(
        &'a self,
        month: NaiveDate,
        chosen: Option<NaiveDate>,
        s: &'static Strings,
        c: Colors,
        on: impl Fn(Message) -> M + Copy + 'a,
    ) -> Element<'a, M> {
        let today = chrono::Local::now().date_naive();
        let arrow = |label: &'static str, step: i32| {
            button(text(label).size(SIZE_BODY).color(c.text))
                .on_press(on(Message::Month(step)))
                .padding(Padding::from([4, 12]))
                .style(|_, _| button::Style::default())
        };
        let name = format!(
            "{} {}",
            s.months
                .get(month.month0() as usize)
                .copied()
                .unwrap_or_default(),
            month.year()
        );
        let head = row![
            arrow("‹", -1),
            container(
                text(name)
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .color(c.text)
            )
            .width(Length::Fill)
            .center_x(Length::Fill),
            arrow("›", 1),
        ]
        .align_y(Alignment::Center);
        let first =
            month - chrono::Duration::days(i64::from(month.weekday().num_days_from_monday()));
        let weeks =
            (i64::from(month.weekday().num_days_from_monday()) + i64::from(days_in(month)) + 6) / 7;
        let letters = Row::with_children(s.weekdays.iter().map(|w| {
            container(text(*w).size(SIZE_SMALL).color(c.muted))
                .width(Length::FillPortion(1))
                .center_x(Length::FillPortion(1))
                .into()
        }));
        let rows = (0..weeks).map(|w| {
            Row::with_children((0..7).map(|d| {
                let day = first + chrono::Duration::days(w * 7 + d);
                let (here, picked, busy) = (
                    day.month() == month.month(),
                    Some(day) == chosen,
                    self.busy.contains(&day),
                );
                let number =
                    text(day.day().to_string())
                        .size(SIZE_SMALL)
                        .color(match (picked, here) {
                            (true, _) => c.background,
                            (false, true) => c.text,
                            (false, false) => c.muted,
                        });
                let dot = text(match busy {
                    true => "•",
                    false => " ",
                })
                .size(SIZE_SMALL)
                .color(match picked {
                    true => c.background,
                    false => c.accent,
                });
                button(column![number, dot].align_x(Alignment::Center))
                    .width(Length::FillPortion(1))
                    .padding(Padding::from([2, 0]))
                    .on_press(on(Message::Day(day)))
                    .style(move |_, status| button::Style {
                        background: match (picked, status) {
                            (true, _) => Some(c.accent.into()),
                            (false, button::Status::Hovered) => Some(c.border.into()),
                            _ => None,
                        },
                        border: iced::Border {
                            color: c.accent,
                            width: if day == today && !picked { 1.0 } else { 0.0 },
                            radius: 6.0.into(),
                        },
                        ..button::Style::default()
                    })
                    .into()
            }))
            .into()
        });
        container(
            Column::with_children(
                [head.into(), letters.into()]
                    .into_iter()
                    .chain(rows)
                    .collect::<Vec<Element<'a, M>>>(),
            )
            .spacing(2),
        )
        .padding(8)
        .style(move |_| container::Style {
            background: Some(c.background.into()),
            border: iced::Border {
                color: c.border,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
    }

    fn field<'a, M: Clone + 'a>(
        &'a self,
        i: usize,
        f: &'a Field,
        s: &'static Strings,
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
            Value::Lines { content, .. } => field::guard(
                text_editor(content)
                    .id(format!("form-field-{i}"))
                    .placeholder(f.placeholder.as_str())
                    .on_action(move |a| on(Message::Edit(i, a)))
                    .key_binding(move |press| lines_binding(content, press))
                    .size(SIZE_BODY)
                    .min_height(LINES_HEIGHT)
                    .padding(Padding::from([8, 12]))
                    .style(move |_, status| {
                        let active =
                            focused || matches!(status, text_editor::Status::Focused { .. });
                        text_editor::Style {
                            background: c.background.into(),
                            border: edge(active),
                            placeholder: c.muted,
                            value: c.text,
                            selection: iced::Color {
                                a: 0.35,
                                ..c.accent
                            },
                        }
                    }),
                field::Follow::Never,
            )
            .into(),
            Value::Masked { text, .. } if f.calendar => {
                let shown = self.picker.filter(|(at, _)| *at == i);
                let toggle = container(
                    button(ui::icon(
                        theme::icons::CALENDAR,
                        16.0,
                        match shown {
                            Some(_) => c.accent,
                            None => c.muted,
                        },
                    ))
                    .on_press(on(Message::Calendar(i)))
                    .padding(Padding::from([8, 12]))
                    .style(|_, _| button::Style::default()),
                )
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right);
                let line = iced::widget::stack![input(text, field::Follow::Typing), toggle];
                match shown {
                    Some((_, month)) => column![
                        line,
                        self.calendar(
                            month,
                            NaiveDate::parse_from_str(text, "%Y-%m-%d").ok(),
                            s,
                            c,
                            on
                        )
                    ]
                    .spacing(4)
                    .into(),
                    None => line.into(),
                }
            }
            Value::Masked { text, .. } => input(text, field::Follow::Typing),
            Value::Search {
                text: written,
                dropdown,
                top,
                ..
            } => {
                let dropdown = *dropdown;
                // a drop-down shows a window of its rows, from `top`
                let (first, rows) = match dropdown {
                    true => (*top, DROPDOWN_ROWS),
                    false => (0, SUGGESTIONS),
                };
                let total = offered(&f.value).map_or(0, |(all, _)| all.len());
                let shown: Vec<Element<'a, M>> = match offered(&f.value) {
                    Some((suggestions, lit)) if focused => suggestions
                        .iter()
                        .enumerate()
                        .skip(first)
                        .take(rows)
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
                let list: Element<'a, M> = match (shown.is_empty(), dropdown) {
                    (true, _) => iced::widget::space::vertical().height(0).into(),
                    // a drop-down: its window of rows, the wheel moving it,
                    // a bar on the right telling where it is among them all
                    (false, true) => {
                        let portion = |n: usize| u16::try_from(n.max(1)).unwrap_or(u16::MAX);
                        let after = total.saturating_sub(first + rows);
                        let thumb = |n: usize, lit: bool| -> Element<'a, M> {
                            container(iced::widget::space::vertical())
                                .width(4)
                                .height(Length::FillPortion(portion(n)))
                                .style(move |_| container::Style {
                                    background: lit.then(|| c.muted.into()),
                                    border: iced::Border {
                                        radius: 2.0.into(),
                                        ..iced::Border::default()
                                    },
                                    ..container::Style::default()
                                })
                                .into()
                        };
                        let bar: Element<'a, M> = match total > rows {
                            true => column![
                                thumb(first, false),
                                // never too thin to see: a twelfth at least
                                thumb(rows.max(total / 12), true),
                                thumb(after, false)
                            ]
                            .height(Length::Fill)
                            .into(),
                            false => iced::widget::space::horizontal().width(0).into(),
                        };
                        // the bar over the rows, as high as they are
                        let window = iced::widget::stack![
                            container(Column::with_children(shown).spacing(2))
                                .width(Length::Fill)
                                .padding(Padding::default().right(10)),
                            container(bar)
                                .width(Length::Fill)
                                .height(Length::Fill)
                                .align_x(iced::alignment::Horizontal::Right)
                        ];
                        iced::widget::mouse_area(container(window).padding(4).style(move |_| {
                            container::Style {
                                background: Some(c.background.into()),
                                border: edge(false),
                                ..container::Style::default()
                            }
                        }))
                        .on_scroll(move |delta| {
                            let y = match delta {
                                iced::mouse::ScrollDelta::Lines { y, .. } => y,
                                iced::mouse::ScrollDelta::Pixels { y, .. } => y / 20.0,
                            };
                            on(Message::Scroll(i, if y > 0.0 { -1 } else { 1 }))
                        })
                        .into()
                    }
                    (false, false) => container(Column::with_children(shown).spacing(2))
                        .padding(4)
                        .style(move |_| container::Style {
                            background: Some(c.background.into()),
                            border: edge(false),
                            ..container::Style::default()
                        })
                        .into(),
                };
                // the arrow of a drop-down, on the right of the field
                let arrow: Element<'a, M> = match dropdown {
                    true => container(
                        button(
                            text(match offered(&f.value).is_some() {
                                true => "▲",
                                false => "▼",
                            })
                            .size(SIZE_SMALL)
                            .color(c.muted),
                        )
                        .on_press(on(Message::Open(i)))
                        .padding(Padding::from([8, 12]))
                        .style(|_, _| button::Style::default()),
                    )
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Right)
                    .into(),
                    false => iced::widget::space::horizontal().width(0).into(),
                };
                column![
                    iced::widget::stack![input(written, field::Follow::Arrows), ghost, arrow],
                    list
                ]
                .spacing(4)
                .into()
            }
            Value::Check { .. } => iced::widget::space::vertical().height(0).into(),
            Value::Choice { options, index } => {
                // the field is one line high: a long label is cut with an
                // ellipsis (the items are told apart by their index)
                let items: Vec<ChoiceItem> = options
                    .iter()
                    .enumerate()
                    .map(|(n, o)| ChoiceItem::new(n, o))
                    .collect();
                pick_list(
                    items.clone(),
                    items.get(*index).cloned(),
                    move |chosen: ChoiceItem| on(Message::Choose(i, chosen.index)),
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

/// Longest label shown in a drop-down, in characters.
const CHOICE_LABEL_MAX: usize = 48;

/// An option of a drop-down: compared by position, shown cut when long.
#[derive(Clone)]
struct ChoiceItem {
    index: usize,
    label: String,
}

impl ChoiceItem {
    fn new(index: usize, option: &str) -> Self {
        Self {
            index,
            label: scout4all_core::names::cut(option, CHOICE_LABEL_MAX, '…'),
        }
    }
}

impl PartialEq for ChoiceItem {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl Eq for ChoiceItem {}

impl std::fmt::Display for ChoiceItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
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

    /// A text of more lines: what the editor writes is the text, Enter in
    /// it a new line (not a save), ⌘Enter saves, the change is undone.
    #[test]
    fn a_text_of_more_lines_takes_new_lines_and_saves_with_command_enter() {
        use text_editor::{Action, Edit};
        let mut form = Form::new(
            "t",
            vec![Field::text("title", ""), Field::lines("notes", "")],
        );
        for c in "uno".chars() {
            form.update(Message::Edit(1, Action::Edit(Edit::Insert(c))));
        }
        assert_eq!(
            form.update(Message::Edit(1, Action::Edit(Edit::Enter))),
            Outcome::Changed(1)
        );
        for c in "due".chars() {
            form.update(Message::Edit(1, Action::Edit(Edit::Insert(c))));
        }
        assert_eq!(form.text(1), "uno\ndue");
        assert_eq!(form.focused, 1);
        assert_eq!(form.focus_id(), Id::from("form-field-1"));
        // moving the cursor changes nothing
        assert_eq!(
            form.update(Message::Edit(1, Action::Move(text_editor::Motion::Home))),
            Outcome::Stay
        );
        assert_eq!(form.key(key!(ctrl - enter)), Outcome::Save);
        // the footer says it there, Enter elsewhere
        let saves = |form: &Form| -> String {
            let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
            form.on_field(Form::hints(s))
                .into_iter()
                .find(|(_, what)| what == s.save)
                .map(|(k, _)| k)
                .unwrap_or_default()
        };
        assert_eq!(saves(&form), SAVE_FROM_LINES);
        form.update(Message::Focus(0));
        assert_eq!(saves(&form), "Enter");
        form.key(key!(ctrl - z));
        assert_eq!(form.text(1), "uno\n");
    }

    fn press(named: iced::keyboard::key::Named) -> text_editor::KeyPress {
        use iced::keyboard::{
            Key, Modifiers,
            key::{NativeCode, Physical},
        };
        text_editor::KeyPress {
            key: Key::Named(named),
            modified_key: Key::Named(named),
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            modifiers: Modifiers::empty(),
            text: None,
            status: text_editor::Status::Focused { is_hovered: false },
        }
    }

    /// ↑ on the first line and ↓ on the last are left to the form (to the
    /// field before, after), inside they move the cursor; Enter is the
    /// editor's, Tab the form's.
    #[test]
    fn the_arrows_leave_a_text_of_more_lines_only_at_its_ends() {
        use iced::keyboard::key::Named;
        let mut content = text_editor::Content::with_text("uno\ndue");
        let binding =
            |content: &text_editor::Content, key| lines_binding::<()>(content, press(key));
        // the cursor on the first line
        assert!(binding(&content, Named::ArrowUp).is_none());
        assert!(binding(&content, Named::ArrowDown).is_some());
        assert!(binding(&content, Named::Enter).is_some());
        assert!(binding(&content, Named::Tab).is_none());
        content.perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
        assert!(binding(&content, Named::ArrowUp).is_some());
        assert!(binding(&content, Named::ArrowDown).is_none());
    }

    fn countries() -> Vec<Suggestion> {
        (0..30)
            .map(|n| Suggestion {
                label: format!("country {n:02}"),
                value: format!("country {n:02}"),
            })
            .collect()
    }

    /// A drop-down: Alt ↓ (or its arrow) opens all it may be, the window
    /// of its rows following the one lit, the wheel moving it; Enter takes
    /// the one lit, Alt ↓ again closes.
    #[test]
    fn a_drop_down_opens_on_all_of_them_and_follows_the_one_lit() {
        let mut f = Form::new("t", vec![Field::dropdown("country", "country 12")]);
        assert!(!f.offering());
        assert_eq!(f.key(key!(alt - down)), Outcome::Changed(0));
        assert!(f.browsing(0));
        // the screen offers them on the change: all, the one written in
        // sight with a couple above it
        f.suggest(0, countries());
        assert!(f.offering());
        let top = |f: &Form| match &f.fields[0].value {
            Value::Search { top, .. } => *top,
            _ => usize::MAX,
        };
        assert_eq!(top(&f), 10);
        // down past the window: it follows
        (0..DROPDOWN_ROWS + 3).for_each(|_| {
            f.key(key!(down));
        });
        assert_eq!(f.lit(), Some(DROPDOWN_ROWS + 2));
        assert_eq!(top(&f), 3);
        f.update(Message::Scroll(0, 1));
        assert_eq!(top(&f), 4);
        f.update(Message::Scroll(0, 100));
        assert_eq!(top(&f), 30 - DROPDOWN_ROWS);
        assert_eq!(f.key(key!(enter)), Outcome::Changed(0));
        assert_eq!(f.text(0), "country 10");
        assert!(!f.offering() && !f.browsing(0));
        // written: narrowed again, all no more
        f.update(Message::Open(0));
        assert!(f.offering());
        f.update(Message::Open(0));
        assert!(!f.offering());
        f.update(Message::Input(0, "coun".into()));
        assert!(!f.browsing(0));
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

    /// The button of a date opens its month (the one the date holds), the
    /// arrows move it, a day written in the field and the calendar closed;
    /// Esc closes the calendar before the form; a field without the
    /// button has none to open.
    #[test]
    fn a_calendar_chooses_the_day_of_a_date() {
        let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        let mut f = Form::new(
            "t",
            vec![
                Field::masked("d", "2026-11-23", Mask::Date, "").with_calendar(),
                Field::masked("plain", "", Mask::Date, ""),
            ],
        );
        f.update(Message::Calendar(1));
        assert_eq!(f.picker, None);
        f.update(Message::Calendar(0));
        assert_eq!(f.picker, Some((0, day(2026, 11, 1))));
        f.update(Message::Month(2));
        assert_eq!(f.picker, Some((0, day(2027, 1, 1))));
        f.update(Message::Month(-3));
        assert_eq!(f.picker, Some((0, day(2026, 10, 1))));
        assert_eq!(f.key(BACK_KEY), Outcome::Stay);
        assert_eq!(f.picker, None);
        f.update(Message::Calendar(0));
        assert_eq!(f.update(Message::Day(day(2026, 12, 5))), Outcome::Changed(0));
        assert_eq!(f.text(0), "2026-12-05");
        assert_eq!(f.picker, None);
        assert_eq!(f.key(BACK_KEY), Outcome::Cancel);
        assert_eq!(days_in(day(2026, 2, 1)), 28);
        assert_eq!(days_in(day(2028, 2, 1)), 29);
        assert_eq!(days_in(day(2026, 12, 1)), 31);
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

    /// The only suggestion, lit by ↓, is still offered though the field
    /// now has its text: Enter takes it (a person of the registry, whose
    /// details come with it), it doesn't save.
    #[test]
    fn the_only_suggestion_lit_is_taken() {
        let mut f = Form::new("t", vec![Field::search("name", "")]);
        f.update(Message::Input(0, "ross".into()));
        f.suggest(
            0,
            vec![Suggestion {
                label: "Anna Rossi (Cortona 2025)".into(),
                value: "Anna Rossi".into(),
            }],
        );
        f.key(key!(down));
        assert_eq!(f.text(0), "Anna Rossi");
        assert_eq!((f.offering(), f.lit()), (true, Some(0)));
        assert_eq!(f.key(key!(enter)), Outcome::Changed(0));
        assert!(!f.offering());
        assert_eq!(f.key(key!(enter)), Outcome::Save);
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
