//! The data of the versions before the database as rows of the database
//! (`domain`): the same conversion for the migration of the data folder
//! and for the import of the old archives (TODO/001, D4, D5, D9).
//!
//! - A player id in more teams is one person with a membership in each:
//!   the same id was the same person (the `team.json` of each team had a
//!   copy). Their details are those of a copy where the player is active
//!   (a removed player has none left), of the first team by id among
//!   them; copies that differ are told (`SharedTeams::differing`).
//! - A match is scouted on the side of the team of its folder (A), against
//!   its opponent by name; it keeps where it was (`LegacyId`).
//! - Its roster copies number and role from the team of today (the best
//!   one can rebuild), the active team when it has none (up to v0.10.1).
//!   Numbers are unique in a roster: of two players with the same number
//!   (one removed since and the number given to another) the roster keeps
//!   who played the match, then who's still in the team, then the first
//!   listed; the other stays in the events and the lineups, with the
//!   number of the team, as for a player out of the roster.
//! - Every player the match tells of (lineups, events) must be of the
//!   team: the rows of the events refer to the persons.

use crate::{
    domain::{
        LegacyId, Person, TeamRef,
        adapters::{MatchRows, TeamRows},
    },
    errors::{AppError, IOError},
    legacy::folder::{LegacyTeam, NotRead},
    logging::logger::log_info,
    shapes::{r#match::MatchEntry, player::PlayerEntry, team::TeamEntry},
    storage::layout,
};
use std::collections::BTreeMap;
use uuid::Uuid;

/// The teams with their persons shared.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedTeams {
    pub teams: Vec<TeamRows>,
    /// the persons whose details differ among the teams they're active in,
    /// by id
    pub differing: Vec<Uuid>,
}

/// The rows of the teams, in their order: one person per player id,
/// whatever the teams.
pub fn team_rows(teams: &[TeamEntry]) -> Result<SharedTeams, AppError> {
    let mut by_id: Vec<&TeamEntry> = teams.iter().collect();
    by_id.sort_by_key(|t| t.id);
    let copies: Vec<&PlayerEntry> = by_id.iter().flat_map(|t| &t.players).collect();
    let mut chosen: BTreeMap<Uuid, &PlayerEntry> = BTreeMap::new();
    for copy in &copies {
        let better = match chosen.get(&copy.id) {
            None => true,
            Some(kept) => kept.deleted && !copy.deleted,
        };
        if better {
            chosen.insert(copy.id, copy);
        }
    }
    let differing: Vec<Uuid> = chosen
        .values()
        .filter(|kept| {
            copies
                .iter()
                .any(|c| c.id == kept.id && !c.deleted && !same_person(c, kept))
        })
        .map(|kept| kept.id)
        .collect();
    for id in &differing {
        log_info(&format!(
            "player {id}: different details in more teams, kept those of one of them"
        ));
    }
    let teams = teams
        .iter()
        .map(|team| {
            TeamRows::of(team).map(|rows| TeamRows {
                persons: rows
                    .persons
                    .iter()
                    .map(|p| chosen.get(&p.id).map_or_else(|| p.clone(), |c| person(c)))
                    .collect(),
                ..rows
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(SharedTeams { teams, differing })
}

fn person(player: &PlayerEntry) -> Person {
    Person {
        id: player.id,
        name: player.name.clone(),
        birth_date: player.birth_date,
        height_cm: player.height_cm,
        notes: player.notes.clone(),
        deleted: false,
    }
}

fn same_person(a: &PlayerEntry, b: &PlayerEntry) -> bool {
    person(a) == person(b)
}

/// The rows of match `m` of `team` (its team A), as match `id`: `legacy`
/// where it was, if it comes from a data folder.
pub fn match_rows(
    team: &TeamEntry,
    m: &MatchEntry,
    id: Uuid,
    legacy: Option<LegacyId>,
) -> Result<MatchRows, AppError> {
    let played = m.played();
    let mut strangers: Vec<&Uuid> = played
        .iter()
        .filter(|p| team.find_player(**p).is_none())
        .collect();
    strangers.sort();
    match strangers.first() {
        Some(player) => Err(AppError::IO(IOError::Msg(format!(
            "match {}: player {player} not in team {}",
            m.id, team.id
        )))),
        None => {
            let roster = roster(team, m);
            let entry = MatchEntry {
                team: team.with_roster(&roster),
                roster,
                ..m.clone()
            };
            MatchRows::of(&entry, id, TeamRef::Name(m.opponent.clone()), legacy)
        }
    }
}

/// The roster of the match with unique numbers (see the module).
fn roster(team: &TeamEntry, m: &MatchEntry) -> Vec<Uuid> {
    let listed: Vec<Uuid> = match m.roster.as_slice() {
        [] => team.active_players().iter().map(|p| p.id).collect(),
        ids => ids.to_vec(),
    };
    let played = m.played();
    // the lower the better
    let rank = |i: usize, player: &PlayerEntry| (!played.contains(&player.id), player.deleted, i);
    let ranked: Vec<(Uuid, Option<(u8, (bool, bool, usize))>)> = listed
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, team.find_player(*id).map(|p| (p.number, rank(i, p)))))
        .collect();
    ranked
        .iter()
        .filter(|(_, player)| match player {
            // not in the team: `MatchRows::of` tells
            None => true,
            Some((number, rank)) => !ranked
                .iter()
                .any(|(_, other)| matches!(other, Some((n, r)) if n == number && r < rank)),
        })
        .map(|(id, _)| *id)
        .collect()
}

/// The rows of a data folder: the teams read, and each match read, or why
/// it couldn't be read or converted.
#[derive(Debug, Clone)]
pub struct FolderRows {
    pub teams: SharedTeams,
    pub matches: Vec<Result<MatchRows, NotRead>>,
}

/// The rows of the teams of a data folder (see `folder::read_folder`) and
/// of their matches, each with a new id.
pub fn folder_rows(teams: &[LegacyTeam]) -> Result<FolderRows, AppError> {
    let entries: Vec<TeamEntry> = teams.iter().map(|t| t.team.clone()).collect();
    let matches = teams
        .iter()
        .flat_map(|t| t.matches.iter().map(move |m| (&t.team, m)))
        .map(|(team, m)| {
            m.clone().and_then(|m| {
                let legacy = LegacyId {
                    id: m.id.clone(),
                    team: team.id,
                };
                match_rows(team, &m, Uuid::new_v4(), Some(legacy)).map_err(|e| NotRead {
                    folder: layout::match_folder(&team.id, &m.id),
                    reason: e.to_string(),
                })
            })
        })
        .collect();
    Ok(FolderRows {
        teams: team_rows(&entries)?,
        matches,
    })
}
