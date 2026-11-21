//! Creating or editing a session, whatever the user interface: the form
//! holds what the user typed and chose, and turns into the input of the
//! `TrainingWriter` once it's valid.

use crate::{
    localization::Labels,
    training::{
        Activity, ActivityHit, ActivityInput, ActivityStatus, DEFAULT_SESSION_MINUTES,
        SessionInput, SessionStatus, TrainingSession,
    },
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

/// What the user filled in for an activity: the minutes are text, as typed,
/// and may be empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityForm {
    pub name: String,
    pub description: String,
    pub planned: String,
    pub actual: String,
    pub notes: String,
    pub status: ActivityStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityFormError {
    EmptyName,
    /// filled in, and not a number of minutes, at least 1
    InvalidPlanned,
    /// filled in, and not a number of minutes, at least 1
    InvalidActual,
}

impl ActivityFormError {
    pub fn message(&self, labels: &Labels) -> String {
        match self {
            ActivityFormError::EmptyName => labels.name_cannot_be_empty,
            ActivityFormError::InvalidPlanned | ActivityFormError::InvalidActual => {
                labels.invalid_duration
            }
        }
        .to_string()
    }
}

impl ActivityForm {
    /// The form of an activity being edited.
    pub fn of(activity: &Activity) -> Self {
        Self {
            name: activity.name.clone(),
            description: activity.description.clone(),
            planned: activity
                .planned_min
                .map(|m| m.to_string())
                .unwrap_or_default(),
            actual: activity
                .actual_min
                .map(|m| m.to_string())
                .unwrap_or_default(),
            notes: activity.notes.clone(),
            status: activity.status,
        }
    }

    /// The form of a copy: its own activity, planned, with the name, the
    /// description and the planned minutes of the one found.
    pub fn copy_of(hit: &ActivityHit) -> Self {
        Self {
            name: hit.name.clone(),
            description: hit.description.clone(),
            planned: hit.planned_min.map(|m| m.to_string()).unwrap_or_default(),
            ..Self::default()
        }
    }

    /// The input of the writer; the texts are trimmed. The first error, in
    /// the order of the fields, when the form isn't valid.
    pub fn validate(self) -> Result<ActivityInput, ActivityFormError> {
        let name = self.name.trim().to_string();
        match (
            name.is_empty(),
            optional_minutes(&self.planned),
            optional_minutes(&self.actual),
        ) {
            (true, _, _) => Err(ActivityFormError::EmptyName),
            (_, None, _) => Err(ActivityFormError::InvalidPlanned),
            (_, _, None) => Err(ActivityFormError::InvalidActual),
            (false, Some(planned_min), Some(actual_min)) => Ok(ActivityInput {
                name,
                description: self.description.trim().to_string(),
                planned_min,
                actual_min,
                notes: self.notes.trim().to_string(),
                status: self.status,
            }),
        }
    }
}

/// Empty is no minutes (`Some(None)`); otherwise a valid number or nothing.
fn optional_minutes(minutes: &str) -> Option<Option<u16>> {
    match minutes.trim().is_empty() {
        true => Some(None),
        false => parse_minutes(minutes).map(Some),
    }
}
