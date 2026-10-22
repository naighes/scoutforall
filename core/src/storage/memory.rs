//! The data in memory, gone with the app: for the tests, or an app that
//! keeps nothing.

use crate::{
    errors::AppError,
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
pub struct MemoryStore(RwLock<BTreeMap<String, Vec<u8>>>);

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
        Ok(blobs.get(key).cloned())
    }

    async fn write(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        let mut blobs = self.0.write().unwrap_or_else(PoisonError::into_inner);
        blobs.insert(key.to_string(), data.to_vec());
        Ok(())
    }

    async fn append(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        let mut blobs = self.0.write().unwrap_or_else(PoisonError::into_inner);
        blobs
            .entry(key.to_string())
            .or_default()
            .extend_from_slice(data);
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, AppError> {
        let blobs = self.0.read().unwrap_or_else(PoisonError::into_inner);
        let prefix = Self::prefix(key);
        Ok(blobs.contains_key(key) || blobs.keys().any(|k| k.starts_with(&prefix)))
    }

    async fn list(&self, folder: &str) -> Result<Vec<StoreEntry>, AppError> {
        let blobs = self.0.read().unwrap_or_else(PoisonError::into_inner);
        let prefix = Self::prefix(folder);
        let entries: BTreeSet<StoreEntry> = blobs
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .map(|rest| match rest.split_once('/') {
                Some((name, _)) => StoreEntry::Folder(name.to_string()),
                None => StoreEntry::Blob(rest.to_string()),
            })
            .collect();
        Ok(entries.into_iter().collect())
    }
}
