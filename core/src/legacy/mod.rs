//! The data of the versions before the database (TODO/001), only read:
//! the data folder (`~/.scout4all`, `folder`) and the archives they
//! exported (`archive`), with the rules for what each version left out
//! (`format`); and their rows for the database (`rows`), the same for the
//! migration of the data folder and for the import of the old archives;
//! and the migration at the start (`migrate`), the only one that writes:
//! the rows into the database, then away the folders moved.
//!
//! The providers over the files read through here too, as long as the
//! apps use them.

pub mod archive;
pub mod folder;
pub mod format;
pub mod migrate;
pub mod rows;
