//! What a team (or all of them) does in a period: sessions and matches, in
//! one list (TODO/021 §5). Pure: the provider reads the events, this says
//! which days the periods cover and how the events fall into them.

use crate::training::{SessionStatus, venue::VenueRef};
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
        /// Not an official match.
        friendly: bool,
    },
}

impl AgendaItem {
    /// The id of the session or of the match.
    pub fn id(&self) -> Uuid {
        match self {
            AgendaItem::Session { id, .. } | AgendaItem::Match { id, .. } => *id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchState {
    Scheduled,
    /// It has sets and isn't over: sets won by the team of the event and by
    /// the opponent; none when the summary isn't there.
    InProgress(Option<(u8, u8)>),
    /// Over, with the sets won as above.
    Over(u8, u8),
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
    /// the gym of the session or match, if any
    pub venue: Option<VenueRef>,
    /// another team holds the same gym at the same time: allowed, but shown
    pub shared_venue: bool,
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

/// The events by day, only the days with some, in time order: the list of
/// a long period, where the empty days are not drawn.
pub fn busy_days(events: &[AgendaEvent]) -> Vec<(NaiveDate, Vec<AgendaEvent>)> {
    let mut sorted = events.to_vec();
    sorted.sort_by_key(|e| (e.start, e.minutes));
    sorted.into_iter().fold(vec![], |mut days, event| {
        let day = event.start.date();
        match days.last_mut() {
            Some((last, found)) if *last == day => found.push(event),
            _ => days.push((day, vec![event])),
        }
        days
    })
}

/// What a team has ahead.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Appointments {
    /// Today's sessions and matches, the official ones only still to play.
    pub today: Vec<AgendaEvent>,
    /// The first official match still to play, from today on.
    pub next_match: Option<AgendaEvent>,
    /// The first one after today: shown beside `today`, it is never one of it.
    pub later_match: Option<AgendaEvent>,
}

impl AgendaEvent {
    /// An official match without sets yet: never a friendly, nor a session.
    pub fn official_to_play(&self) -> bool {
        matches!(
            self.item,
            AgendaItem::Match {
                friendly: false,
                state: MatchState::Scheduled,
                ..
            }
        )
    }

    fn official_played(&self) -> bool {
        matches!(
            self.item,
            AgendaItem::Match {
                friendly: false,
                state: MatchState::InProgress(_) | MatchState::Over(..),
                ..
            }
        )
    }
}

/// The appointments of `today` and the official matches ahead, from the
/// events of one team from `today` on.
pub fn appointments(today: NaiveDate, events: &[AgendaEvent]) -> Appointments {
    let mut ahead: Vec<&AgendaEvent> = events.iter().filter(|e| e.start.date() >= today).collect();
    ahead.sort_by_key(|e| (e.start, e.minutes));
    let first_official = |after: bool| {
        ahead
            .iter()
            .find(|e| e.official_to_play() && (!after || e.start.date() > today))
            .map(|e| (*e).clone())
    };
    Appointments {
        today: ahead
            .iter()
            .filter(|e| e.start.date() == today && !e.official_played())
            .map(|e| (*e).clone())
            .collect(),
        next_match: first_official(false),
        later_match: first_official(true),
    }
}

/// The hours a week drawn by the hour shows at least: from 7 to midnight
/// (training goes on until late).
pub const FIRST_HOUR: u32 = 7;
pub const LAST_HOUR: u32 = 24;

/// The hours `[first, last)` a week by the hour shows for events given as
/// (minute of the day it starts, minutes long): 7–24, earlier for an event
/// before; one past midnight stops at it.
pub fn hours(spans: &[(u32, u32)]) -> (u32, u32) {
    spans.iter().fold(
        (FIRST_HOUR, LAST_HOUR),
        |(first, last), &(start, minutes)| {
            (
                first.min(start / 60),
                last.max((start + minutes).div_ceil(60)).min(24),
            )
        },
    )
}

/// Where each event of a day goes across its column, the events given as
/// (minute of the day it starts, minutes long) in order of time: its lane
/// and the lanes of the events it overlaps with, directly or through
/// others (each `1 / lanes` wide). An event shorter than half an hour
/// takes half an hour, so it can be drawn.
pub fn lanes(spans: &[(u32, u32)]) -> Vec<(usize, usize)> {
    let spans: Vec<(u32, u32)> = spans
        .iter()
        .map(|&(start, minutes)| (start, start + minutes.max(30)))
        .collect();
    // the groups: the events overlapping each other, one after another
    let groups = spans.iter().enumerate().fold(
        Vec::<(u32, Vec<usize>)>::new(),
        |mut groups, (i, &(start, end))| {
            match groups.last_mut() {
                Some((group_end, members)) if start < *group_end => {
                    *group_end = (*group_end).max(end);
                    members.push(i);
                }
                _ => groups.push((end, vec![i])),
            }
            groups
        },
    );
    let mut placed = vec![(0, 1); spans.len()];
    for (_, members) in groups {
        // each in the first lane free at its start
        let mut ends: Vec<u32> = vec![];
        let lanes: Vec<(usize, usize)> = members
            .iter()
            .filter_map(|&i| spans.get(i).map(|span| (i, *span)))
            .map(|(i, (start, end))| {
                let lane = ends.iter().position(|e| *e <= start).unwrap_or(ends.len());
                match ends.get_mut(lane) {
                    Some(e) => *e = end,
                    None => ends.push(end),
                }
                (i, lane)
            })
            .collect();
        for (i, lane) in lanes {
            if let Some(slot) = placed.get_mut(i) {
                *slot = (lane, ends.len());
            }
        }
    }
    placed
}
