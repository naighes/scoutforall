//! Creating or editing a match, whatever the user interface: the form holds
//! what the user typed and chose, and turns into what the `MatchWriter`
//! saves once it's valid.

use crate::{
    constants::{DEFAULT_MAX_SUBSTITUTIONS, MAX_ROSTER, MAX_ROSTER_WITHOUT_LIBERO, MIN_ROSTER},
    domain::{Competition, CompetitionFormat, Home, Match, Stage, TeamRef},
    localization::Labels,
    shapes::{
        enums::{EventTypeEnum, RoleEnum},
        r#match::MatchEntry,
        player::PlayerEntry,
        team::TeamEntry,
    },
};
use chrono::{DateTime, FixedOffset};
use std::collections::HashSet;
use uuid::Uuid;

/// What the user filled in: the substitutions are text, as typed; the date
/// is missing when the user interface couldn't read it; the notes may be
/// empty. The roster, the players on the score sheet, is none until chosen:
/// then the active players of the team for a new match, and no roster for
/// a match saved before v0.11 without one.
///
/// Team B is `opponent_team` when chosen among the teams (`opponent` its
/// name), else the name written, made a team without players when
/// `create_opponent`. The competition is `competition_id` when chosen,
/// else the name written, a new one of `competition_format` (a league if
/// none); none when empty. `neutral` wins over `home`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchForm {
    pub opponent: String,
    pub date: Option<DateTime<FixedOffset>>,
    pub home: bool,
    pub max_substitutions: String,
    pub notes: String,
    pub roster: Option<Vec<Uuid>>,
    pub opponent_team: Option<Uuid>,
    pub create_opponent: bool,
    pub neutral: bool,
    pub friendly: bool,
    pub competition: String,
    pub competition_id: Option<Uuid>,
    pub competition_format: Option<CompetitionFormat>,
    pub stage: Option<Stage>,
    pub round: String,
}

/// Team B as the form saves it (D3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opponent {
    Team(Uuid),
    Name(String),
    /// a new team, without players, with this name
    NewTeam(String),
}

/// The competition as the form saves it (D12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompetitionInput {
    Existing(Uuid),
    New {
        name: String,
        format: CompetitionFormat,
    },
}

/// A player that can be on the score sheet, and whether it is.
#[derive(Debug, Clone)]
pub struct RosterChoice {
    pub player: PlayerEntry,
    pub chosen: bool,
    /// played in a set of the match: on the score sheet for good
    pub played: bool,
}

/// A valid form, the arguments of `MatchWriter::create` and `update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchInput {
    pub opponent: Opponent,
    pub date: DateTime<FixedOffset>,
    pub home: Home,
    pub max_substitutions: u8,
    pub notes: String,
    /// empty for a match saved without one, which keeps the whole team
    pub roster: Vec<Uuid>,
    pub friendly: bool,
    pub competition: Option<CompetitionInput>,
    pub stage: Option<Stage>,
    pub round: Option<String>,
}

impl MatchInput {
    /// A match against the name `opponent`, played by A at home or away,
    /// in no competition: what the matches were before the database.
    pub fn against(
        opponent: impl Into<String>,
        date: DateTime<FixedOffset>,
        home: bool,
        max_substitutions: u8,
        notes: impl Into<String>,
        roster: Vec<Uuid>,
    ) -> Self {
        Self {
            opponent: Opponent::Name(opponent.into()),
            date,
            home: Home::from_a(home),
            max_substitutions,
            notes: notes.into(),
            roster,
            friendly: false,
            competition: None,
            stage: None,
            round: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchFormError {
    EmptyOpponent,
    /// team B is team A
    OpponentIsTeamA,
    InvalidDate,
    /// not a number from 1 to 99
    InvalidMaxSubstitutions,
    /// fewer than the substitutions already made in a set of the match
    MaxSubstitutionsBelowUsed,
    /// fewer than six and the libero
    RosterTooSmall,
    /// more than the score sheet takes
    RosterTooLarge,
    /// without a player who already played in a set of the match
    RosterWithoutPlayed,
    /// more than 12 players and no libero among them: the score sheet
    /// takes 13 or 14 only with a libero
    RosterWithoutLibero,
}

impl MatchFormError {
    /// An error of the roster, not of the other fields.
    pub fn is_roster(self) -> bool {
        matches!(
            self,
            MatchFormError::RosterTooSmall
                | MatchFormError::RosterTooLarge
                | MatchFormError::RosterWithoutPlayed
                | MatchFormError::RosterWithoutLibero
        )
    }

    pub fn message(&self, labels: &Labels) -> String {
        use MatchFormError::*;
        match self {
            EmptyOpponent => labels.opponent_cannot_be_empty,
            OpponentIsTeamA => labels.opponent_is_team_a,
            InvalidDate => labels.invalid_date,
            InvalidMaxSubstitutions => labels.invalid_max_substitutions,
            MaxSubstitutionsBelowUsed => labels.max_substitutions_below_existing_substitutions,
            RosterTooSmall => labels.roster_too_small,
            RosterTooLarge => labels.roster_too_large,
            RosterWithoutPlayed => labels.roster_without_played,
            RosterWithoutLibero => labels.roster_without_libero,
        }
        .to_string()
    }
}

impl MatchForm {
    /// The form of a new match, on `date`.
    pub fn new(date: DateTime<FixedOffset>) -> Self {
        Self {
            date: Some(date),
            max_substitutions: DEFAULT_MAX_SUBSTITUTIONS.to_string(),
            ..Self::default()
        }
    }

    /// The form of a match being edited: `m` from A's side, `row` its row,
    /// `competition` the one it's played in.
    pub fn of(m: &MatchEntry, row: &Match, competition: Option<&Competition>) -> Self {
        Self {
            opponent: m.opponent.clone(),
            date: Some(m.date),
            home: row.home == Home::A,
            max_substitutions: m.max_substitutions.to_string(),
            notes: m.notes.clone(),
            roster: Some(m.roster.clone()).filter(|r| !r.is_empty()),
            opponent_team: match row.team_b {
                TeamRef::Team(id) => Some(id),
                TeamRef::Name(_) => None,
            },
            create_opponent: false,
            neutral: row.home == Home::Neutral,
            friendly: row.friendly,
            competition: competition.map(|c| c.name.clone()).unwrap_or_default(),
            competition_id: competition.map(|c| c.id),
            competition_format: competition.map(|c| c.format),
            stage: row.stage,
            round: row.round.clone().unwrap_or_default(),
        }
    }

    /// Where the match is played: `neutral` wins over `home`.
    fn court(&self) -> Home {
        match (self.neutral, self.home) {
            (true, _) => Home::Neutral,
            (false, home) => Home::from_a(home),
        }
    }

    /// Team B: the team chosen, else the name written (trimmed), a new team
    /// if asked; none when nothing is written.
    fn opponent_input(&self) -> Option<Opponent> {
        let name = self.opponent.trim().to_string();
        match (self.opponent_team, name.is_empty(), self.create_opponent) {
            (Some(id), _, _) => Some(Opponent::Team(id)),
            (None, true, _) => None,
            (None, false, true) => Some(Opponent::NewTeam(name)),
            (None, false, false) => Some(Opponent::Name(name)),
        }
    }

    /// The competition: the one chosen, else a new one with the name
    /// written (trimmed); none when nothing is written.
    fn competition_input(&self) -> Option<CompetitionInput> {
        let name = self.competition.trim().to_string();
        match (self.competition_id, name.is_empty()) {
            (Some(id), _) => Some(CompetitionInput::Existing(id)),
            (None, true) => None,
            (None, false) => Some(CompetitionInput::New {
                name,
                format: self.competition_format.unwrap_or(CompetitionFormat::League),
            }),
        }
    }

    /// Who can be on the score sheet of a match of `team`, or of `existing`,
    /// by number: the active players, and those of the roster or who played
    /// even if removed since. Chosen as in the roster; until it's chosen,
    /// the active players, and also who played in a match saved without a
    /// roster.
    pub fn roster_choices(
        &self,
        team: &TeamEntry,
        existing: Option<&MatchEntry>,
    ) -> Vec<RosterChoice> {
        let played = existing.map(MatchEntry::played).unwrap_or_default();
        let kept: HashSet<Uuid> = existing
            .map(|m| m.roster.iter().chain(&played).copied().collect())
            .unwrap_or_default();
        let chosen = self.chosen(team, existing);
        let mut choices: Vec<RosterChoice> = team
            .players
            .iter()
            .filter(|p| !p.deleted || kept.contains(&p.id))
            .map(|p| RosterChoice {
                player: p.clone(),
                chosen: chosen.contains(&p.id),
                played: played.contains(&p.id),
            })
            .collect();
        choices.sort_by_key(|c| c.player.number);
        choices
    }

    /// Puts `player` on the score sheet, or takes it off unless it played.
    pub fn toggle(&mut self, player: Uuid, team: &TeamEntry, existing: Option<&MatchEntry>) {
        let choices = self.roster_choices(team, existing);
        match choices.iter().find(|c| c.player.id == player) {
            Some(RosterChoice {
                chosen: true,
                played: true,
                ..
            })
            | None => {}
            Some(_) => {
                self.roster = Some(
                    choices
                        .iter()
                        .filter(|c| c.chosen != (c.player.id == player))
                        .map(|c| c.player.id)
                        .collect(),
                )
            }
        }
    }

    /// The players chosen, by `roster_choices`.
    fn chosen(&self, team: &TeamEntry, existing: Option<&MatchEntry>) -> HashSet<Uuid> {
        match (&self.roster, existing) {
            (Some(roster), _) => roster.iter().copied().collect(),
            (None, Some(m)) => team
                .active_players()
                .iter()
                .map(|p| p.id)
                .chain(m.played())
                .collect(),
            (None, None) => team.active_players().iter().map(|p| p.id).collect(),
        }
    }

    /// What creates a match of `team`, or updates `existing`. The opponent,
    /// the notes, the competition and the round are trimmed; the roster is by number, without whoever
    /// can't be on it. The first error, in the order of the fields, when the
    /// form isn't valid.
    pub fn validate(
        self,
        team: &TeamEntry,
        existing: Option<&MatchEntry>,
    ) -> Result<MatchInput, MatchFormError> {
        let opponent = self.opponent_input();
        let max_substitutions = parse_max_substitutions(&self.max_substitutions);
        let used = existing.map(used_substitutions).unwrap_or(0);
        let roster = self.valid_roster(team, existing);
        match (opponent, self.date, max_substitutions, roster) {
            (None, _, _, _) => Err(MatchFormError::EmptyOpponent),
            (Some(Opponent::Team(b)), _, _, _) if b == team.id => {
                Err(MatchFormError::OpponentIsTeamA)
            }
            (_, None, _, _) => Err(MatchFormError::InvalidDate),
            (_, _, None, _) => Err(MatchFormError::InvalidMaxSubstitutions),
            (_, _, Some(max), _) if max < used => Err(MatchFormError::MaxSubstitutionsBelowUsed),
            (_, _, _, Err(e)) => Err(e),
            (Some(opponent), Some(date), Some(max_substitutions), Ok(roster)) => Ok(MatchInput {
                opponent,
                date,
                home: self.court(),
                max_substitutions,
                notes: self.notes.trim().to_string(),
                roster,
                friendly: self.friendly,
                competition: self.competition_input(),
                stage: self.stage,
                round: Some(self.round.trim().to_string()).filter(|r| !r.is_empty()),
            }),
        }
    }

    /// The roster to save: none for a match saved without one and left so,
    /// else from `MIN_ROSTER` to `MAX_ROSTER` players with all who played,
    /// and a libero when more than `MAX_ROSTER_WITHOUT_LIBERO`.
    fn valid_roster(
        &self,
        team: &TeamEntry,
        existing: Option<&MatchEntry>,
    ) -> Result<Vec<Uuid>, MatchFormError> {
        let chosen: Vec<PlayerEntry> = self
            .roster_choices(team, existing)
            .into_iter()
            .filter(|c| c.chosen)
            .map(|c| c.player)
            .collect();
        let with_libero = chosen.iter().any(|p| p.role == Some(RoleEnum::Libero));
        let roster: Vec<Uuid> = chosen.iter().map(|p| p.id).collect();
        let played = existing.map(MatchEntry::played).unwrap_or_default();
        let undeclared = self.roster.is_none() && existing.is_some_and(|m| m.roster.is_empty());
        match roster.len() {
            _ if undeclared => Ok(vec![]),
            n if n < MIN_ROSTER => Err(MatchFormError::RosterTooSmall),
            n if n > MAX_ROSTER => Err(MatchFormError::RosterTooLarge),
            _ if !played.iter().all(|p| roster.contains(p)) => {
                Err(MatchFormError::RosterWithoutPlayed)
            }
            n if n > MAX_ROSTER_WITHOUT_LIBERO && !with_libero => {
                Err(MatchFormError::RosterWithoutLibero)
            }
            _ => Ok(roster),
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
