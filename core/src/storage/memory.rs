//! The data in memory, gone with the app: for the tests, or an app that
//! keeps nothing.

use crate::{
    errors::{AppError, IOError},
    storage::{Store, StoreEntry},
};
use async_trait::async_trait;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{PoisonError, RwLock},
};

/// The blobs by key. The lock only guards values written at once: they're
/// still good after a panic elsewhere.
#[derive(Debug, Default)]
pub struct MemoryStore(RwLock<Blobs>);

#[derive(Debug, Default)]
struct Blobs {
    by_key: BTreeMap<String, Vec<u8>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn prefix(folder: &str) -> String {
        match folder {
            "" => String::new(),
            _ => format!("{}/", folder),
        }
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn read(&self, key: &str) -> Result<Option<Vec<u8>>, AppError> {
        let blobs = self.0.read().unwrap_or_else(PoisonError::into_inner);
        Ok(blobs.by_key.get(key).cloned())
    }

    async fn write(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        let mut blobs = self.0.write().unwrap_or_else(PoisonError::into_inner);
        blobs.by_key.insert(key.to_string(), data.to_vec());
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, AppError> {
        let blobs = self.0.read().unwrap_or_else(PoisonError::into_inner);
        let prefix = Self::prefix(key);
        Ok(blobs.by_key.contains_key(key) || blobs.by_key.keys().any(|k| k.starts_with(&prefix)))
    }

    async fn list(&self, folder: &str) -> Result<Vec<StoreEntry>, AppError> {
        let blobs = self.0.read().unwrap_or_else(PoisonError::into_inner);
        let prefix = Self::prefix(folder);
        let entries: BTreeSet<StoreEntry> = blobs
            .by_key
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .map(|rest| match rest.split_once('/') {
                Some((name, _)) => StoreEntry::Folder(name.to_string()),
                None => StoreEntry::Blob(rest.to_string()),
            })
            .collect();
        Ok(entries.into_iter().collect())
    }

    async fn remove(&self, key: &str) -> Result<(), AppError> {
        let mut blobs = self.0.write().unwrap_or_else(PoisonError::into_inner);
        blobs.by_key.remove(key);
        Ok(())
    }

    async fn remove_folder(&self, folder: &str) -> Result<(), AppError> {
        if folder.is_empty() {
            return Err(AppError::IO(IOError::Msg(
                "the root can't be removed".into(),
            )));
        }
        let mut blobs = self.0.write().unwrap_or_else(PoisonError::into_inner);
        let prefix = Self::prefix(folder);
        blobs.by_key.retain(|k, _| !k.starts_with(&prefix));
        Ok(())
    }
}
