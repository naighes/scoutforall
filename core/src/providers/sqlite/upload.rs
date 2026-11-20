//! The matches to upload, read from the database (TODO/001, D14): the
//! archive in the format of the files of today, from A's side, the same
//! the API had before the database, and, when both teams were scouted
//! (`"both"`), the sets of B in files of their own: random ids, no names.

use crate::{
    analytics::upload::{SideB, ToUpload, UploadSource, written_set_files, written_set_files_of_b},
    domain::{Home, ScoutingMode, TeamRef, sql::Sql},
    errors::AppError,
    localization::Language,
    providers::sqlite::{
        Database,
        matches::{match_uuid, replayed},
        rows::match_rows,
        sets::numbered,
        sqlite,
        teams::read_team,
    },
    shapes::{r#match::MatchEntry, team::TeamEntry},
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
            let (neutral, side_b) = side_b_of(tx, &team, match_id, &self.language)?;
            Ok(ToUpload {
                set_files: written_set_files(&entry)?,
                file_id,
                entry,
                neutral,
                side_b,
            })
        })
    }
}

/// Whether the match was played on neutral ground, and team B's sets when
/// both teams were scouted.
fn side_b_of(
    tx: &Transaction,
    team: &TeamEntry,
    match_id: &str,
    language: &Language,
) -> Result<(bool, Option<SideB>), AppError> {
    let Some(rows) = match_rows(tx, match_uuid(match_id)?)? else {
        return Ok((false, None));
    };
    let neutral = rows.row.home == Home::Neutral;
    let side_b = match (rows.row.scouting, &rows.row.team_b) {
        (ScoutingMode::Both, TeamRef::Team(b)) => {
            let team_b = read_team(tx, b)?;
            rows.entry_of_b(team, &team_b)?
                .map(|e| {
                    numbered(e.sets.clone(), language, match_id)
                        .map(|sets| MatchEntry { sets, ..e })
                })
                .transpose()?
                .map(|e| {
                    written_set_files_of_b(&e).map(|set_files| SideB {
                        team_id: team_b.id,
                        set_files,
                    })
                })
                .transpose()?
        }
        _ => None,
    };
    Ok((neutral, side_b))
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
