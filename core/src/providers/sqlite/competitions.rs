//! The competitions (D12): shared by the teams that play them, chosen or
//! created in the form of a match, edited and removed in their list.

use crate::{
    domain::{Competition, sql::Sql},
    errors::AppError,
    providers::sqlite::{Database, not_found, sqlite},
};
use rusqlite::{OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

const COLUMNS: &str = "id, name, format, season, gender, country, friendly, deleted";

fn competition(row: &Row) -> rusqlite::Result<Competition> {
    Ok(Competition {
        id: row.get::<_, Sql<Uuid>>(0)?.0,
        name: row.get(1)?,
        format: row.get(2)?,
        season: row.get(3)?,
        gender: row.get(4)?,
        country: row.get(5)?,
        friendly: row.get(6)?,
        deleted: row.get(7)?,
    })
}

/// The competition, removed or not.
pub(crate) fn competition_row(tx: &Transaction, id: Uuid) -> Result<Option<Competition>, AppError> {
    tx.query_row(
        &format!("SELECT {COLUMNS} FROM competitions WHERE id = ?1"),
        [Sql(id)],
        competition,
    )
    .optional()
    .map_err(sqlite)
}

/// The competitions not removed, by name, then the most recent season.
pub(crate) fn competition_rows(tx: &Transaction) -> Result<Vec<Competition>, AppError> {
    tx.prepare(&format!(
        "SELECT {COLUMNS} FROM competitions WHERE deleted = 0
         ORDER BY name COLLATE NOCASE, season DESC, id"
    ))
    .and_then(|mut statement| {
        statement
            .query_map([], competition)?
            .collect::<Result<_, _>>()
    })
    .map_err(sqlite)
}

pub(crate) fn insert_competition(tx: &Transaction, c: &Competition) -> Result<(), AppError> {
    tx.execute(
        &format!("INSERT INTO competitions ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
        params![
            Sql(c.id),
            c.name,
            c.format,
            c.season,
            c.gender,
            c.country,
            c.friendly,
            c.deleted
        ],
    )
    .map(|_| ())
    .map_err(sqlite)
}

pub struct SqliteCompetitions(Database);

impl SqliteCompetitions {
    pub fn new(db: Database) -> Self {
        Self(db)
    }

    /// The competitions not removed, by name, then the most recent season.
    pub fn read_all(&self) -> Result<Vec<Competition>, AppError> {
        self.0.read(competition_rows)
    }

    /// The competition, removed or not.
    pub fn read(&self, id: Uuid) -> Result<Option<Competition>, AppError> {
        self.0.read(|tx| competition_row(tx, id))
    }

    /// The competitions not removed, as `read_all`, each with how many
    /// matches not removed are played in it.
    pub fn read_counted(&self) -> Result<Vec<(Competition, u32)>, AppError> {
        self.0.read(|tx| {
            let counts: std::collections::HashMap<Uuid, u32> = tx
                .prepare(
                    "SELECT competition_id, COUNT(*) FROM matches
                     WHERE deleted = 0 AND competition_id IS NOT NULL
                     GROUP BY competition_id",
                )
                .and_then(|mut statement| {
                    statement
                        .query_map([], |r| Ok((r.get::<_, Sql<Uuid>>(0)?.0, r.get(1)?)))?
                        .collect::<Result<_, _>>()
                })
                .map_err(sqlite)?;
            Ok(competition_rows(tx)?
                .into_iter()
                .map(|c| {
                    let n = counts.get(&c.id).copied().unwrap_or(0);
                    (c, n)
                })
                .collect())
        })
    }

    /// Writes the competition as edited (its form validated): its matches
    /// keep their own `friendly`. A competition removed or not there is
    /// not written.
    pub fn update(&self, c: &Competition) -> Result<(), AppError> {
        self.0.write(|tx| {
            let changed = tx
                .execute(
                    "UPDATE competitions
                     SET name = ?2, format = ?3, season = ?4, gender = ?5, country = ?6,
                         friendly = ?7
                     WHERE id = ?1 AND deleted = 0",
                    params![
                        Sql(c.id),
                        c.name,
                        c.format,
                        c.season,
                        c.gender,
                        c.country,
                        c.friendly
                    ],
                )
                .map_err(sqlite)?;
            match changed {
                0 => Err(not_found(&format!("competition {}", c.id))),
                _ => Ok(()),
            }
        })
    }

    /// Removes the competition (only its flag: D7); its matches stay,
    /// without a competition, their stage and round as they were.
    pub fn remove(&self, id: Uuid) -> Result<(), AppError> {
        self.0.write(|tx| {
            let changed = tx
                .execute(
                    "UPDATE competitions SET deleted = 1 WHERE id = ?1 AND deleted = 0",
                    [Sql(id)],
                )
                .map_err(sqlite)?;
            match changed {
                0 => Err(not_found(&format!("competition {id}"))),
                _ => tx
                    .execute(
                        "UPDATE matches SET competition_id = NULL WHERE competition_id = ?1",
                        [Sql(id)],
                    )
                    .map(|_| ())
                    .map_err(sqlite),
            }
        })
    }
}
