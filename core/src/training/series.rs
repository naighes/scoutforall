//! The repetition of sessions (TODO/021 §3): a band for each weekday
//! chosen, between two dates; the sessions are real ones sharing a
//! `series_id`, no rule is saved. Pure: the provider reads the busy slots
//! and writes. Dates and times are local and without offset, so daylight
//! saving time, time zones and leap days move nothing: a Monday at 20:30 is
//! 20:30 on every Monday.

use crate::training::{SessionStatus, TrainingSession, overlap::Slot};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

/// The most sessions a repetition makes.
pub const MAX_SESSIONS: usize = 400;

/// The most days a repetition spans (bounds the walk over the dates).
const MAX_DAYS: i64 = 3660;

/// The time and length of the session of a weekday.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    pub weekday: Weekday,
    pub time: NaiveTime,
    pub duration_min: u16,
}

/// What the user asked: bands between two dates, both included, in one
/// place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesInput {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub location: String,
    pub bands: Vec<Band>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesError {
    /// the end is before the start
    InvalidRange,
    NoBands,
    /// two bands for the same weekday
    DuplicateWeekday,
    /// a band shorter than a minute
    InvalidDuration,
    /// more than `MAX_SESSIONS` sessions, or a span too long
    TooMany,
}

impl SeriesInput {
    /// The sessions it makes: start and minutes, by start.
    pub fn generate(&self) -> Result<Vec<(NaiveDateTime, u16)>, SeriesError> {
        let days = (self.to - self.from).num_days();
        let duplicate = self
            .bands
            .iter()
            .enumerate()
            .any(|(i, b)| self.bands[..i].iter().any(|o| o.weekday == b.weekday));
        match (days, self.bands.is_empty(), duplicate) {
            (d, _, _) if d < 0 => Err(SeriesError::InvalidRange),
            (_, true, _) => Err(SeriesError::NoBands),
            (_, _, true) => Err(SeriesError::DuplicateWeekday),
            _ if self.bands.iter().any(|b| b.duration_min == 0) => {
                Err(SeriesError::InvalidDuration)
            }
            (d, _, _) if d > MAX_DAYS => Err(SeriesError::TooMany),
            _ => {
                let all: Vec<(NaiveDateTime, u16)> = self
                    .from
                    .iter_days()
                    .take_while(|day| *day <= self.to)
                    .flat_map(|day| {
                        self.bands
                            .iter()
                            .filter(move |b| b.weekday == day.weekday())
                            .map(move |b| (day.and_time(b.time), b.duration_min))
                    })
                    .collect();
                match all.len() {
                    n if n > MAX_SESSIONS => Err(SeriesError::TooMany),
                    _ => Ok(all),
                }
            }
        }
    }
}

/// A session of the repetition that overlaps something: the slots it hits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesConflict {
    pub start: NaiveDateTime,
    pub minutes: u16,
    pub with: Vec<Slot>,
}

/// What `create_series` did: the sessions written and the dates left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesOutcome {
    pub created: Vec<TrainingSession>,
    pub conflicts: Vec<SeriesConflict>,
}

/// Which sessions of a series a group change touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesScope {
    This,
    ThisAndFollowing,
    All,
}

/// A group change: location, time of day, duration; never the date. What is
/// none stays as it is, so the time is rewritten only when it's changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeriesEdit {
    pub location: Option<String>,
    pub time: Option<NaiveTime>,
    pub duration_min: Option<u16>,
}

/// The sessions `scope` touches among the `members` of the series of
/// `anchor`: only the planned ones. A session in no series is its own
/// group.
pub fn select(
    scope: SeriesScope,
    anchor: &TrainingSession,
    members: &[TrainingSession],
) -> Vec<TrainingSession> {
    let planned = |s: &&TrainingSession| s.status == SessionStatus::Planned;
    match (anchor.series_id, scope) {
        (None, _) | (_, SeriesScope::This) => Some(anchor)
            .iter()
            .copied()
            .filter(planned)
            .cloned()
            .collect(),
        (Some(series), SeriesScope::All) => members
            .iter()
            .filter(|s| s.series_id == Some(series))
            .filter(planned)
            .cloned()
            .collect(),
        (Some(series), SeriesScope::ThisAndFollowing) => members
            .iter()
            .filter(|s| s.series_id == Some(series) && s.start >= anchor.start)
            .filter(planned)
            .cloned()
            .collect(),
    }
}

impl SeriesEdit {
    /// The session as changed: same day, the new time and duration if any.
    pub fn apply(&self, session: &TrainingSession) -> (NaiveDateTime, u16, String) {
        (
            self.time
                .map(|t| session.start.date().and_time(t))
                .unwrap_or(session.start),
            self.duration_min.unwrap_or(session.duration_min),
            self.location
                .as_ref()
                .map(|l| l.trim().to_string())
                .unwrap_or_else(|| session.location.clone()),
        )
    }
}

/// The slots of `moved` windows, in order, that hit `busy` or an earlier
/// one of themselves: what a group change or a repetition mustn't do.
pub fn conflicts_of(
    windows: &[(NaiveDateTime, u16)],
    mut busy: Vec<Slot>,
) -> (Vec<(NaiveDateTime, u16)>, Vec<SeriesConflict>) {
    windows
        .iter()
        .fold((vec![], vec![]), |(mut ok, mut bad), (start, minutes)| {
            let slot = Slot::session(*start, *minutes);
            match crate::training::overlap::clashes(&slot, &busy) {
                with if with.is_empty() => {
                    busy.push(slot);
                    ok.push((*start, *minutes));
                }
                with => bad.push(SeriesConflict {
                    start: *start,
                    minutes: *minutes,
                    with,
                }),
            }
            (ok, bad)
        })
}
