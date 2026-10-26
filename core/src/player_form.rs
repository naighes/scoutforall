//! Adding or editing a player of a team, whatever the user interface: the
//! form holds what the user typed and chose, and turns into the input of
//! the `TeamWriter` once it's valid.

use crate::{
    localization::Labels,
    providers::team_writer::{PlayerDetails, PlayerInput},
    shapes::{enums::RoleEnum, player::PlayerEntry, team::TeamEntry},
};
use chrono::{Datelike, NaiveDate};

/// What the user filled in: the number, the birth date (YYYY-MM-DD) and the
/// height (centimetres) are text, as typed; the last three may be empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerForm {
    pub name: String,
    pub role: Option<RoleEnum>,
    pub number: String,
    pub birth_date: String,
    pub height_cm: String,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerFormError {
    EmptyName,
    MissingRole,
    /// not a number from 0 to 99
    InvalidNumber,
    /// the number of another player of the team, removed ones aside
    NumberInUse,
    /// not a date, before 1900 or after today
    InvalidBirthDate,
    /// not a number of centimetres from 100 to 250
    InvalidHeight,
}

impl PlayerFormError {
    pub fn message(&self, labels: &Labels) -> String {
        use PlayerFormError::*;
        match self {
            EmptyName => labels.name_cannot_be_empty,
            MissingRole => labels.role_is_required,
            InvalidNumber => labels.number_must_be_between_0_and_99,
            NumberInUse => labels.number_already_in_use,
            InvalidBirthDate => labels.invalid_birth_date,
            InvalidHeight => labels.invalid_height,
        }
        .to_string()
    }
}

impl PlayerForm {
    /// The form of a player being edited.
    pub fn of(player: &PlayerEntry) -> Self {
        Self {
            name: player.name.clone(),
            role: player.role,
            number: player.number.to_string(),
            birth_date: player
                .birth_date
                .map(|d| d.format(BIRTH_DATE).to_string())
                .unwrap_or_default(),
            height_cm: player.height_cm.map(|h| h.to_string()).unwrap_or_default(),
            notes: player.notes.clone(),
        }
    }

    /// The input that adds a player to `team`, or updates `existing`,
    /// keeping its id. The name and the notes are trimmed; a birth date
    /// after `today` isn't valid. The first error, in the order of the
    /// fields, when the form isn't valid.
    pub fn validate(
        self,
        team: &TeamEntry,
        existing: Option<&PlayerEntry>,
        today: NaiveDate,
    ) -> Result<PlayerInput, PlayerFormError> {
        let name = self.name.trim().to_string();
        let number = parse_number(&self.number);
        let birth_date = optional(&self.birth_date, |d| parse_birth_date(d, today));
        let height_cm = optional(&self.height_cm, parse_height);
        let existing_id = existing.map(|p| p.id);
        let in_use = |number: u8| {
            team.active_players()
                .iter()
                .any(|p| p.number == number && Some(p.id) != existing_id)
        };
        match (name.is_empty(), self.role, number, birth_date, height_cm) {
            (true, _, _, _, _) => Err(PlayerFormError::EmptyName),
            (_, None, _, _, _) => Err(PlayerFormError::MissingRole),
            (_, _, None, _, _) => Err(PlayerFormError::InvalidNumber),
            (_, _, Some(number), _, _) if in_use(number) => Err(PlayerFormError::NumberInUse),
            (_, _, _, Err(()), _) => Err(PlayerFormError::InvalidBirthDate),
            (_, _, _, _, Err(())) => Err(PlayerFormError::InvalidHeight),
            (false, Some(role), Some(number), Ok(birth_date), Ok(height_cm)) => {
                let details = PlayerDetails {
                    birth_date,
                    height_cm,
                    notes: self.notes.trim().to_string(),
                };
                Ok(match existing {
                    Some(player) => PlayerInput::Existing(PlayerEntry {
                        name,
                        role: Some(role),
                        number,
                        birth_date: details.birth_date,
                        height_cm: details.height_cm,
                        notes: details.notes,
                        ..player.clone()
                    }),
                    None => PlayerInput::New {
                        name,
                        role,
                        number,
                        details,
                    },
                })
            }
        }
    }
}

/// How a birth date is typed and shown: 2005-03-14.
pub const BIRTH_DATE: &str = "%Y-%m-%d";

/// Nothing when empty (surrounding spaces aside), else what `parse` reads.
fn optional<T>(text: &str, parse: impl Fn(&str) -> Option<T>) -> Result<Option<T>, ()> {
    match text.trim() {
        "" => Ok(None),
        text => parse(text).map(Some).ok_or(()),
    }
}

/// A date from 1900 to `today`.
fn parse_birth_date(date: &str, today: NaiveDate) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date, BIRTH_DATE)
        .ok()
        .filter(|d| d.year() >= 1900 && *d <= today)
}

/// Centimetres from 100 to 250.
fn parse_height(height: &str) -> Option<u16> {
    match height.chars().all(|c| c.is_ascii_digit()) {
        true => height.parse().ok().filter(|h| (100..=250).contains(h)),
        false => None,
    }
}

/// One or two digits, surrounding spaces aside.
fn parse_number(number: &str) -> Option<u8> {
    let number = number.trim();
    match (number.len(), number.chars().all(|c| c.is_ascii_digit())) {
        (1 | 2, true) => number.parse().ok(),
        _ => None,
    }
}

/// The input that removes a player: it stays in the team, for the matches
/// already played, but no longer among the active players.
pub fn removal(player: &PlayerEntry) -> PlayerInput {
    PlayerInput::Existing(PlayerEntry {
        deleted: true,
        ..player.clone()
    })
}
