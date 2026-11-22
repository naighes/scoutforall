//! The archive of the training of a team (TODO/021 §7), apart from the
//! archives of teams and matches: the same format (`archive`: a manifest
//! and a JSON per table), `contents: "training"` in the manifest.
//!
//! What's in it: the training rows of the team not removed (programs,
//! mesocycles, microcycles, sessions, their activities and attendance), the
//! row of the team, the persons of the attendance (the players of the team
//! as they are, the guests with only the name). Not the roster, the
//! matches, the video. The notes on the players are in it.
//!
//! Importing (all in one transaction, `Imported::training`):
//! - the team must be here already, else the import is refused
//!   (`import_team_first`); its row, its roster are never touched;
//! - the persons are merged as in the other archives (a guest not here is
//!   made, without a team);
//! - a row here with the same data is reused, with other data **the local
//!   one wins** (counted); a row removed here stays removed (counted too);
//! - what's not here is added, unless it overlaps what's here (cycles of
//!   the same level, sessions between them and with the matches in
//!   programma): it isn't imported and is counted; a session that fits
//!   but whose microcycle isn't here, or doesn't hold its day, comes in
//!   free, as a cycle does without its parent;
//! - activities and attendance go with their session, when it's added.

use crate::{
    errors::{AppError, IOError, TrainingError},
    localization::Language,
    providers::sqlite::{
        Database,
        archive::{
            CITED, Imported, Kind, Merged, Preview, Row, TEAMS, TrainingMerged, TrainingPreview,
            agrees_person, columns, deleted, dump, ids, insert, invalid, json_of_ids, local,
            merge as merge_rows, select, text,
        },
        not_found, sqlite,
        teams::read_team,
        training::{check_session, read_session},
        training_cycles::{check_cycle, check_microcycle, read_cycle},
    },
    training::cycles::Level,
};
use rusqlite::{Connection, Transaction};
use serde_json::Value as Json;
use uuid::Uuid;

/// The tables of the training in an archive, in the order they're written
/// (the foreign keys), each with the order of its rows.
pub(crate) const OWN: [(&str, &str); 6] = [
    ("training_programs", "start_date, id"),
    ("mesocycles", "start_date, id"),
    ("microcycles", "start_date, id"),
    ("training_sessions", "start_local, id"),
    ("training_activities", "session_id, position"),
    ("training_attendance", "session_id, person_id"),
];

const PERSONS: (&str, &str) = ("persons", "id");
const TEAMS_TABLE: (&str, &str) = ("teams", "id");

/// The sessions of the team not removed, to chose their children by.
const SESSIONS_KEPT: &str = "(SELECT id FROM training_sessions WHERE team_id IN (SELECT value FROM json_each(?2)) AND deleted = 0)";

fn filter(table: &str) -> String {
    match table {
        "persons" => format!("id IN {CITED}"),
        "teams" => format!("id IN {TEAMS}"),
        "training_activities" | "training_attendance" => {
            format!("deleted = 0 AND session_id IN {SESSIONS_KEPT}")
        }
        _ => format!("team_id IN {TEAMS} AND deleted = 0"),
    }
}

/// The rows of the archive of the training of `team`, by table.
pub(crate) fn tables(db: &Database, team: Uuid) -> Result<Vec<(&'static str, Vec<Row>)>, AppError> {
    let tables = db.read(|tx| {
        let here: bool = tx
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM teams WHERE id = ?1 AND deleted = 0)",
                [team.to_string()],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        match here {
            true => Ok(()),
            false => Err(not_found(&format!("team {team}"))),
        }?;
        let cited = ids(
            tx,
            "SELECT DISTINCT a.person_id FROM training_attendance a
             JOIN training_sessions s ON s.id = a.session_id
             WHERE a.deleted = 0 AND s.deleted = 0 AND s.team_id = ?1",
            &[team.to_string()],
        )?;
        let params = [
            "[]".to_string(),
            json_of_ids(&[team.to_string()]),
            json_of_ids(&cited),
        ];
        [PERSONS, TEAMS_TABLE]
            .iter()
            .chain(OWN.iter())
            .map(|(table, order)| {
                dump(tx, table, filter(table), order, &params).map(|rows| (*table, rows))
            })
            .collect::<Result<Vec<_>, _>>()
    })?;
    Ok(freed(tables))
}

/// A cycle or a session whose parent isn't exported (removed) goes free:
/// the archive must load by itself.
fn freed(mut tables: Vec<(&'static str, Vec<Row>)>) -> Vec<(&'static str, Vec<Row>)> {
    let ids_of = |tables: &[(&str, Vec<Row>)], name: &str| -> Vec<String> {
        tables
            .iter()
            .filter(|(table, _)| *table == name)
            .flat_map(|(_, rows)| rows.iter())
            .filter_map(|row| text(row, "id").map(String::from))
            .collect()
    };
    for (table, column, parent) in PARENTS {
        let there = ids_of(&tables, parent);
        tables
            .iter_mut()
            .filter(|(name, _)| *name == table)
            .flat_map(|(_, rows)| rows.iter_mut())
            .for_each(|row| match text(row, column) {
                Some(id) if !there.iter().any(|t| t == id) => {
                    row.insert(column.into(), Json::Null);
                }
                _ => {}
            });
    }
    tables
}

/// The tables with a parent: the table, the column, the table of the
/// parent.
const PARENTS: [(&str, &str, &str); 3] = [
    ("mesocycles", "program_id", "training_programs"),
    ("microcycles", "mesocycle_id", "mesocycles"),
    ("training_sessions", "microcycle_id", "microcycles"),
];

/// What the archive holds, from the staging database.
pub(crate) fn preview(tx: &Transaction, team: &Uuid) -> Result<Preview, AppError> {
    let count = |tables: &[&str]| -> Result<usize, AppError> {
        tables.iter().try_fold(0, |total, table| {
            tx.query_row(
                &format!("SELECT count(*) FROM {table} WHERE deleted = 0"),
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| total + usize::try_from(n).unwrap_or_default())
            .map_err(sqlite)
        })
    };
    Ok(Preview::Training(TrainingPreview {
        team: read_team(tx, team)?,
        sessions: count(&["training_sessions"])?,
        cycles: count(&["training_programs", "mesocycles", "microcycles"])?,
    }))
}

/// What happened to a row of the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placed {
    Added,
    Same,
    /// here with other data, or removed here
    Kept,
    Overlapping,
}

fn message(e: impl std::fmt::Display) -> AppError {
    AppError::IO(IOError::Msg(e.to_string()))
}

/// The row inserted and checked, or taken back.
enum Fit {
    Added,
    Overlapping,
    /// not inside its parent: to try free
    Outside,
}

/// Inserts the row inside a savepoint and checks it (`check` reads it back
/// as it's saved): kept when it fits, else taken back.
fn attempt(
    tx: &Transaction,
    table: &str,
    known: &[String],
    row: &Row,
    check: &dyn Fn(&Transaction) -> Result<(), AppError>,
) -> Result<Fit, AppError> {
    tx.execute_batch("SAVEPOINT import_row").map_err(sqlite)?;
    let fit = insert(tx, table, known, row)
        .map_err(message)
        .and_then(|()| check(tx))
        .map(|()| Fit::Added)
        .or_else(|e| match e {
            AppError::Training(TrainingError::OutsideParent) => Ok(Fit::Outside),
            AppError::Training(TrainingError::Overlap(_) | TrainingError::CycleOverlap(_)) => {
                Ok(Fit::Overlapping)
            }
            other => Err(other),
        });
    let end = match fit {
        Ok(Fit::Added) => "RELEASE import_row",
        _ => "ROLLBACK TO import_row; RELEASE import_row",
    };
    tx.execute_batch(end).map_err(sqlite)?;
    fit
}

/// The row of the archive here: reused, kept local, or added when it fits
/// (see the module); `parent` is the column and the table of its parent.
fn place(
    tx: &Transaction,
    table: &str,
    parent: Option<(&str, &str)>,
    row: &Row,
    team: &str,
    check: &dyn Fn(&Transaction, &Uuid) -> Result<(), AppError>,
) -> Result<Placed, AppError> {
    let id = Uuid::parse_str(text(row, "id").unwrap_or_default()).map_err(message)?;
    match local(tx, table, row)? {
        Some(here) if !deleted(&here) && here == *row => return Ok(Placed::Same),
        Some(_) => return Ok(Placed::Kept),
        None if deleted(row) => return Ok(Placed::Same),
        None => {}
    }
    let known = columns(tx, table)?;
    let parent_here = |column: &str, table: &str| -> Result<bool, AppError> {
        match text(row, column) {
            None => Ok(false),
            Some(parent) => tx
                .query_row(
                    &format!(
                        "SELECT EXISTS (SELECT 1 FROM {table}
                         WHERE id = ?1 AND team_id = ?2 AND deleted = 0)"
                    ),
                    [parent, team],
                    |r| r.get(0),
                )
                .map_err(sqlite),
        }
    };
    let free = |column: &str| {
        let mut free = row.clone();
        free.insert(column.into(), Json::Null);
        free
    };
    let candidates: Vec<Row> = match parent {
        Some((column, table)) if text(row, column).is_some() => {
            let mut candidates = vec![];
            if parent_here(column, table)? {
                candidates.push(row.clone());
            }
            candidates.push(free(column));
            candidates
        }
        _ => vec![row.clone()],
    };
    for candidate in &candidates {
        match attempt(tx, table, &known, candidate, &|tx| check(tx, &id))? {
            Fit::Added => return Ok(Placed::Added),
            Fit::Overlapping => return Ok(Placed::Overlapping),
            Fit::Outside => {}
        }
    }
    Ok(Placed::Overlapping)
}

/// The rows of a table placed one after the other: how many of each, and
/// the ids added.
fn place_all(
    tx: &Transaction,
    table: &str,
    parent: Option<(&str, &str)>,
    rows: &[Row],
    team: &str,
    check: &dyn Fn(&Transaction, &Uuid) -> Result<(), AppError>,
) -> Result<(Merged, usize, Vec<String>), AppError> {
    rows.iter().try_fold(
        (Merged::default(), 0, vec![]),
        |(merged, over, mut added), row| {
            Ok(match place(tx, table, parent, row, team, check)? {
                Placed::Added => {
                    added.extend(text(row, "id").map(String::from));
                    (
                        Merged {
                            added: merged.added + 1,
                            ..merged
                        },
                        over,
                        added,
                    )
                }
                Placed::Same => (
                    Merged {
                        same: merged.same + 1,
                        ..merged
                    },
                    over,
                    added,
                ),
                Placed::Kept => (
                    Merged {
                        kept_local: merged.kept_local + 1,
                        ..merged
                    },
                    over,
                    added,
                ),
                Placed::Overlapping => (merged, over + 1, added),
            })
        },
    )
}

const LEVELS: [(Level, &str, Option<(&str, &str)>); 3] = [
    (Level::Program, "training_programs", None),
    (
        Level::Mesocycle,
        "mesocycles",
        Some(("program_id", "training_programs")),
    ),
    (
        Level::Microcycle,
        "microcycles",
        Some(("mesocycle_id", "mesocycles")),
    ),
];

/// Adds what the staging database holds of the training of `team` that
/// isn't here: see the module.
pub(crate) fn merge(
    db: &Database,
    staging: &mut Connection,
    team: Uuid,
    language: &Language,
) -> Result<Imported, AppError> {
    let labels = language.labels();
    let staged = staging.transaction().map_err(sqlite)?;
    let rows = |(table, order): (&str, &str)| {
        select(
            &staged,
            &format!("SELECT * FROM {table} ORDER BY {order}"),
            &[],
        )
    };
    let id = team.to_string();
    let teams = rows(TEAMS_TABLE)?;
    let there = teams
        .iter()
        .any(|t| text(t, "id") == Some(&id) && !deleted(t));
    let foreign = OWN
        .iter()
        .filter(|(table, _)| !matches!(*table, "training_activities" | "training_attendance"))
        .try_fold(false, |foreign, own| {
            rows(*own)
                .map(|rows| foreign || rows.iter().any(|row| text(row, "team_id") != Some(&id)))
        })?;
    match (there, foreign) {
        (true, false) => Ok(()),
        _ => Err(invalid(labels, format!("training of team {id}"))),
    }?;
    let persons = rows(PERSONS)?;
    let table = |name: &str| -> Result<Vec<Row>, AppError> {
        OWN.iter()
            .find(|(table, _)| *table == name)
            .map_or(Ok(vec![]), |own| rows(*own))
    };
    let (sessions, activities, attendance) = (
        table("training_sessions")?,
        table("training_activities")?,
        table("training_attendance")?,
    );
    let cycles = LEVELS
        .iter()
        .map(|(level, name, parent)| table(name).map(|rows| (*level, *name, *parent, rows)))
        .collect::<Result<Vec<_>, _>>()?;
    db.write(|tx| {
        let here: bool = tx
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM teams WHERE id = ?1 AND deleted = 0)",
                [&id],
                |row| row.get(0),
            )
            .map_err(sqlite)?;
        match here {
            true => Ok(()),
            false => Err(message(labels.import_team_first)),
        }?;
        let persons = merge_rows(tx, "persons", &persons, agrees_person)?;
        let mut training = TrainingMerged::default();
        for (level, name, parent, rows) in &cycles {
            let (merged, overlapping, _) =
                place_all(tx, name, *parent, rows, &id, &|tx, cycle| {
                    let c = read_cycle(tx, *level, cycle)?;
                    check_cycle(
                        tx,
                        *level,
                        &c.team_id,
                        Some(cycle),
                        &c.window,
                        c.parent_id.as_ref(),
                    )
                })?;
            training.cycles = sum(training.cycles, merged);
            training.cycles_overlapping += overlapping;
        }
        let (merged, overlapping, added) = place_all(
            tx,
            "training_sessions",
            Some(("microcycle_id", "microcycles")),
            &sessions,
            &id,
            &|tx, session| {
                let s = read_session(tx, session)?;
                check_microcycle(tx, &s.team_id, s.microcycle_id.as_ref(), s.start)?;
                check_session(tx, &s.team_id, Some(session), s.start, s.duration_min)
            },
        )?;
        training.sessions = merged;
        training.sessions_overlapping = overlapping;
        for (table, rows) in [
            ("training_activities", &activities),
            ("training_attendance", &attendance),
        ] {
            let known = columns(tx, table)?;
            rows.iter()
                .filter(|row| text(row, "session_id").is_some_and(|s| added.iter().any(|a| a == s)))
                .try_for_each(|row| insert(tx, table, &known, row).map_err(message))?;
        }
        Ok(Imported {
            kind: Kind::Team,
            id: team,
            persons,
            teams: Merged::default(),
            members: Merged::default(),
            competitions: Merged::default(),
            matches: 0,
            matches_here: 0,
            training,
        })
    })
}

fn sum(a: Merged, b: Merged) -> Merged {
    Merged {
        added: a.added + b.added,
        same: a.same + b.same,
        kept_local: a.kept_local + b.kept_local,
    }
}
