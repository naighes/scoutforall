//! Where the blobs live: the settings (`config.json`), the clips of the
//! desktop, and the data folder of the versions before the database
//! (`<team>/<match>/set_1.csv`, read by `legacy`), under keys in folders.
//! Who reads them keeps their format (JSON, CSV), a `Store` only their
//! bytes. The keys are in `layout`.

pub mod fs;
pub mod layout;
pub mod memory;

use crate::errors::{AppError, IOError};
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{from_slice, to_vec_pretty};
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

    /// Whether a blob or a folder is under the key.
    async fn exists(&self, key: &str) -> Result<bool, AppError>;

    /// What's right in the folder (`""` is the root), sorted by name: none
    /// when there's no folder.
    async fn list(&self, folder: &str) -> Result<Vec<StoreEntry>, AppError>;

    /// Removes the blob under the key, if any.
    async fn remove(&self, key: &str) -> Result<(), AppError>;

    /// Removes the folder under the key with all it holds, if any: gone at
    /// once for the readers, never half of it. Not the root.
    async fn remove_folder(&self, folder: &str) -> Result<(), AppError>;
}

/// The blob under the key, which must be there.
pub(crate) async fn read_blob(store: &dyn Store, key: &str) -> Result<Vec<u8>, AppError> {
    store
        .read(key)
        .await?
        .ok_or_else(|| AppError::IO(IOError::Msg(format!("not found: {}", key))))
}

/// The JSON under the key, which must be there.
pub(crate) async fn read_json<T: DeserializeOwned>(
    store: &dyn Store,
    key: &str,
) -> Result<T, AppError> {
    from_slice(&read_blob(store, key).await?).map_err(|e| AppError::IO(IOError::from(e)))
}

/// Saves `value` as JSON under the key.
pub(crate) async fn write_json<T: Serialize>(
    store: &dyn Store,
    key: &str,
    value: &T,
) -> Result<(), AppError> {
    let json = to_vec_pretty(value).map_err(|e| AppError::IO(IOError::from(e)))?;
    store.write(key, &json).await
}
