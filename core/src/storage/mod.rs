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
}
