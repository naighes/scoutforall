//! The totals of a cycle and the attendance of the players (TODO/021 §6).
//! Pure: the provider reads the sessions and the lines in the scope, this
//! counts them.

use crate::training::{
    AttendanceStatus, SessionStatus, TrainingSession,
    cycles::{Level, Window},
};
use chrono::NaiveDateTime;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};
use uuid::Uuid;

/// What a report covers: the whole team, a cycle with everything under it,
/// or the sessions starting in a range of dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    All,
    /// the sessions of the microcycle, of its microcycles for a mesocycle,
    /// of its mesocycles for a program: those attached, not those whose day
    /// falls in its window
    Cycle(Level, Uuid),
    /// start date from `start` included to `end` excluded
    Dates(Window),
}

/// The sessions by status, and the minutes planned against the done.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub planned: u32,
    pub completed: u32,
    pub cancelled: u32,
    /// planned durations of the sessions not cancelled
    pub planned_minutes: u32,
    /// completed sessions: the actual duration, the planned one when empty
    pub actual_minutes: u32,
}

pub fn totals(sessions: &[TrainingSession]) -> Totals {
    sessions.iter().fold(Totals::default(), |mut t, s| {
        match s.status {
            SessionStatus::Planned => t.planned += 1,
            SessionStatus::Completed => {
                t.completed += 1;
                t.actual_minutes += u32::from(s.counted_minutes());
            }
            SessionStatus::Cancelled => t.cancelled += 1,
        }
        if s.status != SessionStatus::Cancelled {
            t.planned_minutes += u32::from(s.duration_min);
        }
        t
    })
}

/// A line of the attendance of a completed session, with the start of the
/// session (the latest line gives the number).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttendanceLine {
    pub session_id: Uuid,
    pub start: NaiveDateTime,
    pub person_id: Uuid,
    /// empty when the person was removed from the registry
    pub name: String,
    pub number: Option<u8>,
    pub status: AttendanceStatus,
    pub guest: bool,
}

/// The attendance of a player: late counts as present (and apart), the
/// unavailable stay out of the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerAttendance {
    pub person_id: Uuid,
    pub name: String,
    /// the one of the latest session
    pub number: Option<u8>,
    /// present and late
    pub present: u32,
    pub late: u32,
    pub absent: u32,
    pub unavailable: u32,
}

impl PlayerAttendance {
    /// The sessions the percentage is on: present, late and absent.
    pub fn base(&self) -> u32 {
        self.present + self.absent
    }

    /// Presences over the base, rounded; none on an empty base.
    pub fn percent(&self) -> Option<u8> {
        let base = u64::from(self.base());
        match base {
            0 => None,
            base => u8::try_from((u64::from(self.present) * 100 + base / 2) / base).ok(),
        }
    }

    /// Higher percentage first, no percentage last, then by name.
    fn order(&self, other: &Self) -> Ordering {
        let (a, b) = (
            u64::from(self.present) * u64::from(other.base()),
            u64::from(other.present) * u64::from(self.base()),
        );
        match (self.base(), other.base()) {
            (0, 0) => Ordering::Equal,
            (0, _) => Ordering::Greater,
            (_, 0) => Ordering::Less,
            _ => b.cmp(&a),
        }
        .then_with(|| self.name.to_lowercase().cmp(&other.name.to_lowercase()))
        .then_with(|| self.person_id.cmp(&other.person_id))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttendanceReport {
    /// completed sessions with the attendance taken: what the report covers
    pub sessions: u32,
    pub players: Vec<PlayerAttendance>,
}

/// The report of the lines: a row for whoever has a line not as a guest,
/// guests count in no row. Ordered by percentage.
pub fn attendance_report(lines: &[AttendanceLine]) -> AttendanceReport {
    let sessions = lines.iter().map(|l| l.session_id).collect::<HashSet<_>>();
    let mut latest = lines.iter().filter(|l| !l.guest).collect::<Vec<_>>();
    latest.sort_by_key(|l| (l.start, l.session_id));
    let mut rows: HashMap<Uuid, PlayerAttendance> = HashMap::new();
    latest.iter().for_each(|l| {
        let p = rows.entry(l.person_id).or_insert_with(|| PlayerAttendance {
            person_id: l.person_id,
            name: String::new(),
            number: None,
            present: 0,
            late: 0,
            absent: 0,
            unavailable: 0,
        });
        p.name = l.name.clone();
        p.number = l.number;
        match l.status {
            AttendanceStatus::Present => p.present += 1,
            AttendanceStatus::Late => {
                p.present += 1;
                p.late += 1;
            }
            AttendanceStatus::Absent => p.absent += 1,
            AttendanceStatus::Unavailable => p.unavailable += 1,
        }
    });
    let mut players: Vec<PlayerAttendance> = rows.into_values().collect();
    players.sort_by(PlayerAttendance::order);
    AttendanceReport {
        sessions: u32::try_from(sessions.len()).unwrap_or(u32::MAX),
        players,
    }
}
