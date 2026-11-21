//! What a team (or all of them) does in a period: sessions and matches, in
//! one list (TODO/021 §5). Pure: the provider reads the events, this says
//! which days the periods cover and how the events fall into them.

use crate::training::SessionStatus;
use chrono::{Datelike, Days, Months, NaiveDate, NaiveDateTime};
use uuid::Uuid;

/// Days `[from, to)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl Period {
    pub fn day(date: NaiveDate) -> Self {
        Self::of(date, date.checked_add_days(Days::new(1)))
    }

    /// The week of `date`, Monday to Sunday.
    pub fn week(date: NaiveDate) -> Self {
        let from = super::cycles::monday(date);
        Self::of(from, from.checked_add_days(Days::new(7)))
    }

    /// The month of `date`, from the 1st.
    pub fn month(date: NaiveDate) -> Self {
        let from = date.with_day(1).unwrap_or(date);
        Self::of(from, from.checked_add_months(Months::new(1)))
    }

    fn of(from: NaiveDate, to: Option<NaiveDate>) -> Self {
        Self {
            from,
            to: to.unwrap_or(NaiveDate::MAX),
        }
    }

    pub fn contains(&self, date: NaiveDate) -> bool {
        self.from <= date && date < self.to
    }

    /// Every day of the period, also the empty ones.
    pub fn days(&self) -> Vec<NaiveDate> {
        self.from
            .iter_days()
            .take_while(|day| *day < self.to)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgendaItem {
    Session {
        id: Uuid,
        title: String,
        location: String,
        status: SessionStatus,
        /// Planned and over: to complete or cancel.
        to_close: bool,
    },
    Match {
        id: Uuid,
        /// The other team's name, or the name written for it.
        opponent: String,
        opponent_team: Option<Uuid>,
        state: MatchState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchState {
    Scheduled,
    /// Played (it has sets): sets won by the team of the event and by the
    /// opponent; none when the summary isn't there.
    Played(Option<(u8, u8)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaEvent {
    pub team_id: Uuid,
    pub team_name: String,
    pub start: NaiveDateTime,
    /// A played match lasts from its first event to its last; a match in
    /// programma its duration, or the usual one.
    pub minutes: u16,
    pub item: AgendaItem,
}

/// The sets of the team of the event and of the opponent, from A's side.
pub fn score_of(team_is_a: bool, a_wins: u8, b_wins: u8) -> (u8, u8) {
    match team_is_a {
        true => (a_wins, b_wins),
        false => (b_wins, a_wins),
    }
}

/// Minutes between the first and the last event of a match, at least one.
pub fn played_minutes(first: NaiveDateTime, last: NaiveDateTime) -> u16 {
    u16::try_from((last - first).num_minutes().max(1)).unwrap_or(u16::MAX)
}

/// The events by day, every day of the period present, each day in time order.
pub fn by_day(period: &Period, events: &[AgendaEvent]) -> Vec<(NaiveDate, Vec<AgendaEvent>)> {
    period
        .days()
        .into_iter()
        .map(|day| {
            let mut found: Vec<AgendaEvent> = events
                .iter()
                .filter(|e| e.start.date() == day)
                .cloned()
                .collect();
            found.sort_by_key(|e| (e.start, e.minutes));
            (day, found)
        })
        .collect()
}
