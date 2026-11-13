//! The archives of the database (TODO/001, D10): a match on its own, a
//! team, a team with all its matches. A zip of readable JSON: a
//! `manifest.json` and one file per table (`persons.json`, `teams.json`…),
//! an array of rows by the names of the columns of the schema, the words
//! of the database for the enumerations. An archive without a manifest is
//! one of the versions before the database (`legacy::archive`), imported
//! through the same merge (`legacy_archive`).
//!
//! What's in an archive:
//! - a **match** is self-contained: its row, its rosters, its sets with
//!   their lineups and events, its teams (A, and B when it's a team) with
//!   their members, the persons they and the match tell of, its
//!   competition; imported where its teams aren't, they're created;
//! - a **team**: its row, its members, their persons; with its matches,
//!   those it plays as A or as B, each as above;
//! - what's removed isn't (teams, matches, sets, events), but a player
//!   removed from a team that a match of the archive tells of; a person
//!   removed from every team of the archive shows only the name (the
//!   person keeps the rest for the other teams, as `player_form::removed`);
//! - not the video (paths of a computer), the upload queue (of a device),
//!   the cache of the lists (derived).
//!
//! Importing, all in one transaction (D10):
//! - the rows are first loaded in a database in memory, with the schema
//!   of the archive (the manifest says which): its constraints check them,
//!   its migrations bring them to this version; an archive of a newer
//!   format or schema isn't read (`IOError::DatabaseNewer`);
//! - a row already here with the same data is reused; with other data
//!   **the local one wins**, counted in `Imported`. A person of the archive
//!   agrees with the local one when it only lacks some of its details;
//! - a team, person or competition removed here comes back as the archive
//!   has it (importing is asked for); a player removed from a team here
//!   stays removed (it's a fact of the team, not a deletion);
//! - the members of a team here go after its players, in their order;
//! - a match here (by id, or by where it was before the database) is
//!   never overwritten: the archive of a match is refused, the matches of
//!   a team archive are counted and left. A match removed here comes back
//!   if the archive has it as it was;
//! - each match is read and replayed with the rows of the archive first:
//!   one that doesn't replay with this version refuses the whole archive,
//!   a value it doesn't know makes it not valid. What wins here (the
//!   details of a team, of a person) doesn't change how a match replays:
//!   its numbers are those of its roster, the roles its lineups' places.

use crate::{
    archive::zip_files,
    db::{self, MIGRATIONS},
    domain::sql::Sql,
    errors::{AppError, IOError},
    legacy::migrate::{same, unzip},
    localization::{Labels, Language},
    providers::sqlite::{
        Database, matches::replayed, not_found, rows::match_rows, sqlite, summary, teams::read_team,
    },
};
use chrono::{DateTime, Utc};
use rusqlite::{
    Connection, OptionalExtension, Transaction, params_from_iter,
    types::{Value, ValueRef},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};
use uuid::Uuid;
use zip::CompressionMethod::Deflated;

/// The version of the layout of the archives (the files and the manifest);
/// the rows follow the schema, whose version is in the manifest too.
pub const FORMAT: u32 = 1;

pub const MANIFEST: &str = "manifest.json";

/// What an archive is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Team,
    Match,
}

/// What an archive tells of itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    /// the version of the schema of its rows
    pub schema: i64,
    pub kind: Kind,
    /// the team or the match
    pub id: Uuid,
    /// the version of the app that exported it
    pub app_version: String,
    pub exported_at: DateTime<Utc>,
}

/// What to export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    Match(Uuid),
    Team(Uuid),
    /// the team and the matches it plays, as A or as B
    TeamWithMatches(Uuid),
}

/// What an import did with the rows of a kind already here or not.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Merged {
    /// not here (or removed here): added
    pub added: usize,
    /// here, the same
    pub same: usize,
    /// here with other data: the local ones kept
    pub kept_local: usize,
}

/// What an import did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub kind: Kind,
    pub id: Uuid,
    pub persons: Merged,
    pub teams: Merged,
    /// memberships of the persons in the teams
    pub members: Merged,
    pub competitions: Merged,
    /// matches added (a removed one brought back too)
    pub matches: usize,
    /// matches already here, left as they are
    pub matches_here: usize,
}

/// A row of a table, by the names of its columns.
type Row = Map<String, Json>;

/// The tables of an archive, in the order they're written (the foreign
/// keys), each with the order of its rows.
const TABLES: [(&str, &str); 9] = [
    ("persons", "id"),
    ("teams", "id"),
    ("team_members", "team_id, position"),
    ("competitions", "id"),
    ("matches", "start, id"),
    ("match_roster", "match_id, team_id, number"),
    ("sets", "match_id, number"),
    ("set_lineups", "match_id, set_number, side"),
    ("events", "match_id, set_number, seq"),
];

/// The tables of the rows of a match.
const MATCH_TABLES: [&str; 4] = ["match_roster", "sets", "set_lineups", "events"];

// the ids exported, as JSON arrays: ?1 the matches, ?2 the teams, ?3 the
// persons the matches tell of
const MATCHES: &str = "(SELECT value FROM json_each(?1))";
const TEAMS: &str = "(SELECT value FROM json_each(?2))";
const CITED: &str = "(SELECT value FROM json_each(?3))";

fn invalid(labels: &Labels, why: impl std::fmt::Display) -> AppError {
    AppError::IO(IOError::Msg(format!("{}: {why}", labels.invalid_archive)))
}

/// The archive of `subject`, as zip bytes.
pub fn export(db: &Database, subject: Subject) -> Result<Vec<u8>, AppError> {
    let (kind, id) = match subject {
        Subject::Match(id) => (Kind::Match, id),
        Subject::Team(id) | Subject::TeamWithMatches(id) => (Kind::Team, id),
    };
    let tables = db.read(|tx| {
        let ids = selection(tx, subject)?;
        TABLES
            .iter()
            .map(|(table, order)| {
                dump(tx, table, filter(table), order, &ids).map(|rows| (*table, rows))
            })
            .collect::<Result<Vec<_>, _>>()
    })?;
    let manifest = Manifest {
        format: FORMAT,
        schema: db::schema_version(),
        kind,
        id,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: Utc::now(),
    };
    let mut files = vec![(MANIFEST.to_string(), pretty(&manifest)?)];
    for (table, rows) in &tables {
        files.push((format!("{table}.json"), pretty(rows)?));
    }
    zip_files(&files, Deflated)
}

fn pretty<T: Serialize>(value: &T) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec_pretty(value).map_err(|e| AppError::IO(IOError::from(e)))
}

/// The ids the rows are chosen by (see the constants above): the matches,
/// the teams, the persons the matches tell of. The subject must be here.
fn selection(tx: &Transaction, subject: Subject) -> Result<[String; 3], AppError> {
    let exists = |sql: &str, id: Uuid| -> Result<bool, AppError> {
        tx.query_row(sql, [Sql(id)], |row| row.get(0))
            .map_err(sqlite)
    };
    let team_here = "SELECT EXISTS (SELECT 1 FROM teams WHERE id = ?1 AND deleted = 0)";
    let match_here = "SELECT EXISTS (SELECT 1 FROM matches WHERE id = ?1 AND deleted = 0)";
    let (matches, team) = match subject {
        Subject::Match(id) => match exists(match_here, id)? {
            true => Ok((ids_json(&[id]), None)),
            false => Err(not_found(&format!("match {id}"))),
        },
        Subject::Team(id) | Subject::TeamWithMatches(id) => {
            let matches = match subject {
                Subject::TeamWithMatches(_) => ids(
                    tx,
                    "SELECT id FROM matches WHERE deleted = 0 AND (team_a = ?1 OR team_b = ?1)",
                    &[id.to_string()],
                )?,
                _ => vec![],
            };
            match exists(team_here, id)? {
                true => Ok((json_of_ids(&matches), Some(id))),
                false => Err(not_found(&format!("team {id}"))),
            }
        }
    }?;
    let teams = ids(
        tx,
        &format!(
            "SELECT value FROM json_each(?2)
             UNION SELECT team_a FROM matches WHERE id IN {MATCHES}
             UNION SELECT team_b FROM matches WHERE id IN {MATCHES} AND team_b IS NOT NULL"
        ),
        &[
            matches.clone(),
            ids_json(&team.into_iter().collect::<Vec<_>>()),
        ],
    )?;
    let cited = ids(
        tx,
        &format!(
            "SELECT person_id FROM match_roster WHERE match_id IN {MATCHES}
             UNION SELECT j.value FROM set_lineups l
                 JOIN sets s ON s.match_id = l.match_id AND s.number = l.set_number,
                 json_each(json_array(l.p1, l.p2, l.p3, l.p4, l.p5, l.p6, l.libero,
                                      l.fallback_libero, l.setter)) j
                 WHERE l.match_id IN {MATCHES} AND s.deleted = 0 AND j.value IS NOT NULL
             UNION SELECT j.value FROM events e,
                 json_each(json_array(e.player, e.target_player)) j
                 WHERE e.match_id IN {MATCHES} AND e.deleted = 0 AND j.value IS NOT NULL
                 AND EXISTS (SELECT 1 FROM sets s WHERE s.match_id = e.match_id
                             AND s.number = e.set_number AND s.deleted = 0)"
        ),
        std::slice::from_ref(&matches),
    )?;
    Ok([matches, json_of_ids(&teams), json_of_ids(&cited)])
}

fn ids_json(ids: &[Uuid]) -> String {
    json_of_ids(&ids.iter().map(Uuid::to_string).collect::<Vec<_>>())
}

fn json_of_ids(ids: &[String]) -> String {
    Json::from(ids.to_vec()).to_string()
}

/// The texts of the first column.
fn ids(tx: &Connection, sql: &str, params: &[String]) -> Result<Vec<String>, AppError> {
    tx.prepare(sql)
        .and_then(|mut statement| {
            let count = statement.parameter_count();
            statement
                .query_map(params_from_iter(params.iter().take(count)), |row| {
                    row.get(0)
                })?
                .collect::<Result<_, _>>()
        })
        .map_err(sqlite)
}

/// Which rows of the table are exported (see `selection`).
fn filter(table: &str) -> String {
    let set_kept = |alias: &str| {
        format!(
            "EXISTS (SELECT 1 FROM sets s WHERE s.match_id = {alias}.match_id
                     AND s.number = {alias}.set_number AND s.deleted = 0)"
        )
    };
    let members = format!("team_id IN {TEAMS} AND (deleted = 0 OR person_id IN {CITED})");
    match table {
        "persons" => {
            format!("id IN {CITED} OR id IN (SELECT person_id FROM team_members WHERE {members})")
        }
        "teams" => format!("id IN {TEAMS}"),
        "team_members" => members,
        "competitions" => {
            format!("id IN (SELECT competition_id FROM matches WHERE id IN {MATCHES})")
        }
        "matches" | "match_roster" => format!("{} IN {MATCHES}", key_of_match(table)),
        "sets" => format!("match_id IN {MATCHES} AND deleted = 0"),
        "set_lineups" => format!("match_id IN {MATCHES} AND {}", set_kept("set_lineups")),
        "events" => format!(
            "match_id IN {MATCHES} AND deleted = 0 AND {}",
            set_kept("events")
        ),
        _ => "0".to_string(),
    }
}

fn key_of_match(table: &str) -> &'static str {
    match table {
        "matches" => "id",
        _ => "match_id",
    }
}

/// The rows of the table chosen by `filter`, in `order`; a person removed
/// from every team exported shows only the name.
fn dump(
    tx: &Connection,
    table: &str,
    filter: String,
    order: &str,
    params: &[String; 3],
) -> Result<Vec<Row>, AppError> {
    let rows = select(
        tx,
        &format!("SELECT * FROM {table} WHERE {filter} ORDER BY {order}"),
        params,
    )?;
    match table {
        "persons" => {
            let active = ids(
                tx,
                &format!(
                    "SELECT person_id FROM team_members WHERE team_id IN {TEAMS} AND deleted = 0"
                ),
                params,
            )?;
            Ok(rows
                .into_iter()
                .map(|row| {
                    match active
                        .iter()
                        .any(|id| Some(id.as_str()) == text(&row, "id"))
                    {
                        true => row,
                        false => only_name(row),
                    }
                })
                .collect())
        }
        _ => Ok(rows),
    }
}

fn only_name(mut row: Row) -> Row {
    row.insert("birth_date".into(), Json::Null);
    row.insert("height_cm".into(), Json::Null);
    row.insert("notes".into(), Json::from(""));
    row
}

fn text<'a>(row: &'a Row, column: &str) -> Option<&'a str> {
    row.get(column).and_then(Json::as_str)
}

/// The rows of a query, by the names of their columns.
fn select(tx: &Connection, sql: &str, params: &[String]) -> Result<Vec<Row>, AppError> {
    let mut statement = tx.prepare(sql).map_err(sqlite)?;
    let names: Vec<String> = statement.column_names().map(String::from).collect();
    let count = statement.parameter_count();
    statement
        .query_map(params_from_iter(params.iter().take(count)), |row| {
            names
                .iter()
                .enumerate()
                .map(|(i, name)| Ok((name.clone(), json(row.get_ref(i)?)?)))
                .collect::<rusqlite::Result<Row>>()
        })
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        .map_err(sqlite)
}

fn json(value: ValueRef) -> rusqlite::Result<Json> {
    match value {
        ValueRef::Null => Ok(Json::Null),
        ValueRef::Integer(i) => Ok(Json::from(i)),
        ValueRef::Real(f) => Ok(Json::from(f)),
        ValueRef::Text(_) => value.as_str().map(Json::from).map_err(Into::into),
        ValueRef::Blob(_) => Err(rusqlite::Error::InvalidColumnType(
            0,
            "blob".into(),
            rusqlite::types::Type::Blob,
        )),
    }
}

/// The value of a column for the database: no booleans, arrays, objects.
fn value(column: &str, json: &Json) -> Result<Value, String> {
    match json {
        Json::Null => Ok(Value::Null),
        Json::String(s) => Ok(Value::Text(s.clone())),
        Json::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => Ok(Value::Integer(i)),
            (None, Some(f)) => Ok(Value::Real(f)),
            (None, None) => Err(format!("{column}: {n}")),
        },
        other => Err(format!("{column}: {other}")),
    }
}

/// The names of the columns of the table.
fn columns(tx: &Connection, table: &str) -> Result<Vec<String>, AppError> {
    tx.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()
        })
        .map_err(sqlite)
}

/// The row added to the table: only its known columns.
fn insert(tx: &Connection, table: &str, known: &[String], row: &Row) -> Result<(), String> {
    let unknown = row.keys().find(|column| !known.contains(column));
    match unknown {
        Some(column) => Err(format!("{table}: column {column:?}")),
        None => {
            let names: Vec<String> = row.keys().map(|c| format!("\"{c}\"")).collect();
            let marks: Vec<String> = (1..=row.len()).map(|i| format!("?{i}")).collect();
            let values = row
                .iter()
                .map(|(column, json)| value(column, json))
                .collect::<Result<Vec<_>, _>>()?;
            tx.execute(
                &format!(
                    "INSERT INTO {table} ({}) VALUES ({})",
                    names.join(", "),
                    marks.join(", ")
                ),
                params_from_iter(values),
            )
            .map(|_| ())
            .map_err(|e| format!("{table}: {e}"))
        }
    }
}

/// The manifest of an archive of the database, none for the archives of
/// the versions before it.
pub fn manifest(archive: &[u8]) -> Option<Manifest> {
    unzip(archive)
        .ok()?
        .into_iter()
        .find(|(name, _)| name == MANIFEST)
        .and_then(|(_, data)| serde_json::from_slice(&data).ok())
}

/// Adds what the archive holds that isn't here: see the module.
pub fn import(db: &Database, archive: &[u8], language: &Language) -> Result<Imported, AppError> {
    let labels = language.labels();
    let files = unzip(archive).map_err(|e| invalid(labels, e))?;
    let file = |name: &str| {
        files
            .iter()
            .find(|(file, _)| file == name)
            .map(|(_, data)| data)
    };
    let manifest: Manifest = file(MANIFEST)
        .ok_or_else(|| invalid(labels, MANIFEST))
        .and_then(|data| serde_json::from_slice(data).map_err(|e| invalid(labels, e)))?;
    let known = db::schema_version();
    let newer = |found: i64| AppError::IO(IOError::DatabaseNewer { found, known });
    let schema = match (manifest.format, manifest.schema) {
        (format, _) if format > FORMAT => Err(newer(i64::from(format))),
        (_, schema) if schema > known => Err(newer(schema)),
        (FORMAT, schema) if schema >= 1 => usize::try_from(schema).map_err(|e| invalid(labels, e)),
        (format, schema) => Err(invalid(labels, format!("format {format}, schema {schema}"))),
    }?;
    let mut staging = db::open_in_memory_with(MIGRATIONS.get(..schema).unwrap_or(MIGRATIONS))?;
    load(&staging, &files, labels)?;
    db::migrate(&mut staging, MIGRATIONS)?;
    merge_staged(db, &mut staging, manifest.kind, manifest.id, language)
}

/// Adds what `staging` holds that isn't here, as an archive of `kind` of
/// `subject` (see the module): `staging` has the schema of today, with
/// the rows of an archive, of this version or of one before the database
/// (`legacy_archive`).
pub(crate) fn merge_staged(
    db: &Database,
    staging: &mut Connection,
    kind: Kind,
    subject: Uuid,
    language: &Language,
) -> Result<Imported, AppError> {
    let labels = language.labels();
    let staged = staging.transaction().map_err(sqlite)?;
    let tables: Vec<(&str, Vec<Row>)> = TABLES
        .iter()
        .map(|(table, order)| {
            select(
                &staged,
                &format!("SELECT * FROM {table} ORDER BY {order}"),
                &[],
            )
            .map(|rows| (*table, rows))
        })
        .collect::<Result<_, _>>()?;
    let of = |name: &str| {
        tables
            .iter()
            .find(|(table, _)| *table == name)
            .map_or(&[][..], |(_, rows)| rows.as_slice())
    };
    // each match reads and replays with the rows of the archive
    of("matches")
        .iter()
        .try_for_each(|m| staged_replays(&staged, m, language))?;
    let id = subject;
    let subject = id.to_string();
    let there = match kind {
        Kind::Team => of("teams")
            .iter()
            .any(|t| text(t, "id") == Some(&subject) && t.get("deleted") == Some(&Json::from(0))),
        Kind::Match => of("matches")
            .iter()
            .any(|m| text(m, "id") == Some(&subject)),
    };
    if !there {
        return Err(invalid(labels, format!("{kind:?} {subject}")));
    }
    db.write(|tx| {
        let persons = merge(tx, "persons", of("persons"), agrees_person)?;
        let teams = merge(tx, "teams", of("teams"), |local, row| local == row)?;
        let members = merge_members(tx, of("team_members"))?;
        let competitions = merge(tx, "competitions", of("competitions"), |l, r| l == r)?;
        let mut imported = Imported {
            kind,
            id,
            persons,
            teams,
            members,
            competitions,
            matches: 0,
            matches_here: 0,
        };
        for m in of("matches") {
            let added = merge_match(tx, &staged, m, &of)?;
            match added {
                true => imported.matches += 1,
                false => imported.matches_here += 1,
            }
        }
        match (kind, imported.matches_here) {
            (Kind::Match, 1..) => Err(AppError::IO(IOError::Msg(
                labels.match_already_exists.to_string(),
            ))),
            _ => Ok(imported),
        }
    })
}

/// The rows of team `team` here (its row, all its members, their persons)
/// copied as they are into `staging`: what's merged back is the same.
pub(crate) fn copy_team(
    local: &Connection,
    staging: &Connection,
    team: Uuid,
) -> Result<(), AppError> {
    let id = [team.to_string()];
    [
        (
            "persons",
            "id IN (SELECT person_id FROM team_members WHERE team_id = ?1)",
        ),
        ("teams", "id = ?1"),
        ("team_members", "team_id = ?1"),
    ]
    .iter()
    .try_for_each(|(table, filter)| {
        let known = columns(staging, table)?;
        select(local, &format!("SELECT * FROM {table} WHERE {filter}"), &id)?
            .iter()
            .try_for_each(|row| insert(staging, table, &known, row))
            .map_err(|e| AppError::IO(IOError::Msg(e)))
    })
}

/// The match of the archive read and replayed with its rows: a value
/// this version doesn't know makes the archive not valid.
fn staged_replays(staged: &Transaction, m: &Row, language: &Language) -> Result<(), AppError> {
    let labels = language.labels();
    let (id, team) = (
        text(m, "id").unwrap_or_default(),
        text(m, "team_a").unwrap_or_default(),
    );
    let team = Uuid::parse_str(team).map_err(|e| invalid(labels, e))?;
    match read_team(staged, &team).and_then(|team| replayed(staged, &team, id, language)) {
        Ok(_) => Ok(()),
        Err(AppError::Match(e)) => Err(AppError::Match(e)),
        Err(e) => Err(invalid(labels, e)),
    }
}

/// The rows of the archive in the database of its schema: its constraints
/// check them.
fn load(
    staging: &Connection,
    files: &[(String, Vec<u8>)],
    labels: &Labels,
) -> Result<(), AppError> {
    TABLES.iter().try_for_each(|(table, _)| {
        let name = format!("{table}.json");
        let rows: Vec<Row> = match files.iter().find(|(file, _)| *file == name) {
            Some((_, data)) => serde_json::from_slice(data).map_err(|e| invalid(labels, e)),
            None => Ok(vec![]),
        }?;
        let known = columns(staging, table)?;
        rows.iter()
            .try_for_each(|row| insert(staging, table, &known, row))
            .map_err(|e| invalid(labels, e))
    })
}

/// The values of the key of a row of the table.
fn key(table: &str) -> &'static [&'static str] {
    match table {
        "team_members" => &["team_id", "person_id"],
        _ => &["id"],
    }
}

/// The row here with the key of `row`, removed or not.
fn local(tx: &Connection, table: &str, row: &Row) -> Result<Option<Row>, AppError> {
    let key = key(table);
    let clause: Vec<String> = key
        .iter()
        .zip(1..)
        .map(|(column, i)| format!("{column} = ?{i}"))
        .collect();
    let values: Vec<String> = key
        .iter()
        .map(|column| text(row, column).unwrap_or_default().to_string())
        .collect();
    select(
        tx,
        &format!("SELECT * FROM {table} WHERE {}", clause.join(" AND ")),
        &values,
    )
    .map(|rows| rows.into_iter().next())
}

fn deleted(row: &Row) -> bool {
    row.get("deleted") == Some(&Json::from(1))
}

/// A person of the archive agrees with the one here when it's the same but
/// for the details it lacks (exported with only the name).
fn agrees_person(local: &Row, row: &Row) -> bool {
    row.iter().all(|(column, value)| {
        let lacking = matches!(
            (column.as_str(), value),
            ("birth_date" | "height_cm", Json::Null) | ("notes", Json::String(_))
        ) && value.as_str().is_none_or(str::is_empty);
        lacking || local.get(column) == Some(value)
    })
}

/// The rows of a table with a key of their own (persons, teams,
/// competitions) here: added when missing or removed here, otherwise the
/// local ones win.
fn merge(
    tx: &Connection,
    table: &str,
    rows: &[Row],
    same: fn(&Row, &Row) -> bool,
) -> Result<Merged, AppError> {
    let known = columns(tx, table)?;
    rows.iter().try_fold(Merged::default(), |merged, row| {
        let here = local(tx, table, row)?;
        let written = match &here {
            None => insert(tx, table, &known, row).map(|()| true),
            Some(l) if deleted(l) && !deleted(row) => update(tx, table, row).map(|()| true),
            Some(_) => Ok(false),
        }
        .map_err(|e| AppError::IO(IOError::Msg(e)))?;
        Ok(match (written, here) {
            (true, _) => Merged {
                added: merged.added + 1,
                ..merged
            },
            (false, Some(l)) if same(&l, row) => Merged {
                same: merged.same + 1,
                ..merged
            },
            (false, _) => Merged {
                kept_local: merged.kept_local + 1,
                ..merged
            },
        })
    })
}

/// The row here as the archive has it.
fn update(tx: &Connection, table: &str, row: &Row) -> Result<(), String> {
    let key = key(table);
    let values = row
        .iter()
        .map(|(column, json)| value(column, json))
        .collect::<Result<Vec<_>, _>>()?;
    let sets: Vec<String> = row
        .keys()
        .zip(1..)
        .map(|(column, i)| format!("\"{column}\" = ?{i}"))
        .collect();
    let keys: Vec<String> = key
        .iter()
        .map(|column| {
            row.keys()
                .zip(1..)
                .find(|(c, _)| c == column)
                .map_or_else(|| "0".to_string(), |(_, i)| format!("{column} = ?{i}"))
        })
        .collect();
    tx.execute(
        &format!(
            "UPDATE {table} SET {} WHERE {}",
            sets.join(", "),
            keys.join(" AND ")
        ),
        params_from_iter(values),
    )
    .map(|_| ())
    .map_err(|e| format!("{table}: {e}"))
}

/// The members of the teams: the missing ones after the players of their
/// team, in their order; the others as they are here.
fn merge_members(tx: &Connection, rows: &[Row]) -> Result<Merged, AppError> {
    let known = columns(tx, "team_members")?;
    rows.iter().try_fold(Merged::default(), |merged, row| {
        let fields = |r: &Row| ["number", "role", "deleted"].map(|c| r.get(c).cloned());
        match local(tx, "team_members", row)? {
            Some(l) if fields(&l) == fields(row) => Ok(Merged {
                same: merged.same + 1,
                ..merged
            }),
            Some(_) => Ok(Merged {
                kept_local: merged.kept_local + 1,
                ..merged
            }),
            None => {
                let position: i64 = tx
                    .query_row(
                        "SELECT coalesce(max(position) + 1, 0) FROM team_members
                         WHERE team_id = ?1",
                        [text(row, "team_id").unwrap_or_default()],
                        |r| r.get(0),
                    )
                    .map_err(sqlite)?;
                let mut row = row.clone();
                row.insert("position".into(), Json::from(position));
                insert(tx, "team_members", &known, &row)
                    .map_err(|e| AppError::IO(IOError::Msg(e)))?;
                Ok(Merged {
                    added: merged.added + 1,
                    ..merged
                })
            }
        }
    })
}

/// The match added with its rows, unless it's here (by id, or by where it
/// was before the database): whether it's added. One removed here comes
/// back when the archive has it as it was (but the ids of the events: an
/// archive of a version before gives them new). Its row in the cache of the
/// lists is written.
fn merge_match<'a>(
    tx: &Transaction,
    staged: &Transaction,
    m: &Row,
    of: &dyn Fn(&str) -> &'a [Row],
) -> Result<bool, AppError> {
    let id = text(m, "id").unwrap_or_default();
    let here: Option<(String, bool)> = tx
        .query_row(
            "SELECT id, deleted FROM matches WHERE id = ?1
             OR (legacy_team = ?2 AND legacy_id = ?3) ORDER BY id = ?1 DESC",
            [Some(id), text(m, "legacy_team"), text(m, "legacy_id")],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sqlite)?;
    let uuid = Uuid::parse_str(id).map_err(|e| AppError::IO(IOError::Msg(e.to_string())))?;
    match here {
        None => {
            let message = |e: String| AppError::IO(IOError::Msg(e));
            let known = columns(tx, "matches")?;
            insert(tx, "matches", &known, m).map_err(message)?;
            for table in MATCH_TABLES {
                let known = columns(tx, table)?;
                of(table)
                    .iter()
                    .filter(|row| text(row, "match_id") == Some(id))
                    .try_for_each(|row| insert(tx, table, &known, row))
                    .map_err(message)?;
            }
            summary::refresh(tx, uuid).map(|()| true)
        }
        Some((here, true)) if here == id => {
            tx.execute("UPDATE matches SET deleted = 0 WHERE id = ?1", [id])
                .map_err(sqlite)?;
            let agree = match (match_rows(tx, uuid)?, match_rows(staged, uuid)?) {
                (Some(here), Some(archived)) => same(&archived, &here)?,
                _ => false,
            };
            match agree {
                true => summary::refresh(tx, uuid).map(|()| true),
                false => tx
                    .execute("UPDATE matches SET deleted = 1 WHERE id = ?1", [id])
                    .map(|_| false)
                    .map_err(sqlite),
            }
        }
        Some(_) => Ok(false),
    }
}
