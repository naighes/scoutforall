//! The matches to upload, read from the database (TODO/001, D14): the
//! archive in the format of the files of today, from A's side, the same
//! the API had before the database. The events of B (`"both"`) don't go.

use crate::{
    analytics::upload::{ToUpload, UploadSource, written_set_files},
    domain::sql::Sql,
    errors::AppError,
    localization::Language,
    providers::sqlite::{
        Database,
        matches::{match_uuid, replayed},
        sqlite,
        teams::read_team,
    },
};
use async_trait::async_trait;
use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

pub struct SqliteUploads {
    db: Database,
    language: Language,
}

impl SqliteUploads {
    pub fn new(db: Database, language: Language) -> Self {
        Self { db, language }
    }
}

#[async_trait]
impl UploadSource for SqliteUploads {
    /// A match removed, or of another team A, can't be read.
    async fn read(&self, team_id: &Uuid, match_id: &str) -> Result<ToUpload, AppError> {
        self.db.read(|tx| {
            let team = read_team(tx, team_id)?;
            let (entry, _) = replayed(tx, &team, match_id, &self.language)?;
            let file_id = legacy_id(tx, match_uuid(match_id)?, team_id)?
                .unwrap_or_else(|| match_id.to_string());
            Ok(ToUpload {
                set_files: written_set_files(&entry)?,
                file_id,
                entry,
            })
        })
    }
}

/// Where the match was before the database, when it was in a folder of
/// its team A: its file keeps the name it had.
fn legacy_id(tx: &Transaction, id: Uuid, team_a: &Uuid) -> Result<Option<String>, AppError> {
    tx.query_row(
        "SELECT legacy_id FROM matches
         WHERE id = ?1 AND legacy_team = ?2 AND legacy_id IS NOT NULL",
        params![Sql(id), Sql(*team_a)],
        |row| row.get(0),
    )
    .optional()
    .map_err(sqlite)
}
