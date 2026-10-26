//! Creating or editing a match, whatever the user interface: the form holds
//! what the user typed and chose, and turns into what the `MatchWriter`
//! saves once it's valid.

use crate::{
    constants::DEFAULT_MAX_SUBSTITUTIONS,
    localization::Labels,
    shapes::{enums::EventTypeEnum, r#match::MatchEntry},
};
use chrono::{DateTime, FixedOffset};

/// What the user filled in: the substitutions are text, as typed; the date
/// is missing when the user interface couldn't read it; the notes may be
/// empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchForm {
    pub opponent: String,
    pub date: Option<DateTime<FixedOffset>>,
    pub home: bool,
    pub max_substitutions: String,
    pub notes: String,
}

/// A valid form, the arguments of `MatchWriter::create` and `update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchInput {
    pub opponent: String,
    pub date: DateTime<FixedOffset>,
    pub home: bool,
    pub max_substitutions: u8,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchFormError {
    EmptyOpponent,
    InvalidDate,
    /// not a number from 1 to 99
    InvalidMaxSubstitutions,
    /// fewer than the substitutions already made in a set of the match
    MaxSubstitutionsBelowUsed,
}

impl MatchFormError {
    pub fn message(&self, labels: &Labels) -> String {
        use MatchFormError::*;
        match self {
            EmptyOpponent => labels.opponent_cannot_be_empty,
            InvalidDate => labels.invalid_date,
            InvalidMaxSubstitutions => labels.invalid_max_substitutions,
            MaxSubstitutionsBelowUsed => labels.max_substitutions_below_existing_substitutions,
        }
        .to_string()
    }
}

impl MatchForm {
    /// The form of a new match, on `date`.
    pub fn new(date: DateTime<FixedOffset>) -> Self {
        Self {
            opponent: String::new(),
            date: Some(date),
            home: false,
            max_substitutions: DEFAULT_MAX_SUBSTITUTIONS.to_string(),
            notes: String::new(),
        }
    }

    /// The form of a match being edited.
    pub fn of(m: &MatchEntry) -> Self {
        Self {
            opponent: m.opponent.clone(),
            date: Some(m.date),
            home: m.home,
            max_substitutions: m.max_substitutions.to_string(),
            notes: m.notes.clone(),
        }
    }

    /// What creates a match, or updates `existing`. The opponent and the
    /// notes are trimmed. The first error, in the order of the fields, when the form
    /// isn't valid.
    pub fn validate(self, existing: Option<&MatchEntry>) -> Result<MatchInput, MatchFormError> {
        let opponent = self.opponent.trim().to_string();
        let max_substitutions = parse_max_substitutions(&self.max_substitutions);
        let used = existing.map(used_substitutions).unwrap_or(0);
        match (opponent.is_empty(), self.date, max_substitutions) {
            (true, _, _) => Err(MatchFormError::EmptyOpponent),
            (_, None, _) => Err(MatchFormError::InvalidDate),
            (_, _, None) => Err(MatchFormError::InvalidMaxSubstitutions),
            (_, _, Some(max)) if max < used => Err(MatchFormError::MaxSubstitutionsBelowUsed),
            (false, Some(date), Some(max_substitutions)) => Ok(MatchInput {
                opponent,
                date,
                home: self.home,
                max_substitutions,
                notes: self.notes.trim().to_string(),
            }),
        }
    }
}

/// One or two digits, surrounding spaces aside, and not zero.
fn parse_max_substitutions(max: &str) -> Option<u8> {
    let max = max.trim();
    match (max.len(), max.chars().all(|c| c.is_ascii_digit())) {
        (1 | 2, true) => max.parse().ok().filter(|max| *max > 0),
        _ => None,
    }
}

/// The most substitutions made in a set of the match.
fn used_substitutions(m: &MatchEntry) -> u8 {
    m.sets
        .iter()
        .map(|s| {
            s.events
                .iter()
                .filter(|e| e.event_type == EventTypeEnum::R)
                .count()
        })
        .max()
        .map(|used| u8::try_from(used).unwrap_or(u8::MAX))
        .unwrap_or(0)
}
