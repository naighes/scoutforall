//! Adding or editing a player of a team, whatever the user interface: the
//! form holds what the user typed and chose, and turns into the input of
//! the `TeamWriter` once it's valid.

use crate::{
    localization::Labels,
    providers::team_writer::PlayerInput,
    shapes::{enums::RoleEnum, player::PlayerEntry, team::TeamEntry},
};

/// What the user filled in: the number is text, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerForm {
    pub name: String,
    pub role: Option<RoleEnum>,
    pub number: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerFormError {
    EmptyName,
    MissingRole,
    /// not a number from 0 to 99
    InvalidNumber,
    /// the number of another player of the team, removed ones aside
    NumberInUse,
}

impl PlayerFormError {
    pub fn message(&self, labels: &Labels) -> String {
        use PlayerFormError::*;
        match self {
            EmptyName => labels.name_cannot_be_empty,
            MissingRole => labels.role_is_required,
            InvalidNumber => labels.number_must_be_between_0_and_99,
            NumberInUse => labels.number_already_in_use,
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
        }
    }

    /// The input that adds a player to `team`, or updates `existing`,
    /// keeping its id. The name is trimmed. The first error, in the order
    /// of the fields, when the form isn't valid.
    pub fn validate(
        self,
        team: &TeamEntry,
        existing: Option<&PlayerEntry>,
    ) -> Result<PlayerInput, PlayerFormError> {
        let name = self.name.trim().to_string();
        let number = parse_number(&self.number);
        let existing_id = existing.map(|p| p.id);
        let in_use = |number: u8| {
            team.active_players()
                .iter()
                .any(|p| p.number == number && Some(p.id) != existing_id)
        };
        match (name.is_empty(), self.role, number) {
            (true, _, _) => Err(PlayerFormError::EmptyName),
            (_, None, _) => Err(PlayerFormError::MissingRole),
            (_, _, None) => Err(PlayerFormError::InvalidNumber),
            (_, _, Some(number)) if in_use(number) => Err(PlayerFormError::NumberInUse),
            (false, Some(role), Some(number)) => Ok(match existing {
                Some(player) => PlayerInput::Existing(PlayerEntry {
                    name,
                    role: Some(role),
                    number,
                    ..player.clone()
                }),
                None => PlayerInput::New { name, role, number },
            }),
        }
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
