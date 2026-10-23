//! Creating or editing a team, whatever the user interface: the form holds
//! what the user typed and chose, and turns into the input of the
//! `TeamWriter` once it's valid.

use crate::{
    localization::Labels,
    providers::team_writer::TeamInput,
    shapes::{
        enums::{GenderEnum, TeamClassificationEnum},
        team::TeamEntry,
    },
};

/// What the user filled in: the year is text, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamForm {
    pub name: String,
    pub classification: Option<TeamClassificationEnum>,
    pub gender: Option<GenderEnum>,
    pub year: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamFormError {
    EmptyName,
    MissingClassification,
    MissingGender,
    /// not a number of 4 digits
    InvalidYear,
}

impl TeamFormError {
    pub fn message(&self, labels: &Labels) -> String {
        use TeamFormError::*;
        match self {
            EmptyName => labels.name_cannot_be_empty,
            MissingClassification => labels.classification_is_required,
            MissingGender => labels.gender_is_required,
            InvalidYear => labels.year_must_be_a_four_digit_number,
        }
        .to_string()
    }
}

impl TeamForm {
    /// The form of a team being edited.
    pub fn of(team: &TeamEntry) -> Self {
        Self {
            name: team.name.clone(),
            classification: team.classification,
            gender: team.gender,
            year: team.year.to_string(),
        }
    }

    /// The input that creates a team, or updates `existing`, keeping its id
    /// and players. The name is trimmed. The first error, in the order of
    /// the fields, when the form isn't valid.
    pub fn validate(self, existing: Option<&TeamEntry>) -> Result<TeamInput, TeamFormError> {
        let name = self.name.trim().to_string();
        let year = parse_year(&self.year);
        match (name.is_empty(), self.classification, self.gender, year) {
            (true, _, _, _) => Err(TeamFormError::EmptyName),
            (_, None, _, _) => Err(TeamFormError::MissingClassification),
            (_, _, None, _) => Err(TeamFormError::MissingGender),
            (_, _, _, None) => Err(TeamFormError::InvalidYear),
            (false, Some(classification), Some(gender), Some(year)) => Ok(match existing {
                Some(team) => TeamInput::Existing(TeamEntry {
                    name,
                    year,
                    classification: Some(classification),
                    gender: Some(gender),
                    ..team.clone()
                }),
                None => TeamInput::New {
                    id: None,
                    name,
                    year,
                    classification: Some(classification),
                    gender: Some(gender),
                    players: vec![],
                },
            }),
        }
    }
}

/// Exactly 4 digits, surrounding spaces aside.
fn parse_year(year: &str) -> Option<u16> {
    let year = year.trim();
    match (year.len(), year.chars().all(|c| c.is_ascii_digit())) {
        (4, true) => year.parse().ok(),
        _ => None,
    }
}
