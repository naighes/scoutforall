//! The database of the data (TODO/001, D6): one SQLite file, opened with the
//! same settings everywhere, its schema brought up to date when it opens.
//!
//! The version of the schema is `PRAGMA user_version`: migration `n` (the
//! `n`-th of `MIGRATIONS`, from 1) takes it from `n - 1` to `n`, in one
//! transaction with the new version, so a migration that fails leaves the
//! database as it was. A database newer than this version of the app (a
//! newer app wrote it) isn't touched: `IOError::DatabaseNewer`.
//!
//! The migrations are only ever appended: one already released never
//! changes, the next change of the schema is a new migration.

use crate::errors::{AppError, IOError};
use rusqlite::{Connection, TransactionBehavior};
use std::{path::Path, time::Duration};

/// The changes of the schema, in order: the `n`-th gives version `n`.
pub const MIGRATIONS: &[&str] = &[];

/// The version of the schema this app writes.
pub fn schema_version() -> i64 {
    version_of(MIGRATIONS)
}

/// How long a write waits for another connection (the app and its
/// background upload, or a script) before giving up.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Opens the database at `path`, creating it if missing, set up and
/// migrated to `schema_version`.
pub fn open(path: &Path) -> Result<Connection, AppError> {
    open_with(path, MIGRATIONS)
}

/// A database in memory, set up and migrated as `open` (the tests).
pub fn open_in_memory() -> Result<Connection, AppError> {
    open_in_memory_with(MIGRATIONS)
}

/// `open` with other migrations (the tests of the mechanism).
pub fn open_with(path: &Path, migrations: &[&str]) -> Result<Connection, AppError> {
    let mut db = Connection::open(path).map_err(sqlite)?;
    configure(&db)?;
    // WAL: the readers don't wait for a writer, and a write is one append
    // to the log; in memory there's no log (the mode stays "memory")
    let mode: String = db
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(sqlite)?;
    match mode.as_str() {
        "wal" => Ok(()),
        other => Err(AppError::IO(IOError::Msg(format!(
            "database journal mode {other}, not wal"
        )))),
    }?;
    migrate(&mut db, migrations)?;
    Ok(db)
}

/// `open_in_memory` with other migrations (the tests of the mechanism).
pub fn open_in_memory_with(migrations: &[&str]) -> Result<Connection, AppError> {
    let mut db = Connection::open_in_memory().map_err(sqlite)?;
    configure(&db)?;
    migrate(&mut db, migrations)?;
    Ok(db)
}

/// The settings of a connection, which SQLite doesn't keep in the file:
/// every connection sets them again.
fn configure(db: &Connection) -> Result<(), AppError> {
    db.busy_timeout(BUSY_TIMEOUT).map_err(sqlite)?;
    // the foreign keys are off by default, on every connection
    // NORMAL in WAL: a commit survives a crash of the app, the last ones may
    // not survive a power cut (the database stays consistent)
    db.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;",
    )
    .map_err(sqlite)
}

/// The version of the schema in the file (`PRAGMA user_version`).
pub fn user_version(db: &Connection) -> Result<i64, AppError> {
    db.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(sqlite)
}

/// Brings the schema from its version to the last of `migrations`, one
/// transaction per migration; returns the versions applied.
pub fn migrate(db: &mut Connection, migrations: &[&str]) -> Result<Vec<i64>, AppError> {
    let known = version_of(migrations);
    let found = user_version(db)?;
    match found {
        found if found > known => Err(AppError::IO(IOError::DatabaseNewer { found, known })),
        found if found < 0 => Err(AppError::IO(IOError::Msg(format!(
            "database schema version {found} not valid"
        )))),
        found => migrations
            .iter()
            .zip(1..)
            .skip_while(|(_, version)| *version <= found)
            .map(|(sql, version)| apply(db, sql, version).map(|()| version))
            .collect(),
    }
}

/// One migration and its version, together or neither. `IMMEDIATE`: the
/// write lock from the start, so two apps opening the same file don't both
/// migrate it (the second waits, then finds the new version and skips).
fn apply(db: &mut Connection, sql: &str, version: i64) -> Result<(), AppError> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sqlite)?;
    match user_version(&tx)? {
        // another connection migrated it meanwhile
        current if current >= version => Ok(()),
        _ => tx
            .execute_batch(sql)
            .and_then(|()| tx.pragma_update(None, "user_version", version))
            .map_err(sqlite),
    }?;
    tx.commit().map_err(sqlite)
}

fn version_of(migrations: &[&str]) -> i64 {
    i64::try_from(migrations.len()).unwrap_or(i64::MAX)
}

fn sqlite(e: rusqlite::Error) -> AppError {
    AppError::IO(IOError::Sqlite(e))
}
