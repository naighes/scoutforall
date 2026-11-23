//! The cycles of the training (TODO/021 §1, §4): program, mesocycle,
//! microcycle. Every one has a window fixed at its creation, never drawn
//! from its children; the window of a child sits inside its parent's. Pure:
//! the provider reads, this says what fits and what to propose.

use crate::localization::Labels;
use chrono::{Datelike, Days, NaiveDate, NaiveDateTime};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Program,
    Mesocycle,
    Microcycle,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Program, Level::Mesocycle, Level::Microcycle];

    /// The level that holds this one.
    pub fn parent(self) -> Option<Level> {
        match self {
            Level::Program => None,
            Level::Mesocycle => Some(Level::Program),
            Level::Microcycle => Some(Level::Mesocycle),
        }
    }

    /// The level this one holds; a microcycle holds sessions, which aren't
    /// a level.
    pub fn child(self) -> Option<Level> {
        match self {
            Level::Program => Some(Level::Mesocycle),
            Level::Mesocycle => Some(Level::Microcycle),
            Level::Microcycle => None,
        }
    }
}

/// How long a cycle lasts: a program says its last day, the others their
/// weeks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    Weeks(u16),
    /// the last day, included
    Until(NaiveDate),
}

/// The days a cycle covers: from `start` to `end`, the latter excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl Window {
    /// The window of a cycle of `level`; none when the span is not the one
    /// of the level (weeks for a microcycle and a mesocycle, a last day not
    /// before the start for a program), or too far.
    pub fn of(level: Level, start: NaiveDate, span: Span) -> Option<Window> {
        match (level, span) {
            (Level::Program, Span::Until(last)) if last >= start => last
                .checked_add_days(Days::new(1))
                .map(|end| Window { start, end }),
            (Level::Mesocycle | Level::Microcycle, Span::Weeks(weeks)) if weeks >= 1 => start
                .checked_add_days(Days::new(7 * u64::from(weeks)))
                .map(|end| Window { start, end }),
            _ => None,
        }
    }

    /// The day of a session: it covers that day only.
    pub fn day(date: NaiveDate) -> Window {
        Window {
            start: date,
            end: date
                .checked_add_days(Days::new(1))
                .unwrap_or(NaiveDate::MAX),
        }
    }

    pub fn contains(&self, inner: &Window) -> bool {
        self.start <= inner.start && inner.end <= self.end
    }

    pub fn has_date(&self, date: NaiveDate) -> bool {
        self.start <= date && date < self.end
    }

    pub fn overlaps(&self, other: &Window) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// The last day covered.
    pub fn last(&self) -> NaiveDate {
        self.end.pred_opt().unwrap_or(self.end)
    }

    /// "2026-11-23 – 2026-12-06", or one date when it is one day.
    pub fn text(&self) -> String {
        match self.last() == self.start {
            true => self.start.format("%Y-%m-%d").to_string(),
            false => format!(
                "{} – {}",
                self.start.format("%Y-%m-%d"),
                self.last().format("%Y-%m-%d")
            ),
        }
    }
}

/// A cycle, as saved. A program has no objectives (they go in its notes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle {
    pub id: Uuid,
    pub team_id: Uuid,
    pub level: Level,
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub span: Span,
    pub window: Window,
    pub objectives: String,
    pub notes: String,
}

/// A valid form, the arguments of `TrainingCycles::create_cycle` and
/// `update_cycle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleInput {
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub start: NaiveDate,
    pub span: Span,
    pub objectives: String,
    pub notes: String,
}

/// What a cycle holds: a cycle of the level below or, for a microcycle, a
/// session (the window is its day).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Child {
    pub id: Uuid,
    pub window: Window,
}

/// What the user filled in: the weeks are text, as typed; a date is missing
/// when the user interface couldn't read it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CycleForm {
    pub title: String,
    pub start: Option<NaiveDate>,
    /// microcycle and mesocycle
    pub weeks: String,
    /// program: the last day
    pub end: Option<NaiveDate>,
    pub objectives: String,
    pub notes: String,
    pub parent: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleFormError {
    InvalidStart,
    /// not a number from 1 to 65535
    InvalidWeeks,
    /// missing, or before the start
    InvalidEnd,
}

impl CycleFormError {
    pub fn message(&self, labels: &Labels) -> String {
        match self {
            CycleFormError::InvalidStart | CycleFormError::InvalidEnd => labels.invalid_date,
            CycleFormError::InvalidWeeks => labels.invalid_weeks,
        }
        .to_string()
    }
}

impl CycleForm {
    /// The form of a cycle being edited.
    pub fn of(cycle: &Cycle) -> Self {
        Self {
            title: cycle.title.clone(),
            start: Some(cycle.window.start),
            weeks: match cycle.span {
                Span::Weeks(weeks) => weeks.to_string(),
                Span::Until(_) => String::new(),
            },
            end: match cycle.span {
                Span::Until(last) => Some(last),
                Span::Weeks(_) => None,
            },
            objectives: cycle.objectives.clone(),
            notes: cycle.notes.clone(),
            parent: cycle.parent_id,
        }
    }

    /// The input of the writer; the texts are trimmed. The first error, in
    /// the order of the fields.
    pub fn validate(self, level: Level) -> Result<CycleInput, CycleFormError> {
        let weeks = self.weeks.trim();
        let parsed = match weeks.chars().all(|c| c.is_ascii_digit()) {
            true => weeks.parse::<u16>().ok().filter(|w| *w >= 1),
            false => None,
        };
        let span = match (self.start, level, parsed, self.end) {
            (None, ..) => Err(CycleFormError::InvalidStart),
            (Some(_), Level::Program, _, Some(last)) => Ok(Span::Until(last)),
            (Some(_), Level::Program, _, None) => Err(CycleFormError::InvalidEnd),
            (Some(_), _, Some(weeks), _) => Ok(Span::Weeks(weeks)),
            (Some(_), _, None, _) => Err(CycleFormError::InvalidWeeks),
        }?;
        match self.start.and_then(|start| Window::of(level, start, span)) {
            Some(window) => Ok(CycleInput {
                parent_id: self.parent,
                title: self.title.trim().to_string(),
                start: window.start,
                span,
                objectives: match level {
                    Level::Program => String::new(),
                    _ => self.objectives.trim().to_string(),
                },
                notes: self.notes.trim().to_string(),
            }),
            None => Err(CycleFormError::InvalidEnd),
        }
    }
}

/// The Monday of the week of `date`.
pub fn monday(date: NaiveDate) -> NaiveDate {
    date.checked_sub_days(Days::new(u64::from(date.weekday().num_days_from_monday())))
        .unwrap_or(date)
}

/// A microcycle for these free sessions: from the Monday of the first to
/// the week of the last. None without sessions.
pub fn proposed_microcycle(starts: &[NaiveDateTime]) -> Option<(NaiveDate, Span)> {
    let first = starts.iter().map(|s| s.date()).min()?;
    let last = starts.iter().map(|s| s.date()).max()?;
    let start = monday(first);
    let weeks = (last - start).num_days() / 7 + 1;
    u16::try_from(weeks)
        .ok()
        .map(|weeks| (start, Span::Weeks(weeks)))
}

/// A container for these free cycles of the level below: from the start of
/// the first to the end of the last (whole weeks for a mesocycle, the last
/// day for a program). None without windows.
pub fn proposed_container(level: Level, windows: &[Window]) -> Option<(NaiveDate, Span)> {
    let start = windows.iter().map(|w| w.start).min()?;
    let end = windows.iter().map(|w| w.end).max()?;
    match level {
        Level::Program => end.pred_opt().map(|last| (start, Span::Until(last))),
        _ => u16::try_from(((end - start).num_days() + 6) / 7)
            .ok()
            .map(|weeks| (start, Span::Weeks(weeks))),
    }
}
