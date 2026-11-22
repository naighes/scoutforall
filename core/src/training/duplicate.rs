//! Duplicating a session to another date (TODO/021 §3): what is copied is
//! the user's choice, and the copy is its own planned session.

use chrono::{Duration, NaiveDateTime};
use uuid::Uuid;

/// The days between a session and the copy the dialog proposes.
pub const PROPOSED_DAYS: i64 = 7;

/// The arguments of `TrainingWriter::duplicate_session`. The title, the
/// gym and the planned duration are always copied; never the
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

/// The most copies of a microcycle one duplication makes.
pub const MAX_CYCLE_COPIES: u16 = 52;

/// The arguments of `TrainingCycles::duplicate_microcycle`: the microcycle
/// is moved `shift_weeks` weeks, `copies` times (the k-th copy `k * shift`
/// weeks away). The copies keep the title and the sessions (same weekday,
/// same time, title, gym, planned duration); never notes, attendance,
/// status or series. The objectives (of the microcycle and of its sessions)
/// and the planned activities are the user's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateCycleInput {
    pub shift_weeks: u16,
    pub copies: u16,
    pub copy_objectives: bool,
    pub copy_activities: bool,
}

impl DuplicateCycleInput {
    /// What the dialog proposes: one copy right after the microcycle, with
    /// both boxes ticked.
    pub fn proposed(weeks: u16) -> Self {
        Self {
            shift_weeks: weeks,
            copies: 1,
            copy_objectives: true,
            copy_activities: true,
        }
    }
}
