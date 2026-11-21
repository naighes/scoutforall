//! Duplicating a session to another date (TODO/021 §3): what is copied is
//! the user's choice, and the copy is its own planned session.

use chrono::{Duration, NaiveDateTime};
use uuid::Uuid;

/// The days between a session and the copy the dialog proposes.
pub const PROPOSED_DAYS: i64 = 7;

/// The arguments of `TrainingWriter::duplicate_session`. The title, the
/// location and the planned duration are always copied; never the
/// attendance, the notes, the final remarks, the actual durations, the
/// status or the series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateInput {
    pub start: NaiveDateTime,
    pub microcycle_id: Option<Uuid>,
    pub copy_objectives: bool,
    pub copy_activities: bool,
}

impl DuplicateInput {
    /// What the dialog proposes: a week later at the same time, with both
    /// boxes ticked.
    pub fn proposed(session_start: NaiveDateTime, microcycle_id: Option<Uuid>) -> Self {
        Self {
            start: session_start + Duration::days(PROPOSED_DAYS),
            microcycle_id,
            copy_objectives: true,
            copy_activities: true,
        }
    }
}
