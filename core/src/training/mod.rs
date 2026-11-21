//! The training of a team (TODO/021): the sessions, with their form. The
//! window of a session is local, without offset: the date and time the
//! user wrote, so that daylight saving time moves nothing.

pub mod form;
pub mod overlap;
pub mod series;

use chrono::{Duration, NaiveDateTime};
use uuid::Uuid;

/// What a session is proposed to last, in minutes.
pub const DEFAULT_SESSION_MINUTES: u16 = 90;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SessionStatus {
    #[default]
    Planned,
    Completed,
    Cancelled,
}

impl SessionStatus {
    pub const ALL: [SessionStatus; 3] = [
        SessionStatus::Planned,
        SessionStatus::Completed,
        SessionStatus::Cancelled,
    ];
}

/// A session of a team, as saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainingSession {
    pub id: Uuid,
    pub team_id: Uuid,
    pub microcycle_id: Option<Uuid>,
    /// the sessions made together by a repetition
    pub series_id: Option<Uuid>,
    pub start: NaiveDateTime,
    pub duration_min: u16,
    pub actual_duration_min: Option<u16>,
    pub location: String,
    pub title: String,
    pub objectives: String,
    pub notes: String,
    pub final_remarks: String,
    pub status: SessionStatus,
}

impl TrainingSession {
    /// Planned and already over (start + duration before `now`): the session
    /// is "to close". The status is never changed on its own.
    pub fn is_to_close(&self, now: NaiveDateTime) -> bool {
        self.status == SessionStatus::Planned
            && self.start + Duration::minutes(i64::from(self.duration_min)) < now
    }

    /// The minutes the totals count: the actual duration, the planned one
    /// when it is empty.
    pub fn counted_minutes(&self) -> u16 {
        self.actual_duration_min.unwrap_or(self.duration_min)
    }
}

/// A valid form, the arguments of `TrainingWriter::create_session` and
/// `update_session`: the team, the series and the id of a session are not
/// changed by an edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInput {
    pub microcycle_id: Option<Uuid>,
    pub start: NaiveDateTime,
    pub duration_min: u16,
    pub actual_duration_min: Option<u16>,
    pub location: String,
    pub title: String,
    pub objectives: String,
    pub notes: String,
    pub final_remarks: String,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActivityStatus {
    #[default]
    Planned,
    Completed,
    Skipped,
}

impl ActivityStatus {
    pub const ALL: [ActivityStatus; 3] = [
        ActivityStatus::Planned,
        ActivityStatus::Completed,
        ActivityStatus::Skipped,
    ];
}

/// An activity of a session, as saved: the session lists them by
/// `position`, the first at 0 (a removed one keeps its place, so the
/// positions may have gaps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub id: Uuid,
    pub session_id: Uuid,
    pub position: u16,
    pub name: String,
    pub description: String,
    pub planned_min: Option<u16>,
    pub actual_min: Option<u16>,
    pub notes: String,
    pub status: ActivityStatus,
}

/// A valid form, the arguments of `TrainingWriter::add_activity` and
/// `update_activity`: the session and the place of an activity are not
/// changed by an edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityInput {
    pub name: String,
    pub description: String,
    pub planned_min: Option<u16>,
    pub actual_min: Option<u16>,
    pub notes: String,
    pub status: ActivityStatus,
}

/// An activity written before, found by name to be copied into a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityHit {
    pub name: String,
    pub description: String,
    pub planned_min: Option<u16>,
    pub team_id: Uuid,
    pub team_name: String,
    /// the start of the session it was in
    pub session_start: NaiveDateTime,
}

/// Where an activity goes in the order of its session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AttendanceStatus {
    #[default]
    Present,
    Absent,
    Late,
    Unavailable,
}

impl AttendanceStatus {
    pub const ALL: [AttendanceStatus; 4] = [
        AttendanceStatus::Present,
        AttendanceStatus::Absent,
        AttendanceStatus::Late,
        AttendanceStatus::Unavailable,
    ];
}

/// A line of the attendance of a session: who, how, and the number they had
/// in that moment (the team's, or their own team's for a guest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttendanceRow {
    pub person_id: Uuid,
    /// empty when the person was removed from the registry (D16)
    pub name: String,
    pub number: Option<u8>,
    pub status: AttendanceStatus,
    pub note: String,
    /// a player of another team
    pub guest: bool,
}

/// The attendance as the session shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttendanceSheet {
    /// written at least once: otherwise `rows` is the active roster of the
    /// team, all present, still to be saved
    pub recorded: bool,
    pub rows: Vec<AttendanceRow>,
}

/// A line to save: `TrainingWriter::save_attendance` writes exactly these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttendanceInput {
    pub person_id: Uuid,
    pub number: Option<u8>,
    pub status: AttendanceStatus,
    pub note: String,
    pub guest: bool,
}

impl From<&AttendanceRow> for AttendanceInput {
    fn from(row: &AttendanceRow) -> Self {
        Self {
            person_id: row.person_id,
            number: row.number,
            status: row.status,
            note: row.note.trim().to_string(),
            guest: row.guest,
        }
    }
}
