//! The archives exported by the versions before the database, imported
//! into it (TODO/001, step 13): converted by `legacy` as the migration of
//! the data folder does (`legacy::rows`), then merged as the archives of
//! the database are (`archive::merge_staged`, D10). Which archive it is
//! the manifest tells (`open`): one without it is of a version before.
//!
//! - A **team** (`team.json`) has its id inside, or (up to v0.9.0) only in
//!   the name of its file (`file_id`). Already here, it merges as a team
//!   archive of today: the local data win, its missing players are added
//!   after those here, one removed here comes back.
//! - A **match** (`match.json` and its sets) comes without its team: the
//!   team here it goes into is chosen among those it may go into (`open`:
//!   its players are all of the team, every team for a match without
//!   sets; not the ones that have it already). Its roster is the one of
//!   the archive kept to the team, with who played after it (as the
//!   import before the database), its numbers and roles those of the team
//!   of today; the team itself doesn't change.
//! - The match is known by where it was (`LegacyId`: the team it goes
//!   into and its id of then, opponent and day), as a migrated one: the
//!   same match imported again, or migrated from the data folder, is here
//!   and never overwritten; one removed here comes back when it's the
//!   same.
//! - A set that doesn't replay with this version refuses the match, with
//!   why, and nothing is written.

use crate::{
    db,
    domain::{LegacyId, adapters::TeamRows},
    errors::{AppError, IOError},
    legacy::{
        archive::{Archive, is_match_of, read_archive, roster_for},
        migrate::{already, insert_team_rows},
        rows::match_rows,
    },
    localization::{Labels, Language},
    providers::sqlite::{
        Database,
        archive::{Imported, Kind, Manifest, copy_team, manifest, merge_staged},
        not_found,
        rows::{insert_match, team_ids, whole_team_rows},
        sqlite,
    },
    shapes::{r#match::MatchEntry, team::TeamEntry},
};
use rusqlite::{Connection, Transaction};
use uuid::Uuid;

/// What an archive is, e.g. to show it before importing it.
#[derive(Debug, Clone)]
pub enum Opened {
    /// an archive of the database (`archive::import`)
    Database(Manifest),
    /// a team of a version before, with its id when the archive has it
    /// (`import_legacy_team`)
    LegacyTeam { team: TeamEntry, id: Option<Uuid> },
    /// a match of a version before, and the teams here it may go into,
    /// never none (`import_legacy_match`)
    LegacyMatch {
        archived: MatchEntry,
        teams: Vec<TeamEntry>,
    },
}

fn message(label: &str) -> AppError {
    AppError::IO(IOError::Msg(label.to_string()))
}

/// What the archive is: a match of a version before that no team here
/// may take (`match_of_another_team`), or that all those that may have
/// already (`match_already_exists`), is an error.
pub fn open(db: &Database, archive: &[u8], labels: &Labels) -> Result<Opened, AppError> {
    match manifest(archive) {
        Some(manifest) => Ok(Opened::Database(manifest)),
        None => match read_archive(archive, labels)? {
            Archive::Team { team, id } => Ok(Opened::LegacyTeam { team, id }),
            Archive::Match(archived) => db
                .read(|tx| teams_for(tx, &archived, labels))
                .map(|teams| Opened::LegacyMatch { archived, teams }),
        },
    }
}

/// The teams here the match may go into (see `open`), by name.
fn teams_for(
    tx: &Transaction,
    archived: &MatchEntry,
    labels: &Labels,
) -> Result<Vec<TeamEntry>, AppError> {
    let ours: Vec<TeamEntry> = team_ids(tx)?
        .iter()
        .filter_map(|id| Uuid::parse_str(id).ok())
        .map(|id| whole_team(tx, id))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|team| is_match_of(archived, team))
        .collect();
    let free: Vec<TeamEntry> = ours
        .iter()
        .map(|team| here(tx, team.id, archived).map(|here| (team, here)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(_, here)| !matches!(here, Some((_, false))))
        .map(|(team, _)| team.clone())
        .collect();
    match (ours.is_empty(), free.is_empty()) {
        (true, _) => Err(message(labels.match_of_another_team)),
        (false, true) => Err(message(labels.match_already_exists)),
        (false, false) => Ok(free),
    }
}

/// The team with all its members, the removed ones too: a match of an
/// archive may tell of one no match here tells of.
fn whole_team(tx: &Transaction, team: Uuid) -> Result<TeamEntry, AppError> {
    whole_team_rows(tx, team)?
        .ok_or_else(|| not_found(&format!("team {team}")))?
        .entry()
}

/// The match of the archive in team `team` here, by where it was, and
/// whether it's removed.
fn here(
    tx: &Transaction,
    team: Uuid,
    archived: &MatchEntry,
) -> Result<Option<(Uuid, bool)>, AppError> {
    already(tx, &legacy_id(team, archived))
}

fn legacy_id(team: Uuid, archived: &MatchEntry) -> LegacyId {
    LegacyId {
        id: archived.id.clone(),
        team,
    }
}

/// A database in memory with the rows `write` writes, to merge.
fn staged(
    write: impl FnOnce(&Transaction) -> Result<(), AppError>,
) -> Result<Connection, AppError> {
    let mut staging = db::open_in_memory()?;
    let tx = staging.transaction().map_err(sqlite)?;
    write(&tx)?;
    tx.commit().map_err(sqlite)?;
    Ok(staging)
}

/// Adds the team of an archive of a version before, with its id or
/// `file_id` (the name of its file, up to v0.9.0): see the module.
pub fn import_legacy_team(
    db: &Database,
    archive: &[u8],
    file_id: Option<Uuid>,
    language: &Language,
) -> Result<Imported, AppError> {
    let labels = language.labels();
    let (team, id) = match read_archive(archive, labels)? {
        Archive::Team { team, id } => Ok((team, id.or(file_id))),
        Archive::Match(_) => Err(message(labels.invalid_archive)),
    }?;
    let team = TeamEntry {
        id: id.ok_or_else(|| message(labels.team_archive_without_id))?,
        ..team
    };
    let rows = TeamRows::of(&team)?;
    let mut staging = staged(|tx| insert_team_rows(tx, &rows))?;
    merge_staged(db, &mut staging, Kind::Team, team.id, language)
}

/// Adds the match of an archive of a version before to team `team` here,
/// one of those `open` tells: see the module.
pub fn import_legacy_match(
    db: &Database,
    archive: &[u8],
    team: Uuid,
    language: &Language,
) -> Result<Imported, AppError> {
    let labels = language.labels();
    let archived = match read_archive(archive, labels)? {
        Archive::Match(m) => Ok(m),
        Archive::Team { .. } => Err(message(labels.invalid_archive)),
    }?;
    // a set this version doesn't replay: nothing is written
    archived
        .replay_issue()
        .map_or(Ok(()), |issue| Err(message(&issue.describe(labels))))?;
    let (entry, id) = db.read(|tx| {
        let entry = whole_team(tx, team)?;
        // the same match again keeps its id: here, or removed here
        let id = here(tx, team, &archived)?.map_or_else(Uuid::new_v4, |(id, _)| id);
        Ok((entry, id))
    })?;
    match is_match_of(&archived, &entry) {
        true => Ok(()),
        false => Err(message(labels.match_of_another_team)),
    }?;
    let m = MatchEntry {
        roster: roster_for(&archived, &entry),
        ..archived
    };
    let rows = match_rows(&entry, &m, id, Some(legacy_id(team, &m)))?;
    let mut staging = db.read(|local| {
        staged(|tx| {
            copy_team(local, tx, team)?;
            insert_match(tx, &rows)
        })
    })?;
    merge_staged(db, &mut staging, Kind::Match, id, language)
}
