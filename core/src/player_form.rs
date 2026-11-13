//! Adding or editing a player of a team, whatever the user interface: the
//! form holds what the user typed and chose, and turns into the input of
//! the `TeamWriter` once it's valid.

use crate::{
    dvw::import::normalized,
    localization::Labels,
    providers::team_writer::{PlayerDetails, PlayerInput},
    shapes::{enums::RoleEnum, player::PlayerEntry, team::TeamEntry},
};
use chrono::{Datelike, NaiveDate};
use uuid::Uuid;

/// What the user filled in: the number, the birth date (YYYY-MM-DD) and the
/// height (centimetres) are text, as typed; the last three may be empty.
/// A new player may be a person of the registry already (D5): the same
/// person, in one more team.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerForm {
    pub name: String,
    pub role: Option<RoleEnum>,
    pub number: String,
    pub birth_date: String,
    pub height_cm: String,
    pub notes: String,
    /// the person of the registry the new player is, chosen by name
    pub person: Option<Uuid>,
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
            person: None,
        }
    }

    /// The input that adds a player to `team`, or updates `existing`,
    /// keeping its id; a new player with a `person` is that person (once
    /// removed from the team, `existing` is them, back in it). The name
    /// and the notes are trimmed; a birth date after `today` isn't valid.
    /// The first error, in the order of the fields, when the form isn't
    /// valid.
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
        let existing_id = existing.map(|p| p.id).or(self.person);
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
                Ok(match (existing, self.person) {
                    (Some(player), _) => PlayerInput::Existing(PlayerEntry {
                        name,
                        role: Some(role),
                        number,
                        birth_date: details.birth_date,
                        height_cm: details.height_cm,
                        notes: details.notes,
                        // a player removed from the team comes back
                        deleted: false,
                        ..player.clone()
                    }),
                    (None, Some(person)) => PlayerInput::Existing(PlayerEntry {
                        id: person,
                        name,
                        role: Some(role),
                        number,
                        deleted: false,
                        birth_date: details.birth_date,
                        height_cm: details.height_cm,
                        notes: details.notes,
                    }),
                    (None, None) => PlayerInput::New {
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

/// Another active player of `team` with the name of `form`, letter case,
/// accents and punctuation aside: not an error, two players may share a
/// name, but likely the same one added twice, so the user is asked first.
/// None for a player edited without changing the name (the person chosen
/// isn't the namesake of themselves).
pub fn namesake<'a>(
    form: &PlayerForm,
    team: &'a TeamEntry,
    existing: Option<&PlayerEntry>,
) -> Option<&'a PlayerEntry> {
    let name = normalized(&form.name);
    let existing_id = existing.map(|p| p.id).or(form.person);
    let unchanged = existing.is_some_and(|p| normalized(&p.name) == name);
    match (name.is_empty(), unchanged) {
        (false, false) => team
            .active_players()
            .into_iter()
            .find(|p| Some(p.id) != existing_id && normalized(&p.name) == name),
        _ => None,
    }
}

/// What the user is told of the `namesake` before saving.
pub fn namesake_message(namesake: &PlayerEntry, labels: &Labels) -> String {
    labels
        .player_namesake
        .replace("{name}", &namesake.name)
        .replace("{number}", &namesake.number.to_string())
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
pub(crate) fn parse_number(number: &str) -> Option<u8> {
    let number = number.trim();
    match (number.len(), number.chars().all(|c| c.is_ascii_digit())) {
        (1 | 2, true) => number.parse().ok(),
        _ => None,
    }
}

/// A player removed but kept for the matches that tell of them: no longer
/// among the active players, with only what the matches show (name, number,
/// role); the personal details gone.
pub fn removed(player: &PlayerEntry) -> PlayerEntry {
    PlayerEntry {
        deleted: true,
        birth_date: None,
        height_cm: None,
        notes: String::new(),
        ..player.clone()
    }
}

/// The players whose person was removed from the registry (D16) have no
/// name left: they read as a removed player, with the number the team or
/// the match gives them ("#7, removed player").
pub fn name_the_removed(team: &mut TeamEntry, labels: &Labels) {
    team.players
        .iter_mut()
        .filter(|p| p.name.trim().is_empty())
        .for_each(|p| p.name = labels.removed_player.to_string());
}
