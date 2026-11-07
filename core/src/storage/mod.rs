//! Where the data lives: blobs under keys like `<team>/<match>/set_1.csv`,
//! in folders. The providers keep the format of the data (JSON, CSV, the
//! rules for the older versions), a `Store` only its bytes: a new place to
//! keep the data, e.g. the sandbox of a mobile app, implements this trait
//! alone. The keys are in `layout`.

pub mod fs;
pub mod layout;
pub mod memory;

use crate::errors::AppError;
use async_trait::async_trait;
use std::fmt::Debug;

/// When a blob last changed, to know if what was computed from it still
/// holds: its size, and when it was modified (nanoseconds since 1970 on the
/// file system, a count of the writes in memory).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stamp {
    pub size: u64,
    pub modified: u64,
}

/// The blobs of a folder with their stamps (see `Store::stamps`).
pub type Stamps = Vec<(String, Option<Stamp>)>;

/// What a folder holds: a blob or another folder, by name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum StoreEntry {
    Blob(String),
    Folder(String),
}

#[async_trait]
pub trait Store: Debug + Send + Sync {
    /// The blob under the key, `None` when there's none.
    async fn read(&self, key: &str) -> Result<Option<Vec<u8>>, AppError>;

    /// Saves the blob in place of the one under the key, if any.
    async fn write(&self, key: &str, data: &[u8]) -> Result<(), AppError>;

    /// Adds the data at the end of the blob, which starts empty if missing.
    async fn append(&self, key: &str, data: &[u8]) -> Result<(), AppError>;

    /// Whether a blob or a folder is under the key.
    async fn exists(&self, key: &str) -> Result<bool, AppError>;

    /// What's right in the folder (`""` is the root), sorted by name: none
    /// when there's no folder.
    async fn list(&self, folder: &str) -> Result<Vec<StoreEntry>, AppError>;

    /// Removes the blob under the key, if any.
    async fn remove(&self, key: &str) -> Result<(), AppError>;

    /// The blobs right in the folder, by name, with their stamps: `None`
    /// for a blob that may still change without its stamp telling (just
    /// written, when the clock of the files is coarse). Hidden as `list`.
    async fn stamps(&self, folder: &str) -> Result<Vec<(String, Option<Stamp>)>, AppError>;

    /// The stamps (see `stamps`) of each folder right in the folder, by
    /// name: at once, for a team and all its matches.
    async fn folder_stamps(&self, folder: &str) -> Result<Vec<(String, Stamps)>, AppError>;
}
