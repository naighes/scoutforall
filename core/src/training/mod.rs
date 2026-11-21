//! The training of a team (TODO/021): the sessions, with their form. The
//! window of a session is local, without offset: the date and time the
//! user wrote, so that daylight saving time moves nothing.

pub mod form;
pub mod overlap;

use chrono::NaiveDateTime;
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
