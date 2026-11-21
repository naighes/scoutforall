//! Creating or editing a session, whatever the user interface: the form
//! holds what the user typed and chose, and turns into the input of the
//! `TrainingWriter` once it's valid.

use crate::{
    localization::Labels,
    training::{DEFAULT_SESSION_MINUTES, SessionInput, SessionStatus, TrainingSession},
};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// What the user filled in: the durations are text, as typed; the start is
/// missing when the user interface couldn't read it. The actual duration
/// may be empty; so may the texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionForm {
    pub start: Option<NaiveDateTime>,
    pub duration: String,
    pub actual_duration: String,
    pub location: String,
    pub title: String,
    pub objectives: String,
    pub notes: String,
    pub final_remarks: String,
    pub microcycle: Option<Uuid>,
    pub status: SessionStatus,
}

impl Default for SessionForm {
    /// A new session: planned, of the usual duration.
    fn default() -> Self {
        Self {
            start: None,
            duration: DEFAULT_SESSION_MINUTES.to_string(),
            actual_duration: String::new(),
            location: String::new(),
            title: String::new(),
            objectives: String::new(),
            notes: String::new(),
            final_remarks: String::new(),
            microcycle: None,
            status: SessionStatus::Planned,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFormError {
    InvalidStart,
    /// not a number of minutes, at least 1
    InvalidDuration,
    /// filled in, and not a number of minutes, at least 1
    InvalidActualDuration,
}

impl SessionFormError {
    pub fn message(&self, labels: &Labels) -> String {
        use SessionFormError::*;
        match self {
            InvalidStart => labels.invalid_date,
            InvalidDuration | InvalidActualDuration => labels.invalid_duration,
        }
        .to_string()
    }
}

impl SessionForm {
    /// The form of a session being edited.
    pub fn of(session: &TrainingSession) -> Self {
        Self {
            start: Some(session.start),
            duration: session.duration_min.to_string(),
            actual_duration: session
                .actual_duration_min
                .map(|m| m.to_string())
                .unwrap_or_default(),
            location: session.location.clone(),
            title: session.title.clone(),
            objectives: session.objectives.clone(),
            notes: session.notes.clone(),
            final_remarks: session.final_remarks.clone(),
            microcycle: session.microcycle_id,
            status: session.status,
        }
    }

    /// The input of the writer; the texts are trimmed. The first error, in
    /// the order of the fields, when the form isn't valid.
    pub fn validate(self) -> Result<SessionInput, SessionFormError> {
        let actual = self.actual_duration.trim();
        match (
            self.start,
            parse_minutes(&self.duration),
            actual.is_empty(),
            parse_minutes(actual),
        ) {
            (None, _, _, _) => Err(SessionFormError::InvalidStart),
            (_, None, _, _) => Err(SessionFormError::InvalidDuration),
            (_, _, false, None) => Err(SessionFormError::InvalidActualDuration),
            (Some(start), Some(duration_min), _, actual_duration_min) => Ok(SessionInput {
                microcycle_id: self.microcycle,
                start,
                duration_min,
                actual_duration_min,
                location: self.location.trim().to_string(),
                title: self.title.trim().to_string(),
                objectives: self.objectives.trim().to_string(),
                notes: self.notes.trim().to_string(),
                final_remarks: self.final_remarks.trim().to_string(),
                status: self.status,
            }),
        }
    }
}

/// Digits only, from 1 to 65535, surrounding spaces aside.
pub(crate) fn parse_minutes(minutes: &str) -> Option<u16> {
    let minutes = minutes.trim();
    match minutes.chars().all(|c| c.is_ascii_digit()) {
        true => minutes.parse().ok().filter(|m| *m >= 1),
        false => None,
    }
}
