//! The providers over the database (TODO/001, step 7): the interfaces of
//! today, with the rows of `domain` turned into the types of today by its
//! adapters, from A's side.
//!
//! - One connection, shared by the providers behind a mutex: each call
//!   is one transaction, read or written all at once, never awaited in
//!   the middle.
//! - The ids of the matches are their UUIDs as text.
//! - Nothing is removed (D7): a removed team, match, set or event is
//!   marked deleted and no longer read. The roster of a match and the
//!   lineups of a set aren't data of their own: they're replaced.
//! - What the lists show of a match is cached (`summary`), updated in the
//!   transaction of each write.
//! - The video of a match is of this device (`video`): replaced when saved.
//! - A player removed from a team stays in it, deleted, for the matches
//!   that tell of them (roster, lineup, event): the others are no longer
//!   read, as the files leave them out.
//! - D16: a removed team takes with it its matches against a name, the
//!   others stay to the other team, against its name (B becoming A when
//!   it was A); a person removed from the registry (`persons`) keeps only
//!   its id, out of every team, anonymous in the matches played.

pub mod archive;
pub mod legacy_archive;
pub mod matches;
pub mod persons;
pub mod queue;
pub(crate) mod rows;
pub mod sets;
pub(crate) mod summary;
pub mod teams;
pub mod upload;
pub mod video;

use crate::errors::{AppError, IOError};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use std::sync::{Arc, Mutex};

/// The database the providers share.
#[derive(Clone)]
pub struct Database(Arc<Mutex<Connection>>);

impl Database {
    /// `connection` opened by `db::open`: set up and migrated.
    pub fn new(connection: Connection) -> Self {
        Self(Arc::new(Mutex::new(connection)))
    }

    /// Reads in one transaction: what's read is consistent, also with
    /// another connection writing meanwhile.
    pub(crate) fn read<T>(
        &self,
        f: impl FnOnce(&Transaction) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        self.transaction(TransactionBehavior::Deferred, |tx| f(tx))
    }

    /// Writes in one transaction, all or nothing; `IMMEDIATE`: the write
    /// lock from the start, so it doesn't fail halfway on another writer.
    pub(crate) fn write<T>(
        &self,
        f: impl FnOnce(&Transaction) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        self.transaction(TransactionBehavior::Immediate, |tx| f(tx))
    }

    fn transaction<T>(
        &self,
        behavior: TransactionBehavior,
        f: impl FnOnce(&Transaction) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut connection = self
            .0
            .lock()
            .map_err(|_| AppError::IO(IOError::Msg("database lock poisoned".into())))?;
        let tx = connection
            .transaction_with_behavior(behavior)
            .map_err(sqlite)?;
        let value = f(&tx)?;
        tx.commit().map_err(sqlite)?;
        Ok(value)
    }
}

pub(crate) fn sqlite(e: rusqlite::Error) -> AppError {
    AppError::IO(IOError::Sqlite(e))
}

pub(crate) fn not_found(what: &str) -> AppError {
    AppError::IO(IOError::Msg(format!("not found: {what}")))
}
