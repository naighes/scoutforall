//! The new season of a team (D15): a team with the same name, gender,
//! country and classification, the year after, and the players chosen
//! among the active ones, with their number and role there. The persons
//! are the same, the memberships new; the old team stays as it is.

use crate::{
    localization::Labels,
    player_form::parse_number,
    providers::team_writer::TeamInput,
    shapes::{enums::RoleEnum, player::PlayerEntry, team::TeamEntry},
    team_form::TeamForm,
};
use uuid::Uuid;

/// A player of the team, and whether they go on to the new season, with
/// the number and the role they'll have there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeasonMember {
    pub player: Uuid,
    pub number: u8,
    pub role: Option<RoleEnum>,
    pub chosen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonError {
    /// not a number from 0 to 99
    InvalidNumber,
    /// the number of another player chosen
    NumberInUse,
}

impl SeasonError {
    pub fn message(&self, labels: &Labels) -> String {
        match self {
            SeasonError::InvalidNumber => labels.number_must_be_between_0_and_99,
            SeasonError::NumberInUse => labels.number_already_in_use,
        }
        .to_string()
    }
}

/// The form of the new team: the old one, the year after.
pub fn team_form(team: &TeamEntry) -> TeamForm {
    TeamForm {
        year: team.year.saturating_add(1).to_string(),
        ..TeamForm::of(team)
    }
}

/// The active players of `team`, all chosen, by number.
pub fn members(team: &TeamEntry) -> Vec<SeasonMember> {
    let mut members: Vec<SeasonMember> = team
        .active_players()
        .into_iter()
        .map(|p| SeasonMember {
            player: p.id,
            number: p.number,
            role: p.role,
            chosen: true,
        })
        .collect();
    members.sort_by_key(|m| m.number);
    members
}

/// The members with `player` given `number` and `role`, by number again:
/// a number from 0 to 99 that no other player chosen has.
pub fn correct(
    members: &[SeasonMember],
    player: Uuid,
    number: &str,
    role: Option<RoleEnum>,
) -> Result<Vec<SeasonMember>, SeasonError> {
    let number = parse_number(number).ok_or(SeasonError::InvalidNumber)?;
    let taken = members
        .iter()
        .any(|m| m.chosen && m.player != player && m.number == number);
    match taken {
        true => Err(SeasonError::NumberInUse),
        false => {
            let mut members: Vec<SeasonMember> = members
                .iter()
                .map(|m| match m.player == player {
                    true => SeasonMember {
                        number,
                        role,
                        ..m.clone()
                    },
                    false => m.clone(),
                })
                .collect();
            members.sort_by_key(|m| m.number);
            Ok(members)
        }
    }
}

/// The new team, with the players chosen of `team`: the same persons, with
/// their details, and the numbers and the roles of `members`. Two players
/// chosen with the same number aren't valid (one taken back after their
/// number went to another).
pub fn new_team(
    team: &TeamEntry,
    input: TeamInput,
    members: &[SeasonMember],
) -> Result<TeamInput, SeasonError> {
    let chosen: Vec<&SeasonMember> = members.iter().filter(|m| m.chosen).collect();
    let doubled = chosen
        .iter()
        .enumerate()
        .any(|(i, m)| chosen.iter().skip(i + 1).any(|o| o.number == m.number));
    let players: Vec<PlayerEntry> = chosen
        .iter()
        .filter_map(|m| {
            team.active_players()
                .into_iter()
                .find(|p| p.id == m.player)
                .map(|p| PlayerEntry {
                    number: m.number,
                    role: m.role,
                    ..p.clone()
                })
        })
        .collect();
    match (doubled, input) {
        (true, _) => Err(SeasonError::NumberInUse),
        (
            false,
            TeamInput::New {
                id,
                name,
                year,
                classification,
                gender,
                country,
                ..
            },
        ) => Ok(TeamInput::New {
            id,
            name,
            year,
            classification,
            gender,
            country,
            players,
        }),
        // a new season is a new team, never the old one written over
        (false, TeamInput::Existing(old)) => Ok(TeamInput::New {
            id: None,
            name: old.name,
            year: old.year,
            classification: old.classification,
            gender: old.gender,
            country: old.country,
            players,
        }),
    }
}
