//! The competitions (D12): shared by the teams that play them, chosen or
//! created in the form of a match.

use crate::{
    domain::{Competition, sql::Sql},
    errors::AppError,
    providers::sqlite::{Database, sqlite},
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
}
